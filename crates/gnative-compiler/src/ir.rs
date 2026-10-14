//! Semantic IR — representasi antara AST `.gn` dan MIR.
//!
//! AST hasil `gnative-lang` masih sarat detail sintaksis (span, elemen
//! deklaratif, ekspresi). Semantic IR menyederhanakannya menjadi struktur
//! yang siap di-lower ke MIR: satu window + fungsi-fungsi top-level + komponen.
//!
//! Milestone 0.8: IR mendukung fungsi nyata (params + return), `let`,
//! `if`/`else`, `for`, dan ekspresi (aritmatika, perbandingan, panggilan).
//! Milestone 0.9: komponen dengan `state` (field + method) — dikompilasi ke
//! instance runtime dengan state tersendiri.

/// Modul IR: hasil lowering satu project `.gn`.
#[derive(Debug, Clone, Default)]
pub struct IrModule {
    /// Fungsi top-level selain window.
    pub funcs: Vec<IrFunc>,
    /// Komponen dengan state (runtime component).
    pub components: Vec<IrComponent>,
    /// Window aplikasi (jika ada).
    pub window: Option<IrWindow>,
}

/// Komponen IR — definisi komponen dengan state + method.
/// Di-monomorphize per instance pemakaian (`Comp__0__method`); render
/// di-lower per situs (dengan props disubstitusi) ke dalam `ComponentInstance`.
#[derive(Debug, Clone)]
pub struct IrComponent {
    pub name: String,
    /// Field state: `state { count: number = 0; }`
    pub state_fields: Vec<IrStateField>,
    /// Method lain (mis. `inc(): void`) → salinan `Comp__i__method` per instance.
    pub methods: Vec<IrFunc>,
}

/// Kunci state/dispatch satu instance: `"Counter#0"`.
/// Disematkan ke string (ABI C-ABI tetap string) agar runtime tanpa perubahan.
pub fn instance_key(comp: &str, inst: usize) -> String {
    format!("{comp}#{inst}")
}

/// Satu field state: `count: number = 0;`
#[derive(Debug, Clone)]
pub struct IrStateField {
    pub name: String,
    pub ty: IrTy,
    pub init: IrExpr,
}

/// Fungsi IR — siap di-lower ke MIR dan dikompilasi native.
#[derive(Debug, Clone)]
pub struct IrFunc {
    pub name: String,
    pub params: Vec<IrParam>,
    /// Tipe return; `None` = void.
    pub ret: Option<IrTy>,
    pub body: Vec<IrStmt>,
}

/// Parameter fungsi.
#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: String,
    pub ty: IrTy,
}

/// Tipe primitif yang didukung codegen native.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrTy {
    /// `number` — direpresentasikan f64 (seperti JS).
    F64,
    /// `boolean`.
    Bool,
    /// `string` — pointer ke string pool (immutable).
    Str,
}

impl IrTy {
    /// Konversi nama tipe `.gn` → `IrTy`; `None` untuk `void`/tidak dikenal.
    pub fn from_name(name: &str) -> Option<IrTy> {
        match name {
            "number" => Some(IrTy::F64),
            "boolean" => Some(IrTy::Bool),
            "string" => Some(IrTy::Str),
            "void" => None,
            _ => None,
        }
    }
}

/// Pernyataan IR.
#[derive(Debug, Clone)]
pub enum IrStmt {
    /// `let name = expr;` (tanpa init → default sesuai tipe/inferensi).
    Let {
        name: String,
        ty: Option<IrTy>,
        value: Option<IrExpr>,
    },
    /// `if (cond) { } else { }`.
    If {
        cond: IrExpr,
        then_branch: Vec<IrStmt>,
        else_branch: Vec<IrStmt>,
    },
    /// `for (var of [item, ...]) { }` — iterasi array literal konstan.
    For {
        var: String,
        items: Vec<IrExpr>,
        body: Vec<IrStmt>,
    },
    /// `return expr;` (tanpa nilai → `None`).
    Return(Option<IrExpr>),
    /// Pernyataan ekspresi (assignment, panggilan, ...).
    Expr(IrExpr),
    /// Cetak string literal ke stdout (intrinsik `puts` — debug/headless).
    PutStr(String),
}

/// Ekspresi IR.
#[derive(Debug, Clone)]
pub enum IrExpr {
    F64(f64),
    Bool(bool),
    Str(String),
    /// Referensi variabel lokal/param.
    Var(String),
    /// Akses field state komponen: `this.field` atau `field` di dalam method.
    StateField(String),
    /// `-x`.
    Neg(Box<IrExpr>),
    /// `!x`.
    Not(Box<IrExpr>),
    /// Aritmatika / perbandingan / logika.
    Bin(IrBinOp, Box<IrExpr>, Box<IrExpr>),
    /// Panggil fungsi top-level atau intrinsik (`puts`).
    Call(String, Vec<IrExpr>),
    /// `x = value` (target variabel lokal atau state field).
    Assign(String, Box<IrExpr>),
    /// `this.method(...)` — panggil method komponen pada instance.
    MethodCall {
        receiver: Box<IrExpr>,
        method: String,
        args: Vec<IrExpr>,
    },
}

/// Operator biner IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    /// Perbandingan relasional (f64).
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// `window Name { title; size; root }`.
#[derive(Debug, Clone)]
pub struct IrWindow {
    /// Judul window — ekspresi string (literal atau hasil `Call`).
    pub title: IrExpr,
    pub size: (f64, f64),
    pub root: IrNode,
}

/// Node scene hasil evaluasi elemen deklaratif (subset built-in).
#[derive(Debug, Clone)]
pub enum IrNode {
    /// `<text content={...} size={n}/>` — konten bisa ekspresi string.
    Text { content: IrExpr, size: f64 },
    /// `<button label={...} onClick={this.m}/>` (`onClick` hanya di render stateful).
    Button {
        label: IrExpr,
        on_click: Option<IrClick>,
    },
    /// `<input placeholder={...}/>`
    Input { placeholder: IrExpr },
    /// `<vstack gap={n}>children</vstack>`
    VStack { gap: f64, children: Vec<IrNode> },
    /// Instance komponen dengan state: `<Counter awal="x"/>`.
    /// `render` sudah disubstitusi props + di-lower dengan konteks state
    /// instance ini; di-inline saat MIR beserta init state.
    ComponentInstance {
        component_name: String,
        /// Nomor instance per komponen (urutan jumpa saat lowering).
        instance: usize,
        props: Vec<(String, IrExpr)>,
        /// Pohon UI instance (bisa merujuk state via `StateField`).
        render: Box<IrNode>,
    },
}

/// Aksi klik: panggil method instance (`onClick={this.inc}`).
#[derive(Debug, Clone)]
pub struct IrClick {
    /// Kunci instance (`instance_key`) pemilik method.
    pub component: String,
    /// Nama method (harus tanpa parameter, dipanggil sebagai `Comp__i__m()`).
    pub method: String,
}
