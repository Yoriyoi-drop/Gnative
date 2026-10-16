//! Codegen native — MIR → object file.
//!
//! Dua backend:
//! - `cranelift` (mode dev, default) — cepat, murni Rust.
//! - `llvm` (mode release) — optimasi agresif via LLVM 18 sistem.
//!
//! Pola mengikuti `gnative-gfx`: trait backend + pemilihan di pipeline.

pub mod cranelift;
pub mod llvm;

use std::collections::HashSet;
use std::path::Path;

use gnative_core::{GnError, GnResult};

use crate::mir::{MirInst, MirModule};

/// Nama intrinsik runtime yang benar-benar dipakai modul MIR (P2-06).
///
/// Dipakai backend agar **hanya** mendeklarasikan import yang diperlukan:
/// object fungsi murni (tanpa UI) jadi bebas referensi `gn_*`/`puts`.
pub fn used_intrinsics(mir: &MirModule) -> HashSet<String> {
    let mut used = HashSet::new();
    for f in &mir.funcs {
        for inst in &f.body {
            match inst {
                // `Call.callee` mencakup `puts` + semua intrinsik `gn_*` yang
                // dipanggil langsung (scene builder, state, register, run).
                MirInst::Call { callee, .. } => {
                    used.insert(callee.clone());
                }
                MirInst::WindowCreate { .. } => {
                    used.insert("gn_window_create".to_string());
                }
                MirInst::WindowRun(_) => {
                    used.insert("gn_window_run".to_string());
                }
                _ => {}
            }
        }
    }
    used
}

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

    /// Triple target dari `--target <triple>`.
    ///
    /// `Ok(None)` = host (flag tak ada). `Err` = flag ada tapi nilainya kosong
    /// atau hilang (P3-02: jangan diam-diam jatuh ke host).
    pub fn target_from_args(args: &[String]) -> Result<Option<String>, String> {
        match args.iter().position(|a| a == "--target") {
            None => Ok(None),
            Some(i) => match args.get(i + 1) {
                Some(t) if !t.is_empty() && !t.starts_with('-') => Ok(Some(t.clone())),
                _ => {
                    Err("`--target` butuh nilai (mis. `--target aarch64-unknown-linux-gnu`)".into())
                }
            },
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{MirFunc, MirInst, MirTy};

    #[test]
    fn fungsi_murni_tanpa_intrinsik() {
        let mir = MirModule {
            strings: vec![],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Void,
                body: vec![MirInst::Ret(None)],
            }],
        };
        assert!(used_intrinsics(&mir).is_empty());
    }

    #[test]
    fn intrinsik_window_terdeteksi() {
        let mir = MirModule {
            strings: vec!["t".into()],
            funcs: vec![MirFunc {
                name: "main".into(),
                params: vec![],
                ret: MirTy::Void,
                body: vec![
                    MirInst::StrAddr(0),
                    MirInst::WindowCreate {
                        title: 0,
                        title_len: 1,
                        width: 1,
                        height: 1,
                    },
                    MirInst::WindowRun(1),
                    MirInst::Ret(None),
                ],
            }],
        };
        let used = used_intrinsics(&mir);
        assert!(used.contains("gn_window_create"));
        assert!(used.contains("gn_window_run"));
        assert!(!used.contains("gn_scene_create"));
    }

    #[test]
    fn target_kosong_ditolak() {
        // P3-02: `--target` tanpa nilai tidak boleh diam-diam jadi host.
        assert!(BuildMode::target_from_args(&["--target".to_string()]).is_err());
        assert!(BuildMode::target_from_args(&["--target".to_string(), String::new()]).is_err());
        assert_eq!(BuildMode::target_from_args(&[]).unwrap(), None);
        assert_eq!(
            BuildMode::target_from_args(&[
                "--target".to_string(),
                "aarch64-unknown-linux-gnu".to_string()
            ])
            .unwrap(),
            Some("aarch64-unknown-linux-gnu".to_string())
        );
    }
}
