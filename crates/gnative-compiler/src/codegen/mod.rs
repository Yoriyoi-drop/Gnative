//! Codegen native — MIR → object file.
//!
//! Dua backend:
//! - `cranelift` (mode dev, default) — cepat, murni Rust.
//! - `llvm` (mode release) — optimasi agresif via LLVM 18 sistem.
//!
//! Pola mengikuti `gnative-gfx`: trait backend + pemilihan di pipeline.

pub mod cranelift;
pub mod llvm;

use std::path::Path;

use gnative_core::{GnError, GnResult};

use crate::mir::MirModule;

/// Mode build → pemilihan backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    /// Backend Cranelift (cepat, murni Rust) — default.
    Dev,
    /// Backend LLVM (optimasi agresif, butuh LLVM 18) — release.
    Release,
}

impl BuildMode {
    pub fn from_args(args: &[String]) -> Self {
        if args.iter().any(|a| a == "--release") {
            BuildMode::Release
        } else {
            BuildMode::Dev
        }
    }

    /// Triple target dari `--target <triple>`; `None` = host.
    pub fn target_from_args(args: &[String]) -> Option<String> {
        args.iter()
            .position(|a| a == "--target")
            .map(|i| args.get(i + 1).cloned().unwrap_or_else(|| "".to_string()))
    }
}

/// Kontrak backend codegen native.
pub trait CodegenBackend {
    fn name(&self) -> &'static str;
    /// Terima MIR → tulis object file (ELF/Mach-O/COFF) ke `out`.
    /// `target` = triple target (mis. `aarch64-unknown-linux-gnu`);
    /// `None` = host ISA.
    fn codegen(&self, mir: &MirModule, target: Option<&str>, out: &Path) -> GnResult<()>;
}

/// Pilih backend sesuai mode build.
pub fn backend_for(mode: BuildMode) -> Box<dyn CodegenBackend> {
    match mode {
        BuildMode::Dev => Box::new(cranelift::CraneliftBackend),
        BuildMode::Release => Box::new(llvm::LlvmBackend),
    }
}

/// Jalankan codegen untuk mode + target tertentu (`None` = host).
pub fn codegen(mode: BuildMode, target: Option<&str>, mir: &MirModule, out: &Path) -> GnResult<()> {
    let backend = backend_for(mode);
    backend
        .codegen(mir, target, out)
        .map_err(|e| GnError::Other(format!("codegen ({}) gagal: {e}", backend.name())))
}
