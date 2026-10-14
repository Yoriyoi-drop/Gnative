//! Backend LLVM (mode release) — STUB.
//!
//! LLVM dipilih untuk build release (optimasi maksimal). Implementasi nyata
//! membutuhkan toolkit LLVM (`llvm-sys`/`inkwell`) — roadmap milestone 0.8.
//! Saat fitur dipanggil, kembalikan `GnError::BackendUnavailable` yang jelas
//! (pola sama dengan `CudaRenderer::new()` tanpa fitur `cuda`).

use std::path::Path;

use gnative_core::{GnError, GnResult};

use super::CodegenBackend;
use crate::mir::MirModule;

/// Backend LLVM — release build (belum tersedia).
pub struct LlvmBackend;

impl CodegenBackend for LlvmBackend {
    fn name(&self) -> &'static str {
        "llvm"
    }

    fn codegen(&self, _mir: &MirModule, _target: Option<&str>, _out: &Path) -> GnResult<()> {
        Err(GnError::BackendUnavailable(
            "backend LLVM belum diimplementasikan (roadmap 0.8). \
             Gunakan mode dev (Cranelift): `gnative build` / `gnative run`"
                .into(),
        ))
    }
}
