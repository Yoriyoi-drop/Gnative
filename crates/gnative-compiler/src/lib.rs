//! # gnative-compiler
//!
//! Pipeline kompilasi native: `src/*.gn` → **binary native**.
//!
//! ```text
//! AST (gnative-lang) → Semantic IR → MIR → Cranelift (dev) | LLVM (release)
//!                     → object file → linker → binary
//! ```
//!
//! Satu file = satu tanggung jawab (lihat DESIGN.md §10):
//! - `config.rs`      → parse `gnative.toml`
//! - `resolve.rs`     → resolusi `use` → path package
//! - `graph.rs`       → dependency graph + deteksi cycle
//! - `fingerprint.rs` → hash deterministik source
//! - `cache.rs`       → cache incremental (`container/cache`)
//! - `ir.rs`          → Semantic IR (representasi antara)
//! - `lower.rs`       → AST → Semantic IR
//! - `mir.rs`         → MIR (instruksi linear)
//! - `mir_lower.rs`   → Semantic IR → MIR
//! - `codegen/`       → MIR → object file (Cranelift / LLVM)
//! - `linker.rs`      → object → binary native
//! - `pipeline.rs`    → orkestrasi seluruh build

pub mod cache;
pub mod codegen;
pub mod config;
pub mod fingerprint;
pub mod graph;
pub mod install;
pub mod ir;
pub mod linker;
pub mod lower;
pub mod mir;
pub mod mir_lower;
pub mod pipeline;
pub mod resolve;

pub use codegen::BuildMode;
pub use pipeline::{build, build_with_target, BuildReport};
