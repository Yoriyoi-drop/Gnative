//! Lowering Semantic IR → MIR.
//!
//! `IrModule` diubah menjadi `MirModule` (slot-based, lihat `mir.rs`):
//! fungsi top-level → `MirFunc`; method komponen → `Comp__method`;
//! window → fungsi build `__gn_build_scene` (dipanggil runtime tiap
//! rebuild) + `main` yang registrasi method lalu `gn_window_run_dynamic`.
//!
//! `for (x of [..])` di-unroll menjadi urutan `StoreVar` + body (array
//! literal konstan kecil — wajar untuk DSL UI; `break`/`continue` belum ada).
//! `&&` / `||` memakai short-circuit via `Branch` + variabel sementara.

use std::collections::HashMap;

use crate::ir::{IrBinOp, IrComponent, IrExpr, IrModule, IrNode, IrStmt};
use crate::mir::{MirBinOp, MirCmpOp, MirFunc, MirInst, MirModule, MirTy};

/// Escape `_` pada ident agar separator `__` tak ambigu (injektif).
///
/// Ident `.gn` boleh mengandung `_` (termasuk `__`), sehingga tanpa escape
/// `mono_method_name("A__0__B", 0, "C")` == `mono_method_name("A", 0, "B__0__C")`
/// (tabrakan, audit P1-09). Escape `_`→`_5f` membuat separator `__` unik;
/// nama tanpa `_` (mis. `Counter`/`inc`) tak berubah → `Counter__0__inc`.
fn escape_ident(s: &str) -> String {
    s.replace('_', "_5f")
}

/// Nama fungsi MIR untuk method satu instance
/// (`Counter`, `0`, `inc` → `Counter__0__inc`). Monomorphize agar kunci
/// state bisa disematkan tanpa mengubah ABI (runtime tetap string).
pub fn mono_method_name(comp: &str, inst: usize, method: &str) -> String {
    format!("{}__{inst}__{}", escape_ident(comp), escape_ident(method))
}

/// Nama fungsi build scene (`window.root` di-outline agar runtime bisa
/// memanggilnya ulang setiap rebuild pasca-klik).
pub const BUILD_FUNC: &str = "__gn_build_scene";

/// Kumpulkan semua instance `(komponen, nomor)` di pohon (rekursif, urut).
fn collect_instances(node: &IrNode, out: &mut Vec<(String, usize)>) {
    match node {
        IrNode::ComponentInstance {
            component_name,
            instance,
            render,
            ..
        } => {
            out.push((component_name.clone(), *instance));
            collect_instances(render, out);
        }
        IrNode::VStack { children, .. } => {
            for c in children {
                collect_instances(c, out);
            }
        }
        IrNode::Text { .. } | IrNode::Button { .. } | IrNode::Input { .. } => {}
    }
}

