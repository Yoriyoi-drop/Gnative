//! Perintah `run` — kompilasi native lalu jalankan binary hasil link.
//!
//! Binary `.gn` memanggil intrinsik C-ABI dari libruntime (wgpu primary /
//! CUDA fallback) yang dibundel saat link, sehingga `run` membuka
//! window native nyata.

use std::path::Path;
use std::process::Command;

use gnative_compiler::BuildMode;

pub fn run(args: &[String]) -> i32 {
    // 1. Pipeline kompilasi native.
    let mode = BuildMode::from_args(args);
    let target = BuildMode::target_from_args(args).filter(|t| !t.is_empty());
    let report = match gnative_compiler::build_with_target(Path::new("."), mode, target.as_deref())
    {
        Ok(report) => report,
        Err(e) => {
            eprintln!("build gagal: {e}");
            return 1;
        }
    };

    for d in &report.diagnostics {
        eprintln!("  {d}");
    }
    if report.has_errors {
        eprintln!("build gagal (lihat error di atas)");
        return 1;
    }

    // 2. Jalankan binary hasil link.
    let Some(binary) = report.binary else {
        eprintln!("tidak ada binary untuk dijalankan");
        return 1;
    };
    println!("menjalankan {} ...", binary.display());
    match Command::new(&binary).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("gagal menjalankan binary: {e}");
            1
        }
    }
}
