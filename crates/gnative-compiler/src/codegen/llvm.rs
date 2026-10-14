//! Backend LLVM (mode release) — MIR → object file native (bagian 1/4).
//!
//! Memakai inkwell atas LLVM 18 sistem (optimasi agresif). Cerminan backend
//! Cranelift: slot MIR → alloca entry-block, CFG Label/Jump/Branch →
//! basic block, intrinsik runtime di-declare (di-resolve saat link).

use std::collections::HashMap;
use std::path::Path;

use gnative_core::{GnError, GnResult};
use inkwell::{
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicMetadataTypeEnum, BasicTypeEnum, FunctionType},
    values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue},
    AddressSpace, FloatPredicate, IntPredicate,
};

use super::CodegenBackend;
use crate::mir::{MirBinOp, MirCmpOp, MirFunc, MirInst, MirModule, MirTy};

/// Backend LLVM — release build.
pub struct LlvmBackend;

/// Jenis slot internal (termasuk mentah i32/i64 untuk handle runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotKind {
    F64,
    I8,
    Ptr,
    I32,
    I64,
}

impl CodegenBackend for LlvmBackend {
    fn name(&self) -> &'static str {
        "llvm"
    }

    fn codegen(&self, mir: &MirModule, target: Option<&str>, out: &Path) -> GnResult<()> {
        Target::initialize_all(&InitializationConfig::default());

        // 1. Target machine: `--target <triple>` → cross; `None` → host.
        let level = inkwell::OptimizationLevel::Aggressive;
        let (triple, machine) = match target {
            Some(t) => {
                let triple = inkwell::targets::TargetTriple::create(t);
                let tg = Target::from_triple(&triple).map_err(|e| {
                    GnError::Other(format!("target `{t}` tidak didukung LLVM: {e}"))
                })?;
                let m = tg
                    .create_target_machine(
                        &triple,
                        "generic",
                        "",
                        level,
                        RelocMode::PIC,
                        CodeModel::Default,
                    )
                    .ok_or_else(|| GnError::Other(format!("mesin target `{t}` gagal dibuat")))?;
                (triple, m)
            }
            None => {
                let triple = TargetMachine::get_default_triple();
                let tg = Target::from_triple(&triple)
                    .map_err(|e| GnError::Other(format!("target host gagal: {e}")))?;
                let m = tg
                    .create_target_machine(
                        &triple,
                        "generic",
                        "",
                        level,
                        RelocMode::PIC,
                        CodeModel::Default,
                    )
                    .ok_or_else(|| GnError::Other("mesin target host gagal dibuat".into()))?;
                (triple, m)
            }
        };

        // 2. Modul + builder.
        let context = Context::create();
        let module = context.create_module("gnative");
        module.set_triple(&triple);
        module.set_data_layout(&machine.get_target_data().get_data_layout());
        let builder = context.create_builder();
        let mut gen = ModuleGen {
            context: &context,
            module: &module,
            builder,
            f64_ty: context.f64_type().into(),
            i8_ty: context.i8_type().into(),
            ptr_ty: context.ptr_type(AddressSpace::default()).into(),
            i32_ty: context.i32_type().into(),
            i64_ty: context.i64_type().into(),
            funcs: HashMap::new(),
            runtime: HashMap::new(),
            runtime_ret: HashMap::new(),
            strings: Vec::new(),
            sigs: HashMap::new(),
        };
        gen.declare_runtime()?;
        gen.declare_strings(mir)?;

        // 3. Signature fungsi user (untuk call + definisi).
        for f in &mir.funcs {
            gen.sigs.insert(f.name.clone(), (f.params.clone(), f.ret));
        }

        // 4. Deklarasi semua fungsi (External) — call bisa maju.
        for f in &mir.funcs {
            let ty = gen.fn_type(&f.params, f.ret);
            let fv = gen
                .module
                .add_function(&f.name, ty, Some(Linkage::External));
            gen.funcs.insert(f.name.clone(), fv);
        }

        // 5. Compile body tiap fungsi.
        for f in &mir.funcs {
            gen.compile_function(f)?;
        }

        // 6. Verifikasi + tulis object file.
        module
            .verify()
            .map_err(|e| GnError::Other(format!("verifikasi LLVM gagal: {e}")))?;
        machine
            .write_to_file(&module, FileType::Object, out)
            .map_err(|e| GnError::Other(format!("menulis object gagal: {e}")))?;
        Ok(())
    }
}

/// Generator satu modul LLVM.
struct ModuleGen<'ctx, 'm> {
    context: &'ctx Context,
    module: &'m Module<'ctx>,
    builder: Builder<'ctx>,
    f64_ty: BasicTypeEnum<'ctx>,
    i8_ty: BasicTypeEnum<'ctx>,
    ptr_ty: BasicTypeEnum<'ctx>,
    i32_ty: BasicTypeEnum<'ctx>,
    i64_ty: BasicTypeEnum<'ctx>,
    /// Nama fungsi user → deklarasi.
    funcs: HashMap<String, FunctionValue<'ctx>>,
    /// Nama intrinsik runtime → deklarasi.
    runtime: HashMap<String, FunctionValue<'ctx>>,
    /// Intrinsik runtime → jenis return (untuk hasil `Call`).
    runtime_ret: HashMap<String, SlotKind>,
    /// String pool → global konstan.
    strings: Vec<PointerValue<'ctx>>,
    /// Nama fungsi user → (params, ret).
    sigs: HashMap<String, (Vec<MirTy>, MirTy)>,
}

