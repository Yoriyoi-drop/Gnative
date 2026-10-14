//! # gnative-lang
//!
//! Bahasa `.gn` — sintaks meniru TypeScript, dipangkas untuk kebutuhan GUI.
//! Pipeline: `lexer` → `parser` → `ast` → `checker`.
//!
//! Satu file = satu tanggung jawab (lihat DESIGN.md §10):
//! - `token.rs`      → token & jenis token
//! - `span.rs`       → posisi source (baris, kolom)
//! - `lexer.rs`      → teks → token
//! - `ast.rs`        → struktur AST
//! - `parser.rs`     → token → AST
//! - `type.rs`       → representasi tipe
//! - `checker.rs`    → validasi semantik & tipe
//! - `diagnostic.rs` → laporan error terformat

pub mod ast;
pub mod checker;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;
pub mod ty;

pub use ast::Module;
pub use diagnostic::Diagnostic;
pub use lexer::Lexer;
pub use parser::Parser;
pub use span::{LineColumn, Span};
pub use token::{Token, TokenKind};
pub use ty::Type;

/// Parse satu file `.gn` menjadi AST ter-validasi.
///
/// Mengembalikan modul + daftar diagnostic (kosong jika bersih).
pub fn parse_source(source: &str) -> (Module, Vec<Diagnostic>) {
    let tokens = Lexer::new(source).tokenize();
    let mut parser = Parser::new(tokens);
    let module = parser.parse_module();
    let mut diagnostics = parser.diagnostics;
    let checker = checker::Checker::new();
    let mut checked = module;
    checker.check(&mut checked, &mut diagnostics);
    (checked, diagnostics)
}