/// Lower seluruh IR menjadi MIR.
pub fn lower_mir(ir: &IrModule) -> MirModule {
    let mut mir = MirModule::default();

    // Peta komponen + tipe state per kunci instance (untuk `gn_state_*`).
    let mut components: HashMap<String, IrComponent> = HashMap::new();
    for comp in &ir.components {
        components.insert(comp.name.clone(), comp.clone());
    }
    let mut instances: Vec<(String, usize)> = Vec::new();
    if let Some(window) = &ir.window {
        collect_instances(&window.root, &mut instances);
    }
    instances.sort();
    instances.dedup();
    let mut state_tys: HashMap<(String, String), MirTy> = HashMap::new();
    for (comp_name, inst) in &instances {
        if let Some(comp) = components.get(comp_name) {
            let key = crate::ir::instance_key(comp_name, *inst);
            for sf in &comp.state_fields {
                state_tys.insert((key.clone(), sf.name.clone()), mir_ty(sf.ty));
            }
        }
    }

    // Signature tipe return (untuk mengetik hasil `Call` saat perbandingan).
    let mut func_ret: HashMap<String, MirTy> = HashMap::new();
    for func in &ir.funcs {
        func_ret.insert(
            func.name.clone(),
            func.ret.map(mir_ty).unwrap_or(MirTy::Void),
        );
    }
    for (comp_name, inst) in &instances {
        if let Some(comp) = components.get(comp_name) {
            for m in &comp.methods {
                func_ret.insert(
                    mono_method_name(comp_name, *inst, &m.name),
                    m.ret.map(mir_ty).unwrap_or(MirTy::Void),
                );
            }
        }
    }

    // Method komponen → salinan `Comp__i__method` per instance
    // (akses state via `gn_state_*` dengan kunci instance).
    for (comp_name, inst) in &instances {
        let Some(comp) = components.get(comp_name) else {
            continue;
        };
        for m in &comp.methods {
            let mut lowerer = Lowerer::new(&mut mir);
            lowerer.func_ret = func_ret.clone();
            lowerer.state_tys = state_tys.clone();
            lowerer.components = components.clone();
            lowerer.current_inst = Some((comp_name.clone(), *inst));
            let mut body = Vec::new();
            for p in m.params.iter() {
                let var = lowerer.fresh_var();
                lowerer.vars.insert(p.name.clone(), var);
                lowerer.var_tys.insert(p.name.clone(), mir_ty(p.ty));
            }
            for stmt in &m.body {
                lowerer.lower_stmt(stmt, &mut body);
            }
            let ret = m.ret.map(mir_ty).unwrap_or(MirTy::Void);
            mir.funcs.push(MirFunc {
                name: mono_method_name(comp_name, *inst, &m.name),
                params: m.params.iter().map(|p| mir_ty(p.ty)).collect(),
                ret,
                body,
            });
        }
    }

    // Fungsi top-level.
    for func in &ir.funcs {
        let mut lowerer = Lowerer::new(&mut mir);
        lowerer.func_ret = func_ret.clone();
        lowerer.state_tys = state_tys.clone();
        lowerer.components = components.clone();
        let mut body = Vec::new();
        // Parameter = variabel lokal yang terisi dari signature.
        for p in func.params.iter() {
            let var = lowerer.fresh_var();
            lowerer.vars.insert(p.name.clone(), var);
            lowerer.var_tys.insert(p.name.clone(), mir_ty(p.ty));
        }
        for stmt in &func.body {
            lowerer.lower_stmt(stmt, &mut body);
        }
        let ret = func.ret.map(mir_ty).unwrap_or(MirTy::Void);
        mir.funcs.push(MirFunc {
            name: func.name.clone(),
            params: func.params.iter().map(|p| mir_ty(p.ty)).collect(),
            ret,
            body,
        });
    }

    // Window → fungsi build `__gn_build_scene` + `main` dinamis.
    //
    // `main`: judul → `WindowCreate`, registrasi method (`gn_register_method`
    // dengan alamat `Comp__m`), lalu `gn_window_run_dynamic(build)`.
    // Runtime memanggil build untuk frame awal dan setiap rebuild
    // pasca-klik (klik → method → state berubah → scene dibangun ulang).
    if let Some(window) = &ir.window {
        // 1. Fungsi build: `gn_scene_create` + rakit pohon UI.
        let mut lowerer = Lowerer::new(&mut mir);
        lowerer.func_ret = func_ret.clone();
        lowerer.state_tys = state_tys.clone();
        lowerer.components = components.clone();
        let mut build_body = Vec::new();
        let scene = lowerer.push(
            MirInst::Call {
                callee: "gn_scene_create".into(),
                args: vec![],
            },
            &mut build_body,
        );
        let root = window.root.clone();
        lowerer.build_scene(&root, scene, &mut build_body);
        lowerer.push(MirInst::Ret(None), &mut build_body);
        mir.funcs.push(MirFunc {
            name: BUILD_FUNC.into(),
            params: vec![],
            ret: MirTy::Void,
            body: build_body,
        });

        // 2. Entry `main`: config + registrasi + run dinamis.
        let mut lowerer = Lowerer::new(&mut mir);
        lowerer.func_ret = func_ret.clone();
        lowerer.state_tys = state_tys.clone();
        lowerer.components = components.clone();
        let mut body = Vec::new();
        let (w, h) = window.size;

        // Judul window (ekspresi string — literal atau hasil Call).
        let title_slot = lowerer.lower_expr(&window.title, &mut body);
        // Panjang judul: diketahui bila literal, `0` (runtime: C-string
        // berbatas) bila dinamis — P2-06, jangan kirim len=0 buta.
        let title_len = match &window.title {
            crate::ir::IrExpr::Str(s) => s.len() as u32,
            _ => 0,
        };
        // Simpan config window (intrinsik runtime wgpu/cuda).
        let _handle = lowerer.push(
            MirInst::WindowCreate {
                title: title_slot,
                title_len,
                width: w as u32,
                height: h as u32,
            },
            &mut body,
        );
        // Registrasi method per instance untuk dispatch klik.
        // `instances` sudah urut; method urut per komponen (deterministik).
        for (comp_name, inst) in &instances {
            let Some(comp) = components.get(comp_name) else {
                continue;
            };
            let key = crate::ir::instance_key(comp_name, *inst);
            let mut method_names: Vec<String> =
                comp.methods.iter().map(|m| m.name.clone()).collect();
            method_names.sort();
            for method in &method_names {
                let (c, f) = lowerer.state_keys(&key, method, &mut body);
                let func = lowerer.push(
                    MirInst::FuncAddr {
                        func: mono_method_name(comp_name, *inst, method),
                    },
                    &mut body,
                );
                lowerer.push(
                    MirInst::Call {
                        callee: "gn_register_method".into(),
                        args: vec![c, f, func],
                    },
                    &mut body,
                );
            }
        }
        // Jalankan event loop dinamis (blocking): build awal + rebuild klik.
        let build = lowerer.push(
            MirInst::FuncAddr {
                func: BUILD_FUNC.into(),
            },
            &mut body,
        );
        lowerer.push(
            MirInst::Call {
                callee: "gn_window_run_dynamic".into(),
                args: vec![build],
            },
            &mut body,
        );

        lowerer.push(MirInst::Ret(None), &mut body);
        mir.funcs.push(MirFunc {
            name: "main".into(),
            params: vec![],
            ret: MirTy::Void,
            body,
        });
    }

    mir
}

