//! Perintah `build` — jalankan pipeline kompilasi native
//! (`src/*.gn` → object file → linker → binary native).

use std::path::Path;

use gnative_compiler::BuildMode;

pub fn run(args: &[String]) -> i32 {
    let mode = BuildMode::from_args(args);
    let target = BuildMode::target_from_args(args).filter(|t| !t.is_empty());
    let project = Path::new(".");
    match gnative_compiler::build_with_target(project, mode, target.as_deref()) {
        Ok(report) => {
            for d in &report.diagnostics {
                eprintln!("  {d}");
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
                println!("  object: {}", obj.display());
            }
            if let Some(bin) = &report.binary {
                println!("  binary: {}", bin.display());
            }
            0
        }
        Err(e) => {
            eprintln!("build gagal: {e}");
            1
        }
    }
}
