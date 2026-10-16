//! Perintah `build` — jalankan pipeline kompilasi native
//! (`src/*.gn` → object file → linker → binary native).

use std::path::Path;

use gnative_compiler::artifact;
use gnative_compiler::BuildMode;

use crate::log;

/// Flag `--strip`: buang debug info dari binary (ukuran turun drastis karena
/// libruntime di-link statis beserta debug info-nya).
const FLAG_STRIP: &str = "--strip";

/// Flag `--split-debug`: simpan debug info ke `<binary>.debug` + debuglink
/// (butuh `--strip`; bila hanya `--split-debug` diberikan, strip implisit).
const FLAG_SPLIT_DEBUG: &str = "--split-debug";

pub fn run(args: &[String]) -> i32 {
    let mode = BuildMode::from_args(args);
    let target = match BuildMode::target_from_args(args) {
        Ok(t) => t,
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    let split_debug = args.iter().any(|a| a == FLAG_SPLIT_DEBUG);
    let do_strip = split_debug || args.iter().any(|a| a == FLAG_STRIP);

    let project = Path::new(".");
    log::debug(format!(
        "build: mode={:?}, target={}, strip={}, split_debug={}",
        mode,
        target.as_deref().unwrap_or("host"),
        do_strip,
        split_debug
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
            if do_strip {
                match artifact::strip(bin, split_debug) {
                    Ok(rep) => {
                        println!(
                            "  binary: {} ({} → {} setelah strip, hemat {})",
                            bin.display(),
                            artifact::human_size(rep.original_size),
                            artifact::human_size(rep.final_size),
                            artifact::human_size(rep.saved())
                        );
                        if let Some(dbg) = &rep.debug_file {
                            println!("  debug:  {}", dbg.display());
                        }
                    }
                    Err(e) => {
                        // Flag eksplisit diminta → jangan diam-diam sukses.
                        log::error(format!("strip gagal: {e}"));
                        return 1;
                    }
                }
            } else {
                println!("  binary: {}", bin.display());
                log::debug(
                    "petunjuk: tambahkan `--strip` (atau `--split-debug`) untuk \
                     membuang debug info dan mengecilkan binary",
                );
            }
            0
        }
        Err(e) => {
            log::error(format!("build gagal: {e}"));
            1
        }
    }
}
