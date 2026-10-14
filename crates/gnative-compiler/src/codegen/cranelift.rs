//! Backend Cranelift (mode dev) — MIR → object file native.
//!
//! Memakai `cranelift-codegen` + `cranelift-frontend` + `cranelift-module` + `cranelift-object` (murni Rust). Menghasilkan object file (ELF di Linux) yang siap di-link menjadi binary native oleh `linker.rs`.
//!
//! Model nilai: setiap slot MIR (index instruksi yang menghasilkan nilai)
//! disimpan ke `Variable` cranelift-frontend; frontend menyisipkan phi
//! secara otomatis untuk nilai yang live di beberapa block (if/&&/||).
//!
//! Intrinsik yang dipanggil (C-ABI, dari `gnative-runtime`):
//!   - `gn_window_create(ptr, len, w, h) -> handle`
//!   - `gn_window_run(handle) -> i32`
//!   - `gn_window_run_dynamic(build_fn) -> i32` (build + rebuild klik)
//!   - `gn_register_method(comp, method, fptr)` (dispatch klik)
//!   - `gn_scene_add_button_clickable(scene, label, comp, method)`
//!   - `gn_state_get/set_*` (state komponen)
//!   - `puts(ptr) -> i32` (libc, untuk headless/debug)

use std::collections::HashMap;
use std::path::Path;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{types, AbiParam, Function, InstBuilder, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
use gnative_core::{GnError, GnResult};

use super::CodegenBackend;
use crate::mir::{MirBinOp, MirCmpOp, MirFunc, MirInst, MirModule, MirTy, SlotTy};

/// Backend Cranelift — dev build.
pub struct CraneliftBackend;

impl CodegenBackend for CraneliftBackend {
    fn name(&self) -> &'static str {
        "cranelift"
    }

    fn codegen(&self, mir: &MirModule, target: Option<&str>, out: &Path) -> GnResult<()> {
        // 1. Target ISA: `--target <triple>` → cross-arch (Cranelift murni
        // Rust, tanpa toolkit eksternal); `None` → ISA host (native).
        let mut flag_builder = settings::builder();
        flag_builder
            .enable("is_pic")
            .map_err(|e| GnError::Other(format!("setting is_pic gagal: {e}")))?;
        let flags = settings::Flags::new(flag_builder);

        let isa = match target {
            Some(triple) => {
                let b = cranelift_codegen::isa::lookup_by_name(triple).map_err(|e| {
                    GnError::Other(format!("target `{triple}` tidak didukung Cranelift: {e}"))
                })?;
                b.finish(flags).map_err(|e| {
                    GnError::Other(format!("inisialisasi ISA `{triple}` gagal: {e}"))
                })?
            }
            None => {
                let isa_builder = cranelift_native::builder()
                    .map_err(|_| GnError::Other("host ISA tidak didukung Cranelift".into()))?;
                isa_builder
                    .finish(flags)
                    .map_err(|e| GnError::Other(format!("inisialisasi ISA gagal: {e}")))?
            }
        };

        // 2. Modul object.
        let obj_builder =
            ObjectBuilder::new(isa, "gnative", cranelift_module::default_libcall_names())
                .map_err(|e| GnError::Other(format!("ObjectBuilder gagal: {e}")))?;
        let mut module = ObjectModule::new(obj_builder);

        // 3. Intrinsik (import C-ABI dari libruntime + libc).
        let pointer = module.target_config().pointer_type();
        let puts_id = declare_import(
            &mut module,
            "puts",
            &[AbiParam::new(pointer)],
            &[AbiParam::new(types::I32)],
        )?;
        let window_create_id = declare_import(
            &mut module,
            "gn_window_create",
            &[
                AbiParam::new(pointer),
                AbiParam::new(types::I64), // title_len (usize)
                AbiParam::new(types::I32), // width
                AbiParam::new(types::I32), // height
            ],
            &[AbiParam::new(types::I32)], // handle
        )?;
        let window_run_id = declare_import(
            &mut module,
            "gn_window_run",
            &[AbiParam::new(types::I32)], // handle
            &[AbiParam::new(types::I32)],
        )?;

        // Intrinsik pembangun Scene (dari binary `.gn` → runtime).
        let scene_create_id = declare_import(
            &mut module,
            "gn_scene_create",
            &[],
            &[AbiParam::new(types::I64)], // handle
        )?;
        let scene_add_text_id = declare_import(
            &mut module,
            "gn_scene_add_text",
            &[
                AbiParam::new(types::I64),
                AbiParam::new(pointer),
                AbiParam::new(types::F64),
            ],
            &[],
        )?;
        let scene_add_button_id = declare_import(
            &mut module,
            "gn_scene_add_button",
            &[AbiParam::new(types::I64), AbiParam::new(pointer)],
            &[],
        )?;
        let scene_add_input_id = declare_import(
            &mut module,
            "gn_scene_add_input",
            &[AbiParam::new(types::I64), AbiParam::new(pointer)],
            &[],
        )?;
        let scene_begin_vstack_id = declare_import(
            &mut module,
            "gn_scene_begin_vstack",
            &[AbiParam::new(types::I64), AbiParam::new(types::F64)],
            &[],
        )?;
        let scene_end_vstack_id = declare_import(
            &mut module,
            "gn_scene_end_vstack",
            &[AbiParam::new(types::I64)],
            &[],
        )?;
        let window_run_scene_id = declare_import(
            &mut module,
            "gn_window_run_scene",
            &[AbiParam::new(types::I64)], // scene handle
            &[AbiParam::new(types::I32)],
        )?;

        // State komponen (`state_store` runtime): get/set per tipe.
        let state_set_f64_id = declare_import(
            &mut module,
            "gn_state_set_f64",
            &[
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(types::F64),
            ],
            &[],
        )?;
        let state_get_f64_id = declare_import(
            &mut module,
            "gn_state_get_f64",
            &[AbiParam::new(pointer), AbiParam::new(pointer)],
            &[AbiParam::new(types::F64)],
        )?;
        let state_set_bool_id = declare_import(
            &mut module,
            "gn_state_set_bool",
            &[
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(types::I8),
            ],
            &[],
        )?;
        let state_get_bool_id = declare_import(
            &mut module,
            "gn_state_get_bool",
            &[AbiParam::new(pointer), AbiParam::new(pointer)],
            &[AbiParam::new(types::I8)],
        )?;
        let state_set_str_id = declare_import(
            &mut module,
            "gn_state_set_str",
            &[
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(pointer),
            ],
            &[],
        )?;
        let state_get_str_id = declare_import(
            &mut module,
            "gn_state_get_str",
            &[AbiParam::new(pointer), AbiParam::new(pointer)],
            &[AbiParam::new(pointer)],
        )?;

        // Klik + rebuild dinamis: tombol clickable, registrasi method,
        // run dengan fungsi build (`__gn_build_scene` dari binary).
        let scene_add_button_clickable_id = declare_import(
            &mut module,
            "gn_scene_add_button_clickable",
            &[
                AbiParam::new(types::I64),
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(pointer),
            ],
            &[],
        )?;
        let register_method_id = declare_import(
            &mut module,
            "gn_register_method",
            &[
                AbiParam::new(pointer),
                AbiParam::new(pointer),
                AbiParam::new(pointer),
            ],
            &[],
        )?;
        let window_run_dynamic_id = declare_import(
            &mut module,
            "gn_window_run_dynamic",
            &[AbiParam::new(pointer)], // fungsi build
            &[AbiParam::new(types::I32)],
        )?;

        // Map nama intrinsik runtime → FuncId + tipe return (untuk codegen Call).
        let mut runtime_ids: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        let mut runtime_ret_clif: HashMap<String, types::Type> = HashMap::new();
        runtime_ids.insert("puts".into(), puts_id);
        runtime_ids.insert("gn_window_create".into(), window_create_id);
        runtime_ids.insert("gn_window_run".into(), window_run_id);
        runtime_ids.insert("gn_scene_create".into(), scene_create_id);
        runtime_ids.insert("gn_scene_add_text".into(), scene_add_text_id);
        runtime_ids.insert("gn_scene_add_button".into(), scene_add_button_id);
        runtime_ids.insert("gn_scene_add_input".into(), scene_add_input_id);
        runtime_ids.insert("gn_scene_begin_vstack".into(), scene_begin_vstack_id);
        runtime_ids.insert("gn_scene_end_vstack".into(), scene_end_vstack_id);
        runtime_ids.insert("gn_window_run_scene".into(), window_run_scene_id);
        runtime_ids.insert("gn_state_set_f64".into(), state_set_f64_id);
        runtime_ids.insert("gn_state_get_f64".into(), state_get_f64_id);
        runtime_ids.insert("gn_state_set_bool".into(), state_set_bool_id);
        runtime_ids.insert("gn_state_get_bool".into(), state_get_bool_id);
        runtime_ids.insert("gn_state_set_str".into(), state_set_str_id);
        runtime_ids.insert("gn_state_get_str".into(), state_get_str_id);
        runtime_ids.insert(
            "gn_scene_add_button_clickable".into(),
            scene_add_button_clickable_id,
        );
        runtime_ids.insert("gn_register_method".into(), register_method_id);
        runtime_ids.insert("gn_window_run_dynamic".into(), window_run_dynamic_id);
        runtime_ret_clif.insert("gn_scene_create".into(), types::I64);
        runtime_ret_clif.insert("gn_window_run_scene".into(), types::I32);
        runtime_ret_clif.insert("gn_window_create".into(), types::I32);
        runtime_ret_clif.insert("gn_state_get_f64".into(), types::F64);
        runtime_ret_clif.insert("gn_state_get_bool".into(), types::I8);
        runtime_ret_clif.insert("gn_state_get_str".into(), pointer);
        runtime_ret_clif.insert("gn_window_run_dynamic".into(), types::I32);

        // 4. String pool → data objects (C string null-terminated).
        let mut data_ids = Vec::new();
        for (i, s) in mir.strings.iter().enumerate() {
            let did = module
                .declare_data(&format!("str_{i}"), Linkage::Local, false, false)
                .map_err(|e| GnError::Other(format!("deklarasi data gagal: {e}")))?;
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0); // null terminator
            let mut data = DataDescription::new();
            data.define(bytes.into_boxed_slice());
            module
                .define_data(did, &data)
                .map_err(|e| GnError::Other(format!("definisi data gagal: {e}")))?;
            data_ids.push(did);
        }

        // 5. Signature tiap fungsi user (untuk call + deklarasi).
        let mut sigs: HashMap<String, (Vec<MirTy>, MirTy)> = HashMap::new();
        for f in &mir.funcs {
            sigs.insert(f.name.clone(), (f.params.clone(), f.ret));
        }

        // 6. Pass 1: deklarasi semua fungsi (Export) — call bisa maju.
        let mut func_ids: HashMap<String, cranelift_module::FuncId> = HashMap::new();
        for f in &mir.funcs {
            let sig = make_sig(&mut module, &f.params, f.ret);
            let fid = module
                .declare_function(&f.name, Linkage::Export, &sig)
                .map_err(|e| GnError::Other(format!("deklarasi `{}` gagal: {e}", f.name)))?;
            func_ids.insert(f.name.clone(), fid);
        }

        // 7. Pass 2: compile body tiap fungsi.
        for f in &mir.funcs {
            compile_function(
                &mut module,
                &func_ids,
                &sigs,
                &runtime_ids,
                &runtime_ret_clif,
                &data_ids,
                f,
            )?;
        }

        // 8. Finalisasi → tulis object file.
        let product = module.finish();
        let file = std::fs::File::create(out).map_err(GnError::Io)?;
        let mut writer = std::io::BufWriter::new(file);
        product
            .object
            .write_stream(&mut writer)
            .map_err(|e| GnError::Other(format!("menulis object gagal: {e}")))?;
        Ok(())
    }
}

