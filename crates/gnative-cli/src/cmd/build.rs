//! Perintah `build` — jalankan pipeline kompilasi native
//! (`src/*.gn` → object file → linker → binary native).

use std::path::Path;

use gnative_compiler::BuildMode;

use crate::log;

pub fn run(args: &[String]) -> i32 {
    let mode = BuildMode::from_args(args);
    let target = match BuildMode::target_from_args(args) {
        Ok(t) => t,
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    let project = Path::new(".");
    log::debug(format!(
        "build: mode={:?}, target={}",
        mode,
        target.as_deref().unwrap_or("host")
    ));
    match gnative_compiler::build_with_target(project, mode, target.as_deref()) {
        Ok(report) => {
            for d in &report.diagnostics {
                log::warn(d);
            }
            println!(
                "build selesai: {} file diproses, {} cache hit{}{}",
                report.regenerated,
                report.cache_hits,
                target
                    .as_deref()
                    .map(|t| format!(" untuk target {t}"))
                    .unwrap_or_default(),
                if report.has_errors {
                    " (ADA ERROR)"
                } else {
                    ""
                }
            );
            if report.has_errors {
                return 1;
            }
            if let Some(obj) = &report.object_file {
                log::debug(format!("object: {}", obj.display()));
            }
            // P1-12: build tanpa binary = sukses semu. Gagalkan dengan jelas
            // (fresh checkout tanpa libruntime harus exit non-0).
            let Some(bin) = &report.binary else {
                log::error(
                    "build gagal: binary native tidak dihasilkan \
                     (libruntime belum dibangun? jalankan `cargo build -p gnative-runtime`)",
                );
                return 1;
            };
            println!("  binary: {}", bin.display());
            0
        }
        Err(e) => {
            log::error(format!("build gagal: {e}"));
            1
        }
    }
}