/// Konversi tipe IR → tipe MIR.
fn mir_ty(ty: crate::ir::IrTy) -> MirTy {
    match ty {
        crate::ir::IrTy::F64 => MirTy::F64,
        crate::ir::IrTy::Bool => MirTy::Bool,
        crate::ir::IrTy::Str => MirTy::Str,
    }
}

/// Helper lowering: string pool, variabel lokal, counter label.
struct Lowerer<'a> {
    mir: &'a mut MirModule,
    vars: HashMap<String, u32>,
    /// Tipe tiap variabel lokal (untuk memilih instruksi perbandingan).
    var_tys: HashMap<String, MirTy>,
    /// Tipe return fungsi (untuk mengetik hasil `Call`).
    func_ret: HashMap<String, MirTy>,
    /// Tipe state `(kunci instance, field)` → MIR.
    state_tys: HashMap<(String, String), MirTy>,
    /// Komponen stateful (untuk init state init per instance).
    components: HashMap<String, IrComponent>,
    /// Instance yang sedang di-lower `(komponen, nomor)` (method / inline render).
    current_inst: Option<(String, usize)>,
    next_var: u32,
    next_label: u32,
    /// Index string pool → intern O(1) (P2-06; sebelumnya scan linier O(n²)).
    interner: HashMap<String, usize>,
}

impl<'a> Lowerer<'a> {
    fn new(mir: &'a mut MirModule) -> Self {
        // Bangun ulang index dari pool yang ada (dipakai lintas fungsi).
        let mut interner = HashMap::new();
        for (i, s) in mir.strings.iter().enumerate() {
            interner.insert(s.clone(), i);
        }
        Lowerer {
            mir,
            vars: HashMap::new(),
            var_tys: HashMap::new(),
            func_ret: HashMap::new(),
            state_tys: HashMap::new(),
            components: HashMap::new(),
            current_inst: None,
            next_var: 0,
            next_label: 0,
            interner,
        }
    }

    /// Kunci instance aktif (`instance_key`) untuk akses `gn_state_*`.
    fn active_key(&self) -> String {
        match &self.current_inst {
            Some((c, i)) => crate::ir::instance_key(c, *i),
            None => String::new(),
        }
    }

    /// Nama callee + tipe untuk akses state baca (`key` = kunci instance).
    fn state_getter(&self, key: &str, field: &str) -> (&'static str, MirTy) {
        match self.state_tys.get(&(key.to_string(), field.to_string())) {
            Some(MirTy::Str) => ("gn_state_get_str", MirTy::Str),
            Some(MirTy::Bool) => ("gn_state_get_bool", MirTy::Bool),
            _ => ("gn_state_get_f64", MirTy::F64),
        }
    }