/// Deklarasikan import fungsi C-ABI.
fn declare_import(
    module: &mut ObjectModule,
    name: &str,
    params: &[AbiParam],
    returns: &[AbiParam],
) -> GnResult<cranelift_module::FuncId> {
    let mut sig = module.make_signature();
    sig.params.extend_from_slice(params);
    sig.returns.extend_from_slice(returns);
    module
        .declare_function(name, Linkage::Import, &sig)
        .map_err(|e| GnError::Other(format!("deklarasi `{name}` gagal: {e}")))
}

/// Buat signature Cranelift dari tipe MIR.
fn make_sig(
    module: &mut ObjectModule,
    params: &[MirTy],
    ret: MirTy,
) -> cranelift_codegen::ir::Signature {
    let ptr = module.target_config().pointer_type();
    let mut sig = module.make_signature();
    for p in params {
        sig.params.push(AbiParam::new(mir_abi_ty(*p, ptr)));
    }
    if ret != MirTy::Void {
        sig.returns.push(AbiParam::new(mir_abi_ty(ret, ptr)));
    }
    sig
}

/// Tipe ABI Cranelift untuk tipe MIR.
fn mir_abi_ty(ty: MirTy, pointer: types::Type) -> types::Type {
    match ty {
        MirTy::F64 => types::F64,
        // Bool diwakili 1 byte (0/1) — konsisten internal & ABI.
        MirTy::Bool => types::I8,
        MirTy::Str | MirTy::Ptr => pointer,
        MirTy::Void => types::I32, // tidak dipakai
    }
}