impl<'ctx, 'm> ModuleGen<'ctx, 'm> {
    /// Tipe ABI LLVM untuk tipe MIR.
    fn abi_ty(&self, ty: MirTy) -> BasicTypeEnum<'ctx> {
        match ty {
            MirTy::F64 => self.f64_ty,
            MirTy::Bool => self.i8_ty,
            MirTy::Str | MirTy::Ptr => self.ptr_ty,
            MirTy::Void => self.i32_ty, // tidak dipakai sebagai nilai
        }
    }

    /// Tipe LLVM untuk jenis slot (alloca + load).
    fn slot_ty(&self, kind: SlotKind) -> BasicTypeEnum<'ctx> {
        match kind {
            SlotKind::F64 => self.f64_ty,
            SlotKind::I8 => self.i8_ty,
            SlotKind::Ptr => self.ptr_ty,
            SlotKind::I32 => self.i32_ty,
            SlotKind::I64 => self.i64_ty,
        }
    }

    /// Signature fungsi LLVM dari tipe MIR.
    fn fn_type(&self, params: &[MirTy], ret: MirTy) -> FunctionType<'ctx> {
        let args: Vec<BasicMetadataTypeEnum> = params
            .iter()
            .map(|p| {
                let t = self.abi_ty(*p);
                BasicMetadataTypeEnum::from(t)
            })
            .collect();
        match ret {
            MirTy::Void => self.context.void_type().fn_type(&args, false),
            MirTy::F64 => self.context.f64_type().fn_type(&args, false),
            MirTy::Bool => self.context.i8_type().fn_type(&args, false),
            MirTy::Str | MirTy::Ptr => self
                .context
                .ptr_type(AddressSpace::default())
                .fn_type(&args, false),
        }
    }

    /// Deklarasikan import fungsi C-ABI (didefinisikan di libruntime/libc).
    fn declare_import(
        &mut self,
        name: &str,
        params: &[BasicMetadataTypeEnum<'ctx>],
        ret: Option<SlotKind>,
    ) {
        let ty = match ret {
            None => self.context.void_type().fn_type(params, false),
            Some(k) => {
                let t = self.slot_ty(k);
                match t {
                    BasicTypeEnum::FloatType(f) => f.fn_type(params, false),
                    BasicTypeEnum::IntType(i) => i.fn_type(params, false),
                    BasicTypeEnum::PointerType(p) => p.fn_type(params, false),
                    _ => self.context.void_type().fn_type(params, false),
                }
            }
        };
        let fv = self.module.add_function(name, ty, None);
        self.runtime.insert(name.to_string(), fv);
    }

    /// Deklarasikan semua intrinsik runtime + catat jenis return-nya.
    fn declare_runtime(&mut self) -> GnResult<()> {
        let (ptr, f64t, i8t, i32t, i64t) = (
            BasicMetadataTypeEnum::from(self.ptr_ty),
            BasicMetadataTypeEnum::from(self.f64_ty),
            BasicMetadataTypeEnum::from(self.i8_ty),
            BasicMetadataTypeEnum::from(self.i32_ty),
            BasicMetadataTypeEnum::from(self.i64_ty),
        );
        self.declare_import("puts", &[ptr], Some(SlotKind::I32));
        self.declare_import(
            "gn_window_create",
            &[ptr, i64t, i32t, i32t],
            Some(SlotKind::I32),
        );
        self.declare_import("gn_window_run", &[i32t], Some(SlotKind::I32));
        self.declare_import("gn_scene_create", &[], Some(SlotKind::I64));
        self.declare_import("gn_scene_add_text", &[i64t, ptr, f64t], None);
        self.declare_import("gn_scene_add_button", &[i64t, ptr], None);
        self.declare_import("gn_scene_add_input", &[i64t, ptr], None);
        self.declare_import("gn_scene_begin_vstack", &[i64t, f64t], None);
        self.declare_import("gn_scene_end_vstack", &[i64t], None);
        self.declare_import("gn_window_run_scene", &[i64t], Some(SlotKind::I32));
        self.declare_import("gn_state_set_f64", &[ptr, ptr, f64t], None);
        self.declare_import("gn_state_get_f64", &[ptr, ptr], Some(SlotKind::F64));
        self.declare_import("gn_state_set_bool", &[ptr, ptr, i8t], None);
        self.declare_import("gn_state_get_bool", &[ptr, ptr], Some(SlotKind::I8));
        self.declare_import("gn_state_set_str", &[ptr, ptr, ptr], None);
        self.declare_import("gn_state_get_str", &[ptr, ptr], Some(SlotKind::Ptr));
        self.declare_import(
            "gn_scene_add_button_clickable",
            &[i64t, ptr, ptr, ptr],
            None,
        );
        self.declare_import("gn_register_method", &[ptr, ptr, ptr], None);
        self.declare_import("gn_window_run_dynamic", &[ptr], Some(SlotKind::I32));

        self.runtime_ret
            .insert("gn_scene_create".into(), SlotKind::I64);
        self.runtime_ret
            .insert("gn_window_run_scene".into(), SlotKind::I32);
        self.runtime_ret
            .insert("gn_window_create".into(), SlotKind::I32);
        self.runtime_ret.insert("puts".into(), SlotKind::I32);
        self.runtime_ret
            .insert("gn_state_get_f64".into(), SlotKind::F64);
        self.runtime_ret
            .insert("gn_state_get_bool".into(), SlotKind::I8);
        self.runtime_ret
            .insert("gn_state_get_str".into(), SlotKind::Ptr);
        self.runtime_ret
            .insert("gn_window_run_dynamic".into(), SlotKind::I32);
        Ok(())
    }

    /// String pool → global konstan null-terminated (linkage private).
    fn declare_strings(&mut self, mir: &MirModule) -> GnResult<()> {
        let i8t = self.context.i8_type();
        for (i, s) in mir.strings.iter().enumerate() {
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0);
            let arr = self.context.const_string(&bytes, false);
            let g = self.module.add_global(
                i8t.array_type(bytes.len() as u32),
                None,
                &format!("str_{i}"),
            );
            g.set_linkage(Linkage::Private);
            g.set_constant(true);
            g.set_initializer(&arr);
            self.strings.push(g.as_pointer_value());
        }
        Ok(())
    }
}

