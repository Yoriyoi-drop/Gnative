//! Perintah `run` — kompilasi native lalu jalankan binary hasil link.
//!
//! Binary `.gn` memanggil intrinsik C-ABI dari libruntime (wgpu primary /
//! CUDA fallback) yang dibundel saat link, sehingga `run` membuka
//! window native nyata.

use std::path::Path;

use gnative_compiler::BuildMode;

use crate::{log, process};

pub fn run(args: &[String]) -> i32 {
    // 1. Pipeline kompilasi native.
    let mode = BuildMode::from_args(args);
    let target = match BuildMode::target_from_args(args) {
        Ok(t) => t,
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    let report = match gnative_compiler::build_with_target(Path::new("."), mode, target.as_deref())
    {
        Ok(report) => report,
        Err(e) => {
            log::error(format!("build gagal: {e}"));
            return 1;
        }
    };

    for d in &report.diagnostics {
        log::warn(d);
    }
    if report.has_errors {
        log::error("build gagal (lihat error di atas)");
        return 1;
    }

    // 2. Jalankan binary hasil link (verifikasi path + laporan sinyal).
    let Some(binary) = report.binary else {
        log::error("tidak ada binary untuk dijalankan");
        return 1;
    };
    process::run_binary(&binary)
}