/// Konversi tipe return MIR → SlotTy (untuk hasil `Call`).
fn mir_ty_to_slot(ty: MirTy) -> SlotTy {
    match ty {
        MirTy::F64 => SlotTy::F64,
        MirTy::Bool => SlotTy::Bool,
        MirTy::Str => SlotTy::Str,
        MirTy::Ptr => SlotTy::Ptr,
        MirTy::Void => SlotTy::F64, // tidak dipakai sebagai nilai
    }
}

/// Compile satu fungsi MIR → Cranelift.
#[allow(clippy::too_many_arguments)]
fn compile_function(
    module: &mut ObjectModule,
    func_ids: &HashMap<String, cranelift_module::FuncId>,
    sigs: &HashMap<String, (Vec<MirTy>, MirTy)>,
    runtime_ids: &HashMap<String, cranelift_module::FuncId>,
    runtime_ret_clif: &HashMap<String, types::Type>,
    data_ids: &[cranelift_module::DataId],
    func: &MirFunc,
) -> GnResult<()> {
    let pointer = module.target_config().pointer_type();
    let sig = make_sig(module, &func.params, func.ret);
    let fid = func_ids[&func.name];

    let mut func_ir = Function::with_name_signature(UserFuncName::user(0, 0), sig);
    let mut ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut func_ir, &mut ctx);

    // Entry block + param block-params.
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    let mut param_vars: Vec<(Variable, SlotTy)> = Vec::new();
    for pty in &func.params {
        let abi = mir_abi_ty(*pty, pointer);
        let bp = builder.append_block_param(entry, abi);
        let v = builder.declare_var(abi);
        builder.def_var(v, bp);
        param_vars.push((v, mir_ty_to_slot(*pty)));
    }
    builder.seal_block(entry);

    // Deklarasikan semua intrinsik runtime di fungsi ini (FuncRef per-func).
    let mut runtime_funcs: HashMap<String, cranelift_codegen::ir::FuncRef> = HashMap::new();
    for (name, id) in runtime_ids {
        let fref = module.declare_func_in_func(*id, builder.func);
        runtime_funcs.insert(name.clone(), fref);
    }
    let local_create = runtime_funcs["gn_window_create"];
    let local_run = runtime_funcs["gn_window_run"];
    let target_config = module.target_config();

    let mut cg = FuncCodegen {
        builder,
        module,
        func_ids,
        sigs,
        runtime_funcs,
        runtime_ret_clif: runtime_ret_clif.clone(),
        local_create,
        local_run,
        data_ids,
        pointer,
        slot_vars: HashMap::new(),
        slot_types: HashMap::new(),
        // Param fungsi terdaftar sebagai variabel lokal 0..n-1,
        // sehingga `LoadVar` param langsung ter-resolve ke block param.
        var_vars: param_vars
            .into_iter()
            .enumerate()
            .map(|(i, pv)| (i as u32, pv))
            .collect(),
        labels: HashMap::new(),
        terminated: false,
    };
    cg.emit_body(&func.body, func.ret)?;

    cg.builder.seal_all_blocks();
    cg.builder.finalize(target_config);

    let mut context = Context::for_function(func_ir);
    module
        .define_function(fid, &mut context)
        .map_err(|e| GnError::Other(format!("definisi `{}` gagal: {e:?}", func.name)))?;
    Ok(())
}