/// Status satu fungsi selama codegen (alloca + block per fungsi).
struct FuncGen<'ctx> {
    slots: HashMap<u32, (PointerValue<'ctx>, SlotKind)>,
    vars: HashMap<u32, (PointerValue<'ctx>, SlotKind)>,
    blocks: HashMap<u32, inkwell::basic_block::BasicBlock<'ctx>>,
    terminated: bool,
}

impl<'ctx, 'm> ModuleGen<'ctx, 'm> {
    /// Compile satu fungsi MIR → LLVM.
    fn compile_function(&mut self, func: &MirFunc) -> GnResult<()> {
        let fv = self.funcs[&func.name];
        let entry = self.context.append_basic_block(fv, "entry");

        // Prescan 1: jenis variabel (params dulu, lalu store pertama menang).
        let mut var_kinds: HashMap<u32, SlotKind> = HashMap::new();
        for (i, p) in func.params.iter().enumerate() {
            var_kinds.insert(i as u32, ModuleGen::mir_kind(*p));
        }

        // Prescan 2: jenis tiap slot hasil (untuk alloca di entry).
        // `LoadVar` memakai jenis variabelnya (default F64 bila asing).
        let mut slot_kinds: HashMap<u32, SlotKind> = HashMap::new();
        for (i, inst) in func.body.iter().enumerate() {
            let slot = i as u32;
            let kind = match inst {
                MirInst::ConstF64(_) | MirInst::BinF64 { .. } => Some(SlotKind::F64),
                MirInst::ConstBool(_)
                | MirInst::CmpF64 { .. }
                | MirInst::CmpBool { .. }
                | MirInst::CmpStr { .. }
                | MirInst::Not(_) => Some(SlotKind::I8),
                MirInst::StrAddr(_) | MirInst::FuncAddr { .. } => Some(SlotKind::Ptr),
                MirInst::LoadVar(var) => Some(var_kinds.get(var).copied().unwrap_or(SlotKind::F64)),
                MirInst::Call { callee, .. } => self.call_ret_kind(callee),
                MirInst::CallIndirect { ret, .. } => Some(ModuleGen::mir_kind(*ret)),
                MirInst::WindowCreate { .. } => Some(SlotKind::I32),
                _ => None,
            };
            if let Some(k) = kind {
                slot_kinds.insert(slot, k);
            }
            if let MirInst::StoreVar { var, val } = inst {
                if !var_kinds.contains_key(var) {
                    let k = slot_kinds.get(val).copied().unwrap_or(SlotKind::F64);
                    var_kinds.insert(*var, k);
                }
            }
        }

        let mut gen = FuncGen {
            slots: HashMap::new(),
            vars: HashMap::new(),
            blocks: HashMap::new(),
            terminated: false,
        };

        // Blok per Label (urutan body) + alloca semua slot/var di entry.
        self.builder.position_at_end(entry);
        for (i, inst) in func.body.iter().enumerate() {
            if let MirInst::Label(n) = inst {
                let bb = self.context.append_basic_block(fv, &format!("label_{n}"));
                gen.blocks.insert(*n, bb);
            }
            if let Some(k) = slot_kinds.get(&(i as u32)) {
                let a = self
                    .builder
                    .build_alloca(self.slot_ty(*k), &format!("slot_{i}"))
                    .map_err(err_llvm)?;
                gen.slots.insert(i as u32, (a, *k));
            }
        }
        for (var, kind) in &var_kinds {
            let a = self
                .builder
                .build_alloca(self.slot_ty(*kind), &format!("var_{var}"))
                .map_err(err_llvm)?;
            gen.vars.insert(*var, (a, *kind));
        }

        // Parameter → simpan ke alloca masing-masing.
        for (i, _) in func.params.iter().enumerate() {
            if let Some(param) = fv.get_nth_param(i as u32) {
                let (a, _) = gen.vars[&(i as u32)];
                self.builder.build_store(a, param).map_err(err_llvm)?;
            }
        }

        // Walk body (mirip backend Cranelift; tanpa terminator di akhir
        // → return default; Label tanpa terminator → bridge branch).
        let mut current = entry;
        for (i, inst) in func.body.iter().enumerate() {
            match inst {
                MirInst::Label(n) => {
                    let bb = gen.blocks[n];
                    if !gen.terminated {
                        self.builder.position_at_end(current);
                        self.builder
                            .build_unconditional_branch(bb)
                            .map_err(err_llvm)?;
                    }
                    self.builder.position_at_end(bb);
                    current = bb;
                    gen.terminated = false;
                }
                other => {
                    if gen.terminated {
                        continue;
                    }
                    self.builder.position_at_end(current);
                    self.emit_inst(i as u32, other, &mut gen)?;
                    if is_terminator(other) {
                        gen.terminated = true;
                    }
                }
            }
        }
        if !gen.terminated {
            self.builder.position_at_end(current);
            self.emit_default_return(func.ret)?;
        }
        Ok(())
    }

    /// Jenis return panggilan `Call` (user dari signature, runtime dari tabel).
    fn call_ret_kind(&self, callee: &str) -> Option<SlotKind> {
        if self.runtime.contains_key(callee) {
            return self.runtime_ret.get(callee).copied();
        }
        self.sigs.get(callee).and_then(|(_, r)| {
            if *r == MirTy::Void {
                None
            } else {
                Some(ModuleGen::mir_kind(*r))
            }
        })
    }

    /// Jenis slot dari tipe MIR.
    fn mir_kind(ty: MirTy) -> SlotKind {
        match ty {
            MirTy::F64 => SlotKind::F64,
            MirTy::Bool => SlotKind::I8,
            MirTy::Str | MirTy::Ptr => SlotKind::Ptr,
            MirTy::Void => SlotKind::F64, // tidak dipakai sebagai nilai
        }
    }
}

/// Petakan error inkwell ke `GnError`.
fn err_llvm<E: std::fmt::Display>(e: E) -> GnError {
    GnError::Other(format!("llvm: {e}"))
}

/// Instruksi yang mengakhiri block.
fn is_terminator(inst: &MirInst) -> bool {
    matches!(
        inst,
        MirInst::Ret(_) | MirInst::Jump(_) | MirInst::Branch { .. }
    )
}

impl<'ctx, 'm> ModuleGen<'ctx, 'm> {
    /// Muat nilai slot (load dari alloca).
    fn load_slot(
        &self,
        gen: &FuncGen<'ctx>,
        slot: u32,
    ) -> GnResult<(BasicValueEnum<'ctx>, SlotKind)> {
        let (a, kind) =
            gen.slots.get(&slot).copied().ok_or_else(|| {
                GnError::Other(format!("slot MIR {slot} tak dikenal (invalid MIR)"))
            })?;
        let v = self
            .builder
            .build_load(self.slot_ty(kind), a, &format!("slotv_{slot}"))
            .map_err(err_llvm)?;
        Ok((v, kind))
    }

    /// Simpan nilai SSA ke alloca slot.
    fn def_slot(
        &self,
        gen: &FuncGen<'ctx>,
        slot: u32,
        value: BasicValueEnum<'ctx>,
    ) -> GnResult<()> {
        let (a, _) =
            gen.slots.get(&slot).copied().ok_or_else(|| {
                GnError::Other(format!("slot MIR {slot} tanpa alloca (invalid MIR)"))
            })?;
        self.builder.build_store(a, value).map_err(err_llvm)?;
        Ok(())
    }

    /// Simpan nilai ke variabel lokal MIR.
    fn store_var(
        &self,
        gen: &FuncGen<'ctx>,
        var: u32,
        value: BasicValueEnum<'ctx>,
        kind: SlotKind,
    ) -> GnResult<()> {
        let (a, want) = gen.vars.get(&var).copied().ok_or_else(|| {
            GnError::Other(format!("variabel MIR {var} tanpa alloca (invalid MIR)"))
        })?;
        if want != kind {
            return Err(GnError::Other(format!(
                "tipe variabel {var} tak konsisten (invalid MIR)"
            )));
        }
        self.builder.build_store(a, value).map_err(err_llvm)?;
        Ok(())
    }

    /// Muat nilai variabel lokal MIR (tanpa definisi → default 0.0).
    fn load_var(
        &self,
        gen: &mut FuncGen<'ctx>,
        var: u32,
    ) -> GnResult<(BasicValueEnum<'ctx>, SlotKind)> {
        if let Some((a, kind)) = gen.vars.get(&var).copied() {
            let v = self
                .builder
                .build_load(self.slot_ty(kind), a, &format!("varv_{var}"))
                .map_err(err_llvm)?;
            return Ok((v, kind));
        }
        // Variabel tanpa definisi → default number 0 (seperti Cranelift).
        let a = self
            .builder
            .build_alloca(self.f64_ty, &format!("var_{var}"))
            .map_err(err_llvm)?;
        let zero = self.context.f64_type().const_float(0.0);
        self.builder.build_store(a, zero).map_err(err_llvm)?;
        gen.vars.insert(var, (a, SlotKind::F64));
        let v = self
            .builder
            .build_load(self.f64_ty, a, &format!("varv_{var}"))
            .map_err(err_llvm)?;
        Ok((v, SlotKind::F64))
    }

    /// Konversi kondisi ke i1 untuk `condbr`.
    fn to_i1(
        &self,
        value: BasicValueEnum<'ctx>,
        kind: SlotKind,
    ) -> GnResult<inkwell::values::IntValue<'ctx>> {
        match (value, kind) {
            (BasicValueEnum::IntValue(v), SlotKind::I8) => {
                let zero = self.context.i8_type().const_zero();
                self.builder
                    .build_int_compare(IntPredicate::NE, v, zero, "truthy")
                    .map_err(err_llvm)
            }
            (BasicValueEnum::FloatValue(v), SlotKind::F64) => {
                let zero = self.context.f64_type().const_zero();
                self.builder
                    .build_float_compare(FloatPredicate::ONE, v, zero, "truthy")
                    .map_err(err_llvm)
            }
            (BasicValueEnum::PointerValue(v), SlotKind::Ptr) => {
                let as_int = self
                    .builder
                    .build_ptr_to_int(v, self.context.i64_type(), "truthy_ptr")
                    .map_err(err_llvm)?;
                let zero = self.context.i64_type().const_zero();
                self.builder
                    .build_int_compare(IntPredicate::NE, as_int, zero, "truthy")
                    .map_err(err_llvm)
            }
            _ => Err(GnError::Other(
                "kondisi branch bukan boolean (invalid MIR)".into(),
            )),
        }
    }
}