    /// Nama callee untuk tulis state (`key` = kunci instance).
    fn state_setter(&self, key: &str, field: &str) -> &'static str {
        match self.state_tys.get(&(key.to_string(), field.to_string())) {
            Some(MirTy::Str) => "gn_state_set_str",
            Some(MirTy::Bool) => "gn_state_set_bool",
            _ => "gn_state_set_f64",
        }
    }

    /// Emit `StrAddr` untuk kunci instance + field; kembalikan slot keduanya.
    fn state_keys(&mut self, key: &str, field: &str, body: &mut Vec<MirInst>) -> (u32, u32) {
        let ci = self.intern(key);
        let fi = self.intern(field);
        let c = self.push(MirInst::StrAddr(ci), body);
        let f = self.push(MirInst::StrAddr(fi), body);
        (c, f)
    }

    /// Masukkan string ke pool; kembalikan index.
    fn intern(&mut self, s: &str) -> usize {
        if let Some(&i) = self.interner.get(s) {
            return i;
        }
        let i = self.mir.strings.len();
        self.mir.strings.push(s.to_string());
        self.interner.insert(s.to_string(), i);
        i
    }

    /// Push instruksi; kembalikan slot (index di body).
    fn push(&mut self, inst: MirInst, body: &mut Vec<MirInst>) -> u32 {
        let slot = body.len() as u32;
        body.push(inst);
        slot
    }

    /// Variabel lokal baru.
    fn fresh_var(&mut self) -> u32 {
        let v = self.next_var;
        self.next_var += 1;
        v
    }

    /// Label CFG baru.
    fn fresh_label(&mut self) -> u32 {
        let l = self.next_label;
        self.next_label += 1;
        l
    }

    /// Cetak string literal (intrinsik `puts`).
    fn putstr(&mut self, s: &str, body: &mut Vec<MirInst>) {
        let idx = self.intern(s);
        let ptr = self.push(MirInst::StrAddr(idx), body);
        self.push(
            MirInst::Call {
                callee: "puts".into(),
                args: vec![ptr],
            },
            body,
        );
    }

    /// Lower satu statement → instruksi.
    fn lower_stmt(&mut self, s: &IrStmt, body: &mut Vec<MirInst>) {
        match s {
            IrStmt::Let { name, ty, value } => {
                let var = self.fresh_var();
                self.vars.insert(name.clone(), var);
                if let Some(t) = ty {
                    self.var_tys.insert(name.clone(), mir_ty(*t));
                }
                let val = match value {
                    Some(e) => self.lower_expr(e, body),
                    None => self.push(MirInst::ConstF64(0.0), body),
                };
                self.push(MirInst::StoreVar { var, val }, body);
            }
            IrStmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let c = self.lower_expr(cond, body);
                let then = self.fresh_label();
                let els = self.fresh_label();
                let merge = self.fresh_label();
                self.push(MirInst::Branch { cond: c, then, els }, body);
                self.push(MirInst::Label(then), body);
                for stmt in then_branch {
                    self.lower_stmt(stmt, body);
                }
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(els), body);
                for stmt in else_branch {
                    self.lower_stmt(stmt, body);
                }
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(merge), body);
            }
            // `for (x of [item, ...])` — unroll penuh (array literal konstan).
            IrStmt::For {
                var,
                items,
                body: loop_body,
            } => {
                let v = self.fresh_var();
                self.vars.insert(var.clone(), v);
                for item in items {
                    let slot = self.lower_expr(item, body);
                    self.push(MirInst::StoreVar { var: v, val: slot }, body);
                    for stmt in loop_body {
                        self.lower_stmt(stmt, body);
                    }
                }
            }
            IrStmt::Return(expr) => {
                let slot = expr.as_ref().map(|e| self.lower_expr(e, body));
                self.push(MirInst::Ret(slot), body);
            }
            IrStmt::Expr(e) => {
                self.lower_expr(e, body);
            }
            IrStmt::PutStr(s) => {
                self.putstr(s, body);
            }
        }
    }

    /// Tipe statis sebuah ekspresi (untuk memilih instruksi perbandingan).
    /// Default: `F64` bila tidak diketahui (JS-like).
    fn expr_ty(&self, e: &IrExpr) -> MirTy {
        match e {
            IrExpr::F64(_) => MirTy::F64,
            IrExpr::Bool(_) => MirTy::Bool,
            IrExpr::Str(_) => MirTy::Str,
            IrExpr::Var(name) => self.var_tys.get(name).copied().unwrap_or(MirTy::F64),
            IrExpr::StateField(name) => match &self.current_inst {
                Some((c, i)) => self
                    .state_tys
                    .get(&(crate::ir::instance_key(c, *i), name.clone()))
                    .copied()
                    .unwrap_or(MirTy::F64),
                None => MirTy::F64,
            },
            IrExpr::Neg(_) => MirTy::F64,
            IrExpr::Not(_) => MirTy::Bool,
            IrExpr::Bin(op, ..) => match op {
                IrBinOp::Add | IrBinOp::Sub | IrBinOp::Mul | IrBinOp::Div => MirTy::F64,
                _ => MirTy::Bool,
            },
            IrExpr::Call(name, _) => self.func_ret.get(name).copied().unwrap_or(MirTy::F64),
            IrExpr::MethodCall { method, .. } => match &self.current_inst {
                Some((c, i)) => self
                    .func_ret
                    .get(&mono_method_name(c, *i, method))
                    .copied()
                    .unwrap_or(MirTy::F64),
                None => MirTy::F64,
            },
            IrExpr::Assign(name, _) => {
                if let Some(t) = self.var_tys.get(name) {
                    *t
                } else if let Some((c, i)) = &self.current_inst {
                    self.state_tys
                        .get(&(crate::ir::instance_key(c, *i), name.clone()))
                        .copied()
                        .unwrap_or(MirTy::F64)
                } else {
                    MirTy::F64
                }
            }
        }
    }

    /// Lower satu ekspresi → slot hasil.
    fn lower_expr(&mut self, e: &IrExpr, body: &mut Vec<MirInst>) -> u32 {
        match e {
            IrExpr::F64(n) => self.push(MirInst::ConstF64(*n), body),
            IrExpr::Bool(b) => self.push(MirInst::ConstBool(*b), body),
            IrExpr::Str(s) => {
                let idx = self.intern(s);
                self.push(MirInst::StrAddr(idx), body)
            }
            IrExpr::Var(name) => {
                let var = *self.vars.get(name).unwrap_or(&0);
                self.push(MirInst::LoadVar(var), body)
            }
            // Akses state (`this.field` / bare field di method): baca dari
            // `state_store` runtime (`gn_state_get_*`) dengan kunci instance.
            IrExpr::StateField(name) => {
                let key = self.active_key();
                let (callee, _) = self.state_getter(&key, name);
                let callee = callee.to_string();
                let (c, f) = self.state_keys(&key, name, body);
                self.push(
                    MirInst::Call {
                        callee,
                        args: vec![c, f],
                    },
                    body,
                )
            }
            IrExpr::Neg(x) => {
                let a = self.lower_expr(x, body);
                let zero = self.push(MirInst::ConstF64(0.0), body);
                self.push(
                    MirInst::BinF64 {
                        op: MirBinOp::Sub,
                        a: zero,
                        b: a,
                    },
                    body,
                )
            }
            IrExpr::Not(x) => {
                let a = self.lower_expr(x, body);
                self.push(MirInst::Not(a), body)
            }
            IrExpr::Bin(op, l, r) => self.lower_bin(*op, l, r, body),
            IrExpr::Call(name, args) => {
                let mut arg_slots = Vec::new();
                for a in args {
                    arg_slots.push(self.lower_expr(a, body));
                }
                self.push(
                    MirInst::Call {
                        callee: name.clone(),
                        args: arg_slots,
                    },
                    body,
                )
            }
            IrExpr::Assign(name, value) => {
                let val = self.lower_expr(value, body);
                //shadowing lokal menang; else state bila field dikenal.
                let is_local = self.vars.contains_key(name);
                let is_state = match &self.current_inst {
                    Some((c, i)) => self
                        .state_tys
                        .contains_key(&(crate::ir::instance_key(c, *i), name.clone())),
                    None => false,
                };
                if !is_local && is_state {
                    let key = self.active_key();
                    let callee = self.state_setter(&key, name).to_string();
                    let (c, f) = self.state_keys(&key, name, body);
                    self.push(
                        MirInst::Call {
                            callee,
                            args: vec![c, f, val],
                        },
                        body,
                    );
                } else {
                    let var = *self.vars.get(name).unwrap_or(&0);
                    self.push(MirInst::StoreVar { var, val }, body);
                }
                val // hasil assignment = nilai yang disimpan
            }
            // Panggil method (`this.m(...)` → `Comp__i__m(...)` instance sama).
            IrExpr::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.lower_expr(receiver, body);
                let mut arg_slots = Vec::new();
                for a in args {
                    arg_slots.push(self.lower_expr(a, body));
                }
                let callee = match &self.current_inst {
                    Some((c, i)) => mono_method_name(c, *i, method),
                    None => method.clone(),
                };
                self.push(
                    MirInst::Call {
                        callee,
                        args: arg_slots,
                    },
                    body,
                )
            }
        }
    }

    /// Lower operator biner; `&&`/`||` memakai short-circuit.
    fn lower_bin(&mut self, op: IrBinOp, l: &IrExpr, r: &IrExpr, body: &mut Vec<MirInst>) -> u32 {
        match op {
            IrBinOp::Add | IrBinOp::Sub | IrBinOp::Mul | IrBinOp::Div => {
                let a = self.lower_expr(l, body);
                let b = self.lower_expr(r, body);
                let mop = match op {
                    IrBinOp::Add => MirBinOp::Add,
                    IrBinOp::Sub => MirBinOp::Sub,
                    IrBinOp::Mul => MirBinOp::Mul,
                    IrBinOp::Div => MirBinOp::Div,
                    _ => unreachable!(),
                };
                self.push(MirInst::BinF64 { op: mop, a, b }, body)
            }
            IrBinOp::Eq | IrBinOp::Ne | IrBinOp::Lt | IrBinOp::Le | IrBinOp::Gt | IrBinOp::Ge => {
                let a = self.lower_expr(l, body);
                let b = self.lower_expr(r, body);
                let cop = match op {
                    IrBinOp::Eq => MirCmpOp::Eq,
                    IrBinOp::Ne => MirCmpOp::Ne,
                    IrBinOp::Lt => MirCmpOp::Lt,
                    IrBinOp::Le => MirCmpOp::Le,
                    IrBinOp::Gt => MirCmpOp::Gt,
                    IrBinOp::Ge => MirCmpOp::Ge,
                    _ => unreachable!(),
                };
                // Pilih instruksi sesuai tipe operan: string & bool hanya
                // mendukung `==`/`!=`; angka mendukung semua perbandingan.
                let lt = self.expr_ty(l);
                let rt = self.expr_ty(r);
                if lt == MirTy::Str || rt == MirTy::Str {
                    self.push(MirInst::CmpStr { op: cop, a, b }, body)
                } else if lt == MirTy::Bool || rt == MirTy::Bool {
                    self.push(MirInst::CmpBool { op: cop, a, b }, body)
                } else {
                    self.push(MirInst::CmpF64 { op: cop, a, b }, body)
                }
            }
            IrBinOp::And => {
                // a && b → a ? b : false (short-circuit)
                let a = self.lower_expr(l, body);
                let then = self.fresh_label();
                let els = self.fresh_label();
                let merge = self.fresh_label();
                let res = self.fresh_var();
                self.push(MirInst::Branch { cond: a, then, els }, body);
                self.push(MirInst::Label(then), body);
                let b = self.lower_expr(r, body);
                self.push(MirInst::StoreVar { var: res, val: b }, body);
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(els), body);
                let f = self.push(MirInst::ConstBool(false), body);
                self.push(MirInst::StoreVar { var: res, val: f }, body);
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(merge), body);
                self.push(MirInst::LoadVar(res), body)
            }
            IrBinOp::Or => {
                // a || b → a ? true : b (short-circuit)
                let a = self.lower_expr(l, body);
                let then = self.fresh_label();
                let els = self.fresh_label();
                let merge = self.fresh_label();
                let res = self.fresh_var();
                self.push(MirInst::Branch { cond: a, then, els }, body);
                self.push(MirInst::Label(then), body);
                let t = self.push(MirInst::ConstBool(true), body);
                self.push(MirInst::StoreVar { var: res, val: t }, body);
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(els), body);
                let b = self.lower_expr(r, body);
                self.push(MirInst::StoreVar { var: res, val: b }, body);
                self.push(MirInst::Jump(merge), body);
                self.push(MirInst::Label(merge), body);
                self.push(MirInst::LoadVar(res), body)
            }
        }
    }

    /// Bangun Scene dari pohon `IrNode` via intrinsik runtime.
    ///
    /// Setiap node diterjemahkan menjadi pemanggilan builder C-ABI
    /// (`gn_scene_add_*`, `gn_scene_begin/end_vstack`) yang dirakit sisi
    /// runtime menjadi `gnative_gfx::Scene` lalu dirender.
    /// `ComponentInstance` di-inline: init state (`gn_state_set_*`) lalu
    /// render_node komponen (baca state via `gn_state_get_*`).
    /// Tombol ber-`onClick` memakai `gn_scene_add_button_clickable`
    /// (runtime mencatat aksi untuk dispatch klik + rebuild).
    fn build_scene(&mut self, node: &IrNode, scene: u32, body: &mut Vec<MirInst>) {
        match node {
            IrNode::Text { content, size } => {
                let ptr = self.lower_expr(content, body);
                let size_slot = self.push(MirInst::ConstF64(*size), body);
                self.push(
                    MirInst::Call {
                        callee: "gn_scene_add_text".into(),
                        args: vec![scene, ptr, size_slot],
                    },
                    body,
                );
            }
            IrNode::Button { label, on_click } => {
                let ptr = self.lower_expr(label, body);
                match on_click {
                    None => {
                        self.push(
                            MirInst::Call {
                                callee: "gn_scene_add_button".into(),
                                args: vec![scene, ptr],
                            },
                            body,
                        );
                    }
                    Some(click) => {
                        let (c, f) = self.state_keys(&click.component, &click.method, body);
                        self.push(
                            MirInst::Call {
                                callee: "gn_scene_add_button_clickable".into(),
                                args: vec![scene, ptr, c, f],
                            },
                            body,
                        );
                    }
                }
            }
            IrNode::Input { placeholder } => {
                let ptr = self.lower_expr(placeholder, body);
                self.push(
                    MirInst::Call {
                        callee: "gn_scene_add_input".into(),
                        args: vec![scene, ptr],
                    },
                    body,
                );
            }
            IrNode::VStack { gap, children } => {
                let gap_slot = self.push(MirInst::ConstF64(*gap), body);
                self.push(
                    MirInst::Call {
                        callee: "gn_scene_begin_vstack".into(),
                        args: vec![scene, gap_slot],
                    },
                    body,
                );
                for child in children {
                    self.build_scene(child, scene, body);
                }
                self.push(
                    MirInst::Call {
                        callee: "gn_scene_end_vstack".into(),
                        args: vec![scene],
                    },
                    body,
                );
            }
            // Instance komponen stateful: init state (kunci instance) lalu
            // inline render milik situs ini (sudah disubstitusi props).
            IrNode::ComponentInstance {
                component_name,
                instance,
                props: _,
                render,
            } => {
                let comp = match self.components.get(component_name) {
                    Some(c) => c.clone(),
                    None => return,
                };
                let key = crate::ir::instance_key(component_name, *instance);
                for sf in &comp.state_fields {
                    let val = self.lower_expr(&sf.init, body);
                    let setter = self.state_setter(&key, &sf.name).to_string();
                    let (c, f) = self.state_keys(&key, &sf.name, body);
                    self.push(
                        MirInst::Call {
                            callee: setter,
                            args: vec![c, f, val],
                        },
                        body,
                    );
                }
                let prev = self.current_inst.clone();
                self.current_inst = Some((component_name.clone(), *instance));
                let render = render.clone();
                self.build_scene(&render, scene, body);
                self.current_inst = prev;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_method_name_injektif() {
        // P1-09: dua triple berbeda yang sebelumnya bertabrakan kini berbeda.
        let a = mono_method_name("A__0__B", 0, "C");
        let b = mono_method_name("A", 0, "B__0__C");
        assert_ne!(a, b, "mangling harus injektif: {a} vs {b}");
        // Nama tanpa `_` tak berubah (kompatibel ABI lama).
        assert_eq!(mono_method_name("Counter", 0, "inc"), "Counter__0__inc");
        // Umum: semua kombinasi unik.
        let names = [
            mono_method_name("A", 0, "x"),
            mono_method_name("A_", 0, "x"),
            mono_method_name("A", 0, "x_"),
            mono_method_name("A__x", 0, ""),
        ];
        let set: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(set.len(), names.len());
    }

    #[test]
    fn window_becomes_main() {
        let ir = crate::ir::IrModule {
            funcs: vec![],
            components: vec![],
            window: Some(crate::ir::IrWindow {
                title: IrExpr::Str("Halo".into()),
                size: (800.0, 600.0),
                root: IrNode::VStack {
                    gap: 8.0,
                    children: vec![IrNode::Text {
                        content: IrExpr::Str("Hi".into()),
                        size: 32.0,
                    }],
                },
            }),
        };
        let mir = lower_mir(&ir);
        // Window di-outline: build fn merakit scene, main menjalankan dinamis.
        let build = mir
            .funcs
            .iter()
            .find(|f| f.name == crate::mir_lower::BUILD_FUNC)
            .expect("build fn ada");
        assert!(matches!(
            build.body[0],
            MirInst::Call { ref callee, .. } if callee == "gn_scene_create"
        ));
        assert!(matches!(build.body.last(), Some(MirInst::Ret(None))));
        let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        assert!(matches!(main.body.last(), Some(MirInst::Ret(None))));
        // Judul window di-lower ke StrAddr di suatu titik (main).
        let has_title = main.body.iter().any(|i| matches!(i, MirInst::StrAddr(_)));
        let has_create = build
            .body
            .iter()
            .any(|i| matches!(i, MirInst::Call { ref callee, .. } if callee == "gn_scene_create"));
        let has_run = main.body.iter().any(
            |i| matches!(i, MirInst::Call { ref callee, .. } if callee == "gn_window_run_dynamic"),
        );
        assert!(
            has_title && has_create && has_run,
            "harus ada title, scene & run"
        );
    }

    #[test]
    fn function_with_if_and_arithmetic() {
        let ir = crate::ir::IrModule {
            funcs: vec![crate::ir::IrFunc {
                name: "max".into(),
                params: vec![crate::ir::IrParam {
                    name: "a".into(),
                    ty: crate::ir::IrTy::F64,
                }],
                ret: Some(crate::ir::IrTy::F64),
                body: vec![IrStmt::If {
                    cond: IrExpr::Bin(
                        IrBinOp::Eq,
                        Box::new(IrExpr::Var("a".into())),
                        Box::new(IrExpr::F64(0.0)),
                    ),
                    then_branch: vec![IrStmt::Return(Some(IrExpr::F64(1.0)))],
                    else_branch: vec![IrStmt::Return(Some(IrExpr::Bin(
                        IrBinOp::Add,
                        Box::new(IrExpr::Var("a".into())),
                        Box::new(IrExpr::F64(1.0)),
                    )))],
                }],
            }],
            components: vec![],
            window: None,
        };
        let mir = lower_mir(&ir);
        assert_eq!(mir.funcs.len(), 1);
        let f = &mir.funcs[0];
        assert_eq!(f.params, vec![MirTy::F64]);
        assert_eq!(f.ret, MirTy::F64);
        // Branch + Label(then) + ret + Jump + Label(els) + ret + Jump + Label(merge)
        assert!(f.body.iter().any(|i| matches!(i, MirInst::Branch { .. })));
        assert!(f.body.iter().any(|i| matches!(i, MirInst::CmpF64 { .. })));
    }

    #[test]
    fn for_loop_unrolls() {
        let ir = crate::ir::IrModule {
            funcs: vec![crate::ir::IrFunc {
                name: "sum".into(),
                params: vec![],
                ret: Some(crate::ir::IrTy::F64),
                body: vec![
                    IrStmt::Let {
                        name: "t".into(),
                        ty: Some(crate::ir::IrTy::F64),
                        value: Some(IrExpr::F64(0.0)),
                    },
                    IrStmt::For {
                        var: "n".into(),
                        items: vec![IrExpr::F64(1.0), IrExpr::F64(2.0), IrExpr::F64(3.0)],
                        body: vec![IrStmt::Expr(IrExpr::Assign(
                            "t".into(),
                            Box::new(IrExpr::Bin(
                                IrBinOp::Add,
                                Box::new(IrExpr::Var("t".into())),
                                Box::new(IrExpr::Var("n".into())),
                            )),
                        ))],
                    },
                    IrStmt::Return(Some(IrExpr::Var("t".into()))),
                ],
            }],
            components: vec![],
            window: None,
        };
        let mir = lower_mir(&ir);
        let f = &mir.funcs[0];
        // let(2) + 3x (const + store + load-load + bin + store) + load + ret
        let stores = f
            .body
            .iter()
            .filter(|i| matches!(i, MirInst::StoreVar { .. }))
            .count();
        assert!(
            stores >= 4,
            "harus ada StoreVar let + 3x loop, dapat {stores}"
        );
        assert!(matches!(f.body.last(), Some(MirInst::Ret(Some(_)))));
    }

    #[test]
    fn string_pool_dedup() {
        let mut mir = MirModule::default();
        let mut l = Lowerer::new(&mut mir);
        let _ = l.intern("a");
        let _ = l.intern("b");
        let _ = l.intern("a");
        assert_eq!(mir.strings, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn stateful_method_becomes_mangled_func() {
        // Method instance 1 `Counter` → `Counter__1__inc` berkunci `Counter#1`.
        let inc_body = vec![IrStmt::Expr(IrExpr::Assign(
            "count".into(),
            Box::new(IrExpr::Bin(
                IrBinOp::Add,
                Box::new(IrExpr::StateField("count".into())),
                Box::new(IrExpr::F64(1.0)),
            )),
        ))];
        let comp = || crate::ir::IrComponent {
            name: "Counter".into(),
            state_fields: vec![crate::ir::IrStateField {
                name: "count".into(),
                ty: crate::ir::IrTy::F64,
                init: IrExpr::F64(0.0),
            }],
            methods: vec![crate::ir::IrFunc {
                name: "inc".into(),
                params: vec![],
                ret: None,
                body: inc_body.clone(),
            }],
        };
        let instance = |n: usize| IrNode::ComponentInstance {
            component_name: "Counter".into(),
            instance: n,
            props: vec![],
            render: Box::new(IrNode::Text {
                content: IrExpr::Str("x".into()),
                size: 14.0,
            }),
        };
        let ir = crate::ir::IrModule {
            funcs: vec![],
            components: vec![comp()],
            window: Some(crate::ir::IrWindow {
                title: IrExpr::Str("T".into()),
                size: (800.0, 600.0),
                root: IrNode::VStack {
                    gap: 0.0,
                    children: vec![instance(0), instance(3)],
                },
            }),
        };
        let mir = lower_mir(&ir);
        for n in [0, 3] {
            let f = mir
                .funcs
                .iter()
                .find(|f| f.name == format!("Counter__{n}__inc"))
                .expect("method ada");
            assert!(
                f.body.iter().any(
                    |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_state_get_f64")
                ),
                "baca state: {:?}",
                f.body
            );
            assert!(
                f.body.iter().any(
                    |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_state_set_f64")
                ),
                "tulis state: {:?}",
                f.body
            );
        }
        // Kunci instance berbeda di pool string.
        for key in ["Counter#0", "Counter#3"] {
            assert!(
                mir.strings.iter().any(|s| s == key),
                "kunci {key} ada: {:?}",
                mir.strings
            );
        }
    }

    #[test]
    fn component_instance_inlines_init_and_render() {
        // `<Counter/>` di window → init state + `gn_scene_add_text`.
        let ir = crate::ir::IrModule {
            funcs: vec![],
            components: vec![crate::ir::IrComponent {
                name: "Counter".into(),
                state_fields: vec![crate::ir::IrStateField {
                    name: "count".into(),
                    ty: crate::ir::IrTy::F64,
                    init: IrExpr::F64(5.0),
                }],
                methods: vec![],
            }],
            window: Some(crate::ir::IrWindow {
                title: IrExpr::Str("T".into()),
                size: (800.0, 600.0),
                root: IrNode::ComponentInstance {
                    component_name: "Counter".into(),
                    instance: 0,
                    props: vec![],
                    render: Box::new(IrNode::Text {
                        content: IrExpr::Str("x".into()),
                        size: 14.0,
                    }),
                },
            }),
        };
        let mir = lower_mir(&ir);
        // Init + render di-inline di fungsi build (bukan main).
        let build = mir
            .funcs
            .iter()
            .find(|f| f.name == crate::mir_lower::BUILD_FUNC)
            .expect("build fn ada");
        assert!(
            build
                .body
                .iter()
                .any(|i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_state_set_f64")),
            "init state: {:?}",
            build.body
        );
        assert!(
            build.body.iter().any(
                |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_scene_add_text")
            ),
            "render inline: {:?}",
            build.body
        );
        // Main meregistrasi (tanpa method → hanya run dinamis).
        let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        assert!(
            main.body.iter().any(
                |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_window_run_dynamic")
            ),
            "run dinamis: {:?}",
            main.body
        );
    }

    #[test]
    fn clickable_button_emits_handler_refs() {
        // Tombol ber-onClick → `gn_scene_add_button_clickable` + registrasi.
        let ir = crate::ir::IrModule {
            funcs: vec![],
            components: vec![crate::ir::IrComponent {
                name: "Counter".into(),
                state_fields: vec![crate::ir::IrStateField {
                    name: "count".into(),
                    ty: crate::ir::IrTy::F64,
                    init: IrExpr::F64(0.0),
                }],
                methods: vec![crate::ir::IrFunc {
                    name: "inc".into(),
                    params: vec![],
                    ret: None,
                    body: vec![],
                }],
            }],
            window: Some(crate::ir::IrWindow {
                title: IrExpr::Str("T".into()),
                size: (800.0, 600.0),
                root: IrNode::ComponentInstance {
                    component_name: "Counter".into(),
                    instance: 0,
                    props: vec![],
                    render: Box::new(IrNode::Button {
                        label: IrExpr::Str("+".into()),
                        on_click: Some(crate::ir::IrClick {
                            component: crate::ir::instance_key("Counter", 0),
                            method: "inc".into(),
                        }),
                    }),
                },
            }),
        };
        let mir = lower_mir(&ir);
        let build = mir
            .funcs
            .iter()
            .find(|f| f.name == crate::mir_lower::BUILD_FUNC)
            .expect("build fn ada");
        assert!(
            build.body.iter().any(
                |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_scene_add_button_clickable")
            ),
            "clickable: {:?}",
            build.body
        );
        let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
        assert!(
            main.body.iter().any(
                |i| matches!(i, MirInst::Call { callee, .. } if callee == "gn_register_method")
            ),
            "registrasi: {:?}",
            main.body
        );
        assert!(
            main.body
                .iter()
                .any(|i| matches!(i, MirInst::FuncAddr { .. })),
            "alamat fungsi: {:?}",
            main.body
        );
    }
}