/// Context codegen satu fungsi.
struct FuncCodegen<'a> {
    builder: FunctionBuilder<'a>,
    module: &'a mut ObjectModule,
    func_ids: &'a HashMap<String, cranelift_module::FuncId>,
    sigs: &'a HashMap<String, (Vec<MirTy>, MirTy)>,
    /// Nama intrinsik runtime → FuncRef (C-ABI dari libruntime).
    runtime_funcs: HashMap<String, cranelift_codegen::ir::FuncRef>,
    /// Nama intrinsik runtime → tipe return Cranelift.
    runtime_ret_clif: HashMap<String, types::Type>,
    local_create: cranelift_codegen::ir::FuncRef,
    local_run: cranelift_codegen::ir::FuncRef,
    data_ids: &'a [cranelift_module::DataId],
    pointer: types::Type,
    /// slot MIR → Variable (hasil instruksi).
    slot_vars: HashMap<u32, Variable>,
    /// slot MIR → tipe hasil.
    slot_types: HashMap<u32, SlotTy>,
    /// variabel lokal MIR → Variable + tipe.
    var_vars: HashMap<u32, (Variable, SlotTy)>,
    /// label MIR → block.
    labels: HashMap<u32, cranelift_codegen::ir::Block>,
    /// block saat ini sudah punya terminator.
    terminated: bool,
}