impl<'ctx, 'm> ModuleGen<'ctx, 'm> {
    /// Codegen satu instruksi; `slot` = index asli di body.
    #[allow(clippy::too_many_lines)]
    fn emit_inst(&mut self, slot: u32, inst: &MirInst, gen: &mut FuncGen<'ctx>) -> GnResult<()> {
        match inst {
            MirInst::ConstF64(n) => {
                let v = BasicValueEnum::FloatValue(self.context.f64_type().const_float(*n));
                self.def_slot(gen, slot, v)?;
            }
            MirInst::ConstBool(b) => {
                let v = BasicValueEnum::IntValue(
                    self.context.i8_type().const_int(u64::from(*b), false),
                );
                self.def_slot(gen, slot, v)?;
            }
            MirInst::StrAddr(idx) => {
                let g = self.strings.get(*idx).copied().ok_or_else(|| {
                    GnError::Other(format!("string pool {idx} tak ada (invalid MIR)"))
                })?;
                let zero = self.context.i32_type().const_zero();
                // Aman: global string pool berukuran ≥1 dengan dua index nol.
                let ptr = unsafe {
                    self.builder.build_in_bounds_gep(
                        self.context.i8_type(),
                        g,
                        &[zero, zero],
                        "straddr",
                    )
                }
                .map_err(err_llvm)?;
                self.def_slot(gen, slot, BasicValueEnum::PointerValue(ptr))?;
            }
            MirInst::FuncAddr { func } => {
                let fv = self.funcs.get(func).copied().ok_or_else(|| {
                    GnError::Other(format!("fungsi `{func}` tak dikenal (invalid MIR)"))
                })?;
                let ptr = fv.as_global_value().as_pointer_value();
                self.def_slot(gen, slot, BasicValueEnum::PointerValue(ptr))?;
            }
            MirInst::BinF64 { op, a, b } => {
                let (av, _) = self.load_slot(gen, *a)?;
                let (bv, _) = self.load_slot(gen, *b)?;
                let (af, bf) = match (av, bv) {
                    (BasicValueEnum::FloatValue(x), BasicValueEnum::FloatValue(y)) => (x, y),
                    _ => {
                        return Err(GnError::Other("aritmatika bukan f64 (invalid MIR)".into()));
                    }
                };
                let v = match op {
                    MirBinOp::Add => self.builder.build_float_add(af, bf, "fadd"),
                    MirBinOp::Sub => self.builder.build_float_sub(af, bf, "fsub"),
                    MirBinOp::Mul => self.builder.build_float_mul(af, bf, "fmul"),
                    MirBinOp::Div => self.builder.build_float_div(af, bf, "fdiv"),
                }
                .map_err(err_llvm)?;
                self.def_slot(gen, slot, BasicValueEnum::FloatValue(v))?;
            }
            MirInst::CmpF64 { op, a, b } => {
                let (av, _) = self.load_slot(gen, *a)?;
                let (bv, _) = self.load_slot(gen, *b)?;
                let (af, bf) = match (av, bv) {
                    (BasicValueEnum::FloatValue(x), BasicValueEnum::FloatValue(y)) => (x, y),
                    _ => {
                        return Err(GnError::Other(
                            "perbandingan bukan f64 (invalid MIR)".into(),
                        ));
                    }
                };
                let pred = match op {
                    MirCmpOp::Eq => FloatPredicate::OEQ,
                    MirCmpOp::Ne => FloatPredicate::ONE,
                    MirCmpOp::Lt => FloatPredicate::OLT,
                    MirCmpOp::Le => FloatPredicate::OLE,
                    MirCmpOp::Gt => FloatPredicate::OGT,
                    MirCmpOp::Ge => FloatPredicate::OGE,
                };
                let cmp = self
                    .builder
                    .build_float_compare(pred, af, bf, "fcmp")
                    .map_err(err_llvm)?;
                let v = self
                    .builder
                    .build_int_z_extend(cmp, self.context.i8_type(), "tobool")
                    .map_err(err_llvm)?;
                self.def_slot(gen, slot, BasicValueEnum::IntValue(v))?;
            }
            MirInst::CmpBool { op, a, b } => {
                self.emit_int_cmp(slot, *op, *a, *b, gen)?;
            }
            MirInst::CmpStr { op, a, b } => {
                // Pointer equality (string di-intern di pool).
                match op {
                    MirCmpOp::Eq | MirCmpOp::Ne => {}
                    _ => {
                        return Err(GnError::Other(
                            "relasional string belum didukung (invalid MIR)".into(),
                        ));
                    }
                }
                self.emit_int_cmp(slot, *op, *a, *b, gen)?;
            }
            MirInst::Not(a) => {
                let (av, _) = self.load_slot(gen, *a)?;
                let v = match av {
                    BasicValueEnum::IntValue(x) => {
                        let zero = self.context.i8_type().const_zero();
                        let cmp = self
                            .builder
                            .build_int_compare(IntPredicate::EQ, x, zero, "not")
                            .map_err(err_llvm)?;
                        self.builder
                            .build_int_z_extend(cmp, self.context.i8_type(), "notbool")
                            .map_err(err_llvm)?
                    }
                    _ => {
                        return Err(GnError::Other("negasi bukan bool (invalid MIR)".into()));
                    }
                };
                self.def_slot(gen, slot, BasicValueEnum::IntValue(v))?;
            }
            MirInst::StoreVar { var, val } => {
                let (vv, kind) = self.load_slot(gen, *val)?;
                self.store_var(gen, *var, vv, kind)?;
            }
            MirInst::LoadVar(var) => {
                let (v, _) = self.load_var(gen, *var)?;
                self.def_slot(gen, slot, v)?;
            }
            MirInst::Call { callee, args } => {
                let mut arg_vals = Vec::new();
                for a in args {
                    let (v, _) = self.load_slot(gen, *a)?;
                    arg_vals.push(BasicMetadataValueEnum::from(v));
                }
                if let Some(fref) = self.runtime.get(callee).copied() {
                    let site = self
                        .builder
                        .build_call(fref, &arg_vals, "rtcall")
                        .map_err(err_llvm)?;
                    if self.runtime_ret.contains_key(callee) {
                        let v = site.try_as_basic_value().left().ok_or_else(|| {
                            GnError::Other(format!("hasil `{callee}` void (invalid MIR)"))
                        })?;
                        self.def_slot(gen, slot, v)?;
                    }
                } else {
                    let fid = self.funcs.get(callee).copied().ok_or_else(|| {
                        GnError::Other(format!("fungsi `{callee}` tak dikenal (invalid MIR)"))
                    })?;
                    let site = self
                        .builder
                        .build_call(fid, &arg_vals, "call")
                        .map_err(err_llvm)?;
                    let ret = self
                        .sigs
                        .get(callee)
                        .map(|(_, r)| *r)
                        .unwrap_or(MirTy::Void);
                    if ret != MirTy::Void {
                        let v = site.try_as_basic_value().left().ok_or_else(|| {
                            GnError::Other(format!("hasil `{callee}` void (invalid MIR)"))
                        })?;
                        self.def_slot(gen, slot, v)?;
                    }
                }
            }
            MirInst::WindowCreate {
                title,
                width,
                height,
            } => {
                let (t, _) = self.load_slot(gen, *title)?;
                let fref = self.runtime["gn_window_create"];
                let len = self.context.i64_type().const_zero();
                let w = self.context.i32_type().const_int(u64::from(*width), false);
                let h = self.context.i32_type().const_int(u64::from(*height), false);
                let site = self
                    .builder
                    .build_call(
                        fref,
                        &[
                            BasicMetadataValueEnum::from(t),
                            BasicMetadataValueEnum::from(len),
                            BasicMetadataValueEnum::from(w),
                            BasicMetadataValueEnum::from(h),
                        ],
                        "windowcreate",
                    )
                    .map_err(err_llvm)?;
                let v = site.try_as_basic_value().left().ok_or_else(|| {
                    GnError::Other("hasil gn_window_create void (invalid MIR)".into())
                })?;
                self.def_slot(gen, slot, v)?;
            }
            MirInst::WindowRun(handle) => {
                let (h, _) = self.load_slot(gen, *handle)?;
                let fref = self.runtime["gn_window_run"];
                self.builder
                    .build_call(fref, &[BasicMetadataValueEnum::from(h)], "windowrun")
                    .map_err(err_llvm)?;
            }
            MirInst::CallIndirect {
                target,
                params,
                ret,
                args,
            } => {
                let (tgt, _) = self.load_slot(gen, *target)?;
                let ptr = match tgt {
                    BasicValueEnum::PointerValue(p) => p,
                    _ => {
                        return Err(GnError::Other(
                            "target indirect bukan pointer (invalid MIR)".into(),
                        ));
                    }
                };
                let sig = self.fn_type(params, *ret);
                let mut arg_vals = Vec::new();
                for a in args {
                    let (v, _) = self.load_slot(gen, *a)?;
                    arg_vals.push(BasicMetadataValueEnum::from(v));
                }
                let site = self
                    .builder
                    .build_indirect_call(sig, ptr, &arg_vals, "indcall")
                    .map_err(err_llvm)?;
                if *ret != MirTy::Void {
                    let v = site.try_as_basic_value().left().ok_or_else(|| {
                        GnError::Other("hasil indirect void (invalid MIR)".into())
                    })?;
                    self.def_slot(gen, slot, v)?;
                }
            }
            MirInst::Ret(value) => {
                match value {
                    Some(s) => {
                        let (v, _) = self.load_slot(gen, *s)?;
                        self.builder.build_return(Some(&v)).map_err(err_llvm)?;
                    }
                    None => {
                        self.builder.build_return(None).map_err(err_llvm)?;
                    }
                }
                gen.terminated = true;
            }
            MirInst::Jump(label) => {
                let bb = gen.blocks.get(label).copied().ok_or_else(|| {
                    GnError::Other(format!("label {label} tak dikenal (invalid MIR)"))
                })?;
                self.builder
                    .build_unconditional_branch(bb)
                    .map_err(err_llvm)?;
                gen.terminated = true;
            }
            MirInst::Branch { cond, then, els } => {
                let (c, kind) = self.load_slot(gen, *cond)?;
                let test = self.to_i1(c, kind)?;
                let tb = gen.blocks.get(then).copied().ok_or_else(|| {
                    GnError::Other(format!("label {then} tak dikenal (invalid MIR)"))
                })?;
                let eb = gen.blocks.get(els).copied().ok_or_else(|| {
                    GnError::Other(format!("label {els} tak dikenal (invalid MIR)"))
                })?;
                self.builder
                    .build_conditional_branch(test, tb, eb)
                    .map_err(err_llvm)?;
                gen.terminated = true;
            }
            MirInst::Label(_) => {
                // Ditangani di compile_function (unreachable di sini).
            }
        }
        Ok(())
    }

