//! MIR (Mid-level IR) — instruksi linear sebelum codegen native.
//!
//! MIR adalah target bersama backend Cranelift (dev) dan LLVM (release):
//! satu modul berisi string pool + fungsi-fungsi (`main` sebagai entry).
//!
//! **Model nilai (slot):** setiap instruksi menempati satu slot = index-nya
//! di `body`. Instruksi yang menghasilkan nilai (`ConstF64`, `LoadVar`,
//! `Call`, ...) menjadikan slot itu sebagai hasilnya; instruksi lain
//! mereferensikan hasil lewat index slot tersebut.
//!
//! **Kontrol aliran:** CFG dinyatakan dengan `Label`/`Jump`/`Branch`.
//! Invariant: setiap `Label` (kecuali yang pertama) didahului terminator
//! (`Jump`/`Branch`/`Ret`), sehingga codegen bisa menutup block sebelumnya.

/// Modul MIR: kumpulan fungsi + string pool.
#[derive(Debug, Clone, Default)]
pub struct MirModule {
    pub funcs: Vec<MirFunc>,
    /// String pool — direferensikan instruksi lewat index.
    pub strings: Vec<String>,
}

/// Fungsi MIR.
#[derive(Debug, Clone)]
pub struct MirFunc {
    /// Nama fungsi. `"main"` = entry point binary.
    pub name: String,
    /// Tipe parameter (urutan signature).
    pub params: Vec<MirTy>,
    /// Tipe return; `Void` = tidak ada nilai.
    pub ret: MirTy,
    pub body: Vec<MirInst>,
}

/// Tipe primitif MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirTy {
    /// `number` — f64.
    F64,
    /// `boolean` — 1 byte (0/1).
    Bool,
    /// `string` — pointer ke string pool (immutable).
    Str,
    /// Pointer buram (alamat fungsi untuk registrasi handler/build).
    Ptr,
    /// Tidak ada nilai (void).
    Void,
}

/// Instruksi MIR — subset yang didukung backend native.
#[derive(Debug, Clone)]
pub enum MirInst {
    /// Penanda block; target `Jump`/`Branch` memakai nomor label.
    Label(u32),
    /// Lompat tak bersyarat ke label.
    Jump(u32),
    /// Percabangan: `cond != 0` → label `then`, selain itu label `els`.
    Branch { cond: u32, then: u32, els: u32 },
    /// Konstanta angka (f64).
    ConstF64(f64),
    /// Konstanta boolean.
    ConstBool(bool),
    /// Alamat string pool (hasil: pointer ke data `str_{i}`).
    StrAddr(usize),
    /// Aritmatika f64: hasil = a op b.
    BinF64 { op: MirBinOp, a: u32, b: u32 },
    /// Perbandingan f64 (hasil: bool).
    CmpF64 { op: MirCmpOp, a: u32, b: u32 },
    /// Perbandingan boolean (hasil: bool).
    CmpBool { op: MirCmpOp, a: u32, b: u32 },
    /// Perbandingan string (pointer equality — string di-intern di pool,
    /// jadi pointer sama ⇔ konten sama). Hasil: bool.
    CmpStr { op: MirCmpOp, a: u32, b: u32 },
    /// Negasi boolean (hasil: bool).
    Not(u32),
    /// Simpan nilai ke variabel lokal.
    StoreVar { var: u32, val: u32 },
    /// Muat nilai variabel lokal (hasil).
    LoadVar(u32),
    /// Panggil fungsi (top-level atau intrinsik `puts`). Hasil = return value.
    Call { callee: String, args: Vec<u32> },
    /// Panggil tak-langsung via alamat (`FuncAddr`/pointer): dipakai uji
    /// pemanggilan handler terregistrasi. Hasil = return value.
    CallIndirect {
        target: u32,
        params: Vec<MirTy>,
        ret: MirTy,
        args: Vec<u32>,
    },
    /// Alamat fungsi MIR lain (untuk registrasi handler/build ke runtime).
    /// Hasil: pointer buram (`Ptr`).
    FuncAddr { func: String },
    /// Panggil intrinsik `gn_window_create(title_ptr, title_len, w, h)`.
    /// `title_len` = panjang byte judul bila diketahui literal, `0` bila judul
    /// dinamis (runtime jatuh ke pembacaan C-string berbatas). Hasil = handle.
    WindowCreate {
        title: u32,
        title_len: u32,
        width: u32,
        height: u32,
    },
    /// Panggil intrinsik `gn_window_run(handle)` (blocking event loop).
    WindowRun(u32),
    /// `return`; `Some(slot)` = nilai, `None` = void.
    Ret(Option<u32>),
}

/// Operator biner MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// Operator perbandingan MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirCmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// Tipe hasil sebuah slot (untuk codegen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotTy {
    F64,
    Bool,
    Str,
    Ptr,
}

impl SlotTy {
    /// Tipe hasil dari instruksi di index `slot` (dengan asumsi body valid).
    pub fn for_inst(inst: &MirInst) -> Option<SlotTy> {
        match inst {
            MirInst::ConstF64(_) | MirInst::BinF64 { .. } => Some(SlotTy::F64),
            MirInst::ConstBool(_)
            | MirInst::CmpF64 { .. }
            | MirInst::CmpBool { .. }
            | MirInst::Not(_) => Some(SlotTy::Bool),
            MirInst::StrAddr(_) => Some(SlotTy::Str),
            MirInst::FuncAddr { .. } => Some(SlotTy::Ptr),
            // Instruksi tanpa hasil.
            _ => None,
        }
    }
}