impl<'a> FuncCodegen<'a> {
    /// Codegen seluruh body; instruksi setelah terminator di-skip
    /// (unreachable — mis. `Jump(merge)` setelah `return`).
    fn emit_body(&mut self, body: &[MirInst], ret: MirTy) -> GnResult<()> {
        let mut entered = false;
        for (i, inst) in body.iter().enumerate() {
            match inst {
                MirInst::Label(n) => {
                    let b = self.block_for(*n);
                    if !entered || self.terminated {
                        // Block baru (block pertama fungsi, atau setelah
                        // terminator) — instruksi berikutnya masuk ke sini.
                        self.builder.switch_to_block(b);
                        entered = true;
                        self.terminated = false;
                    } else {
                        return Err(GnError::Other(format!(
                            "Label({n}) tanpa terminator sebelumnya (invalid MIR)"
                        )));
                    }
                }
                other => {
                    if self.terminated {
                        // Unreachable — skip.
                        continue;
                    }
                    self.emit_inst(i as u32, other)?;
                }
            }
        }
        if !self.terminated {
            self.emit_default_return(ret);
        }
        Ok(())
    }

    /// Path tanpa terminator di akhir → return nilai default.
    fn emit_default_return(&mut self, ret: MirTy) {
        let value = match ret {
            MirTy::Void => {
                self.builder.ins().return_(&[]);
                return;
            }
            MirTy::F64 => self.builder.ins().f64const(0f64),
            MirTy::Bool => self.builder.ins().iconst(types::I8, 0),
            MirTy::Str | MirTy::Ptr => self.builder.ins().iconst(self.pointer, 0),
        };
        self.builder.ins().return_(&[value]);
        self.terminated = true;
    }

    /// Block untuk label (create kalau belum ada).
    fn block_for(&mut self, label: u32) -> cranelift_codegen::ir::Block {
        match self.labels.get(&label) {
            Some(b) => *b,
            None => {
                let b = self.builder.create_block();
                self.labels.insert(label, b);
                b
            }
        }
    }

    /// Simpan hasil SSA ke variabel slot + catat tipe.
    fn def_slot(&mut self, slot: u32, value: cranelift_codegen::ir::Value, ty: SlotTy) {
        let abi = self.slot_abi(ty);
        let v = self.builder.declare_var(abi);
        self.builder.def_var(v, value);
        self.slot_vars.insert(slot, v);
        self.slot_types.insert(slot, ty);
    }

    /// Simpan hasil SSA ke variabel slot tanpa tipe SlotTy (mis. handle i32).
    fn def_slot_raw(&mut self, slot: u32, value: cranelift_codegen::ir::Value, abi: types::Type) {
        let v = self.builder.declare_var(abi);
        self.builder.def_var(v, value);
        self.slot_vars.insert(slot, v);
    }

    /// Ambil nilai slot (use_var).
    fn use_slot(&mut self, slot: u32) -> cranelift_codegen::ir::Value {
        let v = self.slot_vars[&slot];
        self.builder.use_var(v)
    }

    /// Tipe Cranelift dari SlotTy.
    fn slot_abi(&self, ty: SlotTy) -> types::Type {
        match ty {
            SlotTy::F64 => types::F64,
            SlotTy::Bool => types::I8,
            SlotTy::Str | SlotTy::Ptr => self.pointer,
        }
    }