    /// Perbandingan integer/pointer (`==`/`!=`) → bool i8.
    fn emit_int_cmp(
        &mut self,
        slot: u32,
        op: MirCmpOp,
        a: u32,
        b: u32,
        gen: &mut FuncGen<'ctx>,
    ) -> GnResult<()> {
        let pred = match op {
            MirCmpOp::Eq => IntPredicate::EQ,
            MirCmpOp::Ne => IntPredicate::NE,
            _ => {
                return Err(GnError::Other(
                    "relasional non-f64 belum didukung (invalid MIR)".into(),
                ));
            }
        };
        let (av, _) = self.load_slot(gen, a)?;
        let (bv, _) = self.load_slot(gen, b)?;
        let cmp = match (av, bv) {
            (BasicValueEnum::IntValue(x), BasicValueEnum::IntValue(y)) => self
                .builder
                .build_int_compare(pred, x, y, "icmp")
                .map_err(err_llvm)?,
            (BasicValueEnum::PointerValue(x), BasicValueEnum::PointerValue(y)) => {
                let xi = self
                    .builder
                    .build_ptr_to_int(x, self.context.i64_type(), "ptri")
                    .map_err(err_llvm)?;
                let yi = self
                    .builder
                    .build_ptr_to_int(y, self.context.i64_type(), "ptri")
                    .map_err(err_llvm)?;
                self.builder
                    .build_int_compare(pred, xi, yi, "pstrcmp")
                    .map_err(err_llvm)?
            }
            _ => {
                return Err(GnError::Other(
                    "perbandingan tipe campuran (invalid MIR)".into(),
                ));
            }
        };
        let v = self
            .builder
            .build_int_z_extend(cmp, self.context.i8_type(), "tobool")
            .map_err(err_llvm)?;
        self.def_slot(gen, slot, BasicValueEnum::IntValue(v))?;
        Ok(())
    }

    /// Path tanpa terminator di akhir → return nilai default.
    fn emit_default_return(&self, ret: MirTy) -> GnResult<()> {
        match ret {
            MirTy::Void => {
                self.builder.build_return(None).map_err(err_llvm)?;
            }
            MirTy::F64 => {
                let v = self.context.f64_type().const_float(0.0);
                self.builder.build_return(Some(&v)).map_err(err_llvm)?;
            }
            MirTy::Bool => {
                let v = self.context.i8_type().const_zero();
                self.builder.build_return(Some(&v)).map_err(err_llvm)?;
            }
            MirTy::Str | MirTy::Ptr => {
                let v = self.context.ptr_type(AddressSpace::default()).const_null();
                self.builder.build_return(Some(&v)).map_err(err_llvm)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{MirFunc, MirModule};

    /// Object sementara untuk test (di direktori unik per proses).
    fn out_dir(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("gnative-llvm-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("app.o");
        (dir, out)
    }

    #[test]
    fn emits_object_file() {
        let mir = MirModule {
            strings: vec!["Halo".to_string()],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Void,
                body: vec![
                    MirInst::StrAddr(0), // slot 0: title ptr
                    MirInst::WindowCreate {
                        title: 0,
                        width: 800,
                        height: 600,
                    }, // slot 1: handle
                    MirInst::WindowRun(1),
                    MirInst::Ret(None),
                ],
            }],
        };
        let (dir, out) = out_dir("obj");
        LlvmBackend
            .codegen(&mir, None, &out)
            .expect("codegen sukses");
        assert!(out.exists());
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"\x7fELF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emits_function_with_if_and_call() {
        // max(a, b) + main memanggil — cermin test Cranelift.
        let mut func: Vec<MirInst> = Vec::new();
        let cond = func.len() as u32;
        func.push(MirInst::LoadVar(0));
        let zero = func.len() as u32;
        func.push(MirInst::ConstF64(0.0));
        let cmp = func.len() as u32;
        func.push(MirInst::CmpF64 {
            op: MirCmpOp::Eq,
            a: cond,
            b: zero,
        });
        let (then, els, merge) = (100u32, 101u32, 102u32);
        func.push(MirInst::Branch {
            cond: cmp,
            then,
            els,
        });
        func.push(MirInst::Label(then));
        let one = func.len() as u32;
        func.push(MirInst::ConstF64(1.0));
        func.push(MirInst::Ret(Some(one)));
        func.push(MirInst::Jump(merge));
        func.push(MirInst::Label(els));
        let a2 = func.len() as u32;
        func.push(MirInst::LoadVar(0));
        let one2 = func.len() as u32;
        func.push(MirInst::ConstF64(1.0));
        let sum = func.len() as u32;
        func.push(MirInst::BinF64 {
            op: MirBinOp::Add,
            a: a2,
            b: one2,
        });
        func.push(MirInst::Ret(Some(sum)));
        func.push(MirInst::Jump(merge));
        func.push(MirInst::Label(merge));
        func.push(MirInst::Ret(Some(cond)));

        let mir = MirModule {
            strings: vec![],
            funcs: vec![
                MirFunc {
                    name: "max".into(),
                    params: vec![MirTy::F64, MirTy::F64],
                    ret: MirTy::F64,
                    body: func,
                },
                MirFunc {
                    name: "main".into(),
                    params: vec![],
                    ret: MirTy::Void,
                    body: vec![
                        MirInst::ConstF64(0.0),
                        MirInst::ConstF64(0.0),
                        MirInst::Call {
                            callee: "max".into(),
                            args: vec![0, 1],
                        },
                        MirInst::Ret(None),
                    ],
                },
            ],
        };
        let (dir, out) = out_dir("ifcall");
        LlvmBackend
            .codegen(&mir, None, &out)
            .expect("codegen sukses");
        assert_eq!(&std::fs::read(&out).unwrap()[0..4], b"\x7fELF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emits_cross_target_object() {
        // Cross-compile aarch64 (LLVM multi-target).
        let mir = MirModule {
            strings: vec![],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Void,
                body: vec![MirInst::Ret(None)],
            }],
        };
        let (dir, out) = out_dir("cross");
        LlvmBackend
            .codegen(&mir, Some("aarch64-unknown-linux-gnu"), &out)
            .expect("codegen cross-target sukses");
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"\x7fELF");
        assert_eq!(bytes[18], 0xB7, "e_machine harus aarch64");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