    /// Simpan nilai ke variabel lokal MIR (declare tipe sekali).
    fn store_var(&mut self, var: u32, val: cranelift_codegen::ir::Value, ty: SlotTy) {
        let v = match self.var_vars.get(&var) {
            Some((v, _)) => *v,
            None => {
                let nv = self.builder.declare_var(self.slot_abi(ty));
                self.var_vars.insert(var, (nv, ty));
                nv
            }
        };
        self.builder.def_var(v, val);
    }

    /// Muat nilai variabel lokal MIR.
    fn load_var(&mut self, var: u32) -> cranelift_codegen::ir::Value {
        // Param fungsi: variabel 0..param_count terdefinisi saat entry.
        let v = match self.var_vars.get(&var) {
            Some((v, _)) => *v,
            None => {
                // Variabel tanpa definisi → default number 0.
                let nv = self.builder.declare_var(types::F64);
                self.var_vars.insert(var, (nv, SlotTy::F64));
                nv
            }
        };
        self.builder.use_var(v)
    }

    /// Codegen satu instruksi; `slot` = index asli di body.
    fn emit_inst(&mut self, slot: u32, inst: &MirInst) -> GnResult<()> {
        match inst {
            MirInst::ConstF64(n) => {
                let v = self.builder.ins().f64const(*n);
                self.def_slot(slot, v, SlotTy::F64);
            }
            MirInst::ConstBool(b) => {
                let v = self.builder.ins().iconst(types::I8, i64::from(*b));
                self.def_slot(slot, v, SlotTy::Bool);
            }
            MirInst::StrAddr(idx) => {
                let gv = self
                    .module
                    .declare_data_in_func(self.data_ids[*idx], self.builder.func);
                let addr = self.builder.ins().symbol_value(self.pointer, gv);
                self.def_slot(slot, addr, SlotTy::Str);
            }
            MirInst::BinF64 { op, a, b } => {
                let av = self.use_slot(*a);
                let bv = self.use_slot(*b);
                let v = match op {
                    MirBinOp::Add => self.builder.ins().fadd(av, bv),
                    MirBinOp::Sub => self.builder.ins().fsub(av, bv),
                    MirBinOp::Mul => self.builder.ins().fmul(av, bv),
                    MirBinOp::Div => self.builder.ins().fdiv(av, bv),
                };
                self.def_slot(slot, v, SlotTy::F64);
            }
            MirInst::CmpF64 { op, a, b } => {
                let av = self.use_slot(*a);
                let bv = self.use_slot(*b);
                let cc = match op {
                    MirCmpOp::Eq => FloatCC::Equal,
                    MirCmpOp::Ne => FloatCC::NotEqual,
                    MirCmpOp::Lt => FloatCC::LessThan,
                    MirCmpOp::Le => FloatCC::LessThanOrEqual,
                    MirCmpOp::Gt => FloatCC::GreaterThan,
                    MirCmpOp::Ge => FloatCC::GreaterThanOrEqual,
                };
                // Cranelift 0.134: fcmp/icmp menghasilkan i8 (0/1).
                let v = self.builder.ins().fcmp(cc, av, bv);
                self.def_slot(slot, v, SlotTy::Bool);
            }
            MirInst::CmpBool { op, a, b } => {
                let av = self.use_slot(*a);
                let bv = self.use_slot(*b);
                let cc = match op {
                    MirCmpOp::Eq => IntCC::Equal,
                    MirCmpOp::Ne => IntCC::NotEqual,
                    MirCmpOp::Lt | MirCmpOp::Le | MirCmpOp::Gt | MirCmpOp::Ge => {
                        unreachable!("relasional hanya untuk f64")
                    }
                };
                let v = self.builder.ins().icmp(cc, av, bv);
                self.def_slot(slot, v, SlotTy::Bool);
            }
            MirInst::CmpStr { op, a, b } => {
                let av = self.use_slot(*a);
                let bv = self.use_slot(*b);
                let cc = match op {
                    MirCmpOp::Eq => IntCC::Equal,
                    MirCmpOp::Ne => IntCC::NotEqual,
                    MirCmpOp::Lt | MirCmpOp::Le | MirCmpOp::Gt | MirCmpOp::Ge => {
                        unreachable!("relasional string belum didukung")
                    }
                };
                // Pointer equality — string di-intern di pool, jadi pointer
                // sama ⇔ konten sama (strcmp-semantik).
                let v = self.builder.ins().icmp(cc, av, bv);
                self.def_slot(slot, v, SlotTy::Bool);
            }
            MirInst::Not(a) => {
                let av = self.use_slot(*a);
                let v = self.builder.ins().icmp_imm_u(IntCC::Equal, av, 0);
                self.def_slot(slot, v, SlotTy::Bool);
            }
            MirInst::StoreVar { var, val } => {
                let vv = self.use_slot(*val);
                let ty = self.slot_types.get(val).copied().unwrap_or(SlotTy::F64);
                self.store_var(*var, vv, ty);
            }
            MirInst::LoadVar(var) => {
                let v = self.load_var(*var);
                let ty = self
                    .var_vars
                    .get(var)
                    .map(|(_, t)| *t)
                    .unwrap_or(SlotTy::F64);
                self.def_slot(slot, v, ty);
            }
            MirInst::Call { callee, args } => {
                let mut arg_vals = Vec::new();
                for a in args {
                    arg_vals.push(self.use_slot(*a));
                }
                // Intrinsik runtime (C-ABI dari libruntime): `puts`,
                // `gn_window_*`, `gn_scene_*` — dipanggil via FuncRef terdaftar.
                if let Some(fref) = self.runtime_funcs.get(callee) {
                    let inst = self.builder.ins().call(*fref, &arg_vals);
                    if let Some(rt) = self.runtime_ret_clif.get(callee) {
                        let v = self.builder.inst_results(inst)[0];
                        self.def_slot_raw(slot, v, *rt);
                    }
                } else {
                    let fid = self.func_ids[callee];
                    let local = self.module.declare_func_in_func(fid, self.builder.func);
                    let inst = self.builder.ins().call(local, &arg_vals);
                    let ret = self
                        .sigs
                        .get(callee)
                        .map(|(_, r)| *r)
                        .unwrap_or(MirTy::Void);
                    if ret != MirTy::Void {
                        let v = self.builder.inst_results(inst)[0];
                        self.def_slot(slot, v, mir_ty_to_slot(ret));
                    }
                }
            }
            MirInst::WindowCreate {
                title,
                width,
                height,
            } => {
                let t = self.use_slot(*title);
                let len = self.builder.ins().iconst(types::I64, 0); // len diabaikan (C-string)
                let w = self.builder.ins().iconst(types::I32, i64::from(*width));
                let h = self.builder.ins().iconst(types::I32, i64::from(*height));
                let inst = self.builder.ins().call(self.local_create, &[t, len, w, h]);
                let handle = self.builder.inst_results(inst)[0];
                self.def_slot_raw(slot, handle, types::I32);
            }
            MirInst::WindowRun(handle) => {
                let h = self.use_slot(*handle);
                self.builder.ins().call(self.local_run, &[h]);
            }
            MirInst::FuncAddr { func } => {
                // Alamat fungsi se-object (untuk registrasi handler/build).
                let fid = self.func_ids[func];
                let fref = self.module.declare_func_in_func(fid, self.builder.func);
                let v = self.builder.ins().func_addr(self.pointer, fref);
                self.def_slot(slot, v, SlotTy::Ptr);
            }
            MirInst::CallIndirect {
                target,
                params,
                ret,
                args,
            } => {
                // Panggil alamat fungsi (signature dari `params`/`ret`).
                let sig = make_sig(self.module, params, *ret);
                let sigref = self.builder.import_signature(sig);
                let tgt = self.use_slot(*target);
                let mut arg_vals = Vec::new();
                for a in args {
                    arg_vals.push(self.use_slot(*a));
                }
                let inst = self.builder.ins().call_indirect(sigref, tgt, &arg_vals);
                if *ret != MirTy::Void {
                    let v = self.builder.inst_results(inst)[0];
                    self.def_slot(slot, v, mir_ty_to_slot(*ret));
                }
            }
            MirInst::Ret(value) => {
                match value {
                    Some(slot) => {
                        let v = self.use_slot(*slot);
                        self.builder.ins().return_(&[v]);
                    }
                    None => {
                        self.builder.ins().return_(&[]);
                    }
                }
                self.terminated = true;
            }
            MirInst::Jump(label) => {
                let b = self.block_for(*label);
                self.builder.ins().jump(b, &[]);
                self.terminated = true;
            }
            MirInst::Branch { cond, then, els } => {
                // brif menerima "ScalarTruthy" (int apa pun) di Cranelift 0.134.
                let c = self.use_slot(*cond);
                let tb = self.block_for(*then);
                let eb = self.block_for(*els);
                self.builder.ins().brif(c, tb, &[], eb, &[]);
                self.terminated = true;
            }
            MirInst::Label(_) => unreachable!("Label ditangani di emit_body"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{MirFunc, MirModule};

    #[test]
    fn emits_and_or_short_circuit() {
        // Pola `a && b`: Branch(a, then, els); then → b; els → false;
        // merge → hasil lewat variabel sementara.
        let mut body: Vec<MirInst> = Vec::new();
        let a = body.len() as u32;
        body.push(MirInst::ConstBool(true));
        let then = 50u32;
        let els = 51u32;
        let merge = 52u32;
        body.push(MirInst::Branch { cond: a, then, els });
        body.push(MirInst::Label(then));
        let b = body.len() as u32;
        body.push(MirInst::ConstBool(false));
        body.push(MirInst::StoreVar { var: 0, val: b });
        body.push(MirInst::Jump(merge));
        body.push(MirInst::Label(els));
        let f = body.len() as u32;
        body.push(MirInst::ConstBool(false));
        body.push(MirInst::StoreVar { var: 0, val: f });
        body.push(MirInst::Jump(merge));
        body.push(MirInst::Label(merge));
        let res = body.len() as u32;
        body.push(MirInst::LoadVar(0));
        body.push(MirInst::Ret(Some(res)));

        let mir = MirModule {
            strings: vec![],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Bool,
                body,
            }],
        };
        let dir = std::env::temp_dir().join(format!("gnative-cl3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("app.o");

        let backend = CraneliftBackend;
        backend.codegen(&mir, None, &out).expect("codegen sukses");
        assert_eq!(&std::fs::read(&out).unwrap()[0..4], b"\x7fELF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emits_cross_target_object() {
        // Cross-compile: aarch64 object (Cranelift multi-arch, tanpa toolkit).
        let mir = MirModule {
            strings: vec![],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Void,
                body: vec![MirInst::Ret(None)],
            }],
        };
        let dir = std::env::temp_dir().join(format!("gnative-cl4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("app.o");

        let backend = CraneliftBackend;
        backend
            .codegen(&mir, Some("aarch64-unknown-linux-gnu"), &out)
            .expect("codegen cross-target sukses");
        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[0..4], b"\x7fELF");
        // ELF e_machine aarch64 = 183 (0xB7).
        assert_eq!(bytes[18], 0xB7, "e_machine harus aarch64");
        let _ = std::fs::remove_dir_all(&dir);
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
        let dir = std::env::temp_dir().join(format!("gnative-cl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("app.o");

        let backend = CraneliftBackend;
        backend.codegen(&mir, None, &out).expect("codegen sukses");
        assert!(out.exists());
        let bytes = std::fs::read(&out).unwrap();
        // ELF magic 0x7f 'E' 'L' 'F'
        assert_eq!(&bytes[0..4], b"\x7fELF");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn emits_function_with_if_and_call() {
        // max(a, b) = if (a == 0) { 1 } else { a + 1 }; main memanggil max(0, 0).
        let mut func: Vec<MirInst> = Vec::new();
        // slot 0: a (param), 1: b (param) — variabel 0 & 1 adalah param.
        let cond = func.len() as u32;
        func.push(MirInst::LoadVar(0)); // cond: a
        let zero = func.len() as u32;
        func.push(MirInst::ConstF64(0.0));
        let cmp = func.len() as u32;
        func.push(MirInst::CmpF64 {
            op: MirCmpOp::Eq,
            a: cond,
            b: zero,
        });
        let then = 100u32;
        let els = 101u32;
        let merge = 102u32;
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
                        MirInst::ConstF64(0.0), // slot 0
                        MirInst::ConstF64(0.0), // slot 1
                        MirInst::Call {
                            callee: "max".into(),
                            args: vec![0, 1],
                        }, // slot 2 (hasil dibuang)
                        MirInst::Ret(None),
                    ],
                },
            ],
        };
        let dir = std::env::temp_dir().join(format!("gnative-cl2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("app.o");

        let backend = CraneliftBackend;
        backend.codegen(&mir, None, &out).expect("codegen sukses");
        assert_eq!(&std::fs::read(&out).unwrap()[0..4], b"\x7fELF");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
