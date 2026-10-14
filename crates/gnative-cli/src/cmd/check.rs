//! Perintah `check` — validasi project tanpa codegen/link.
//!
//! Menjalankan fase parse + type check + lowering (via `pipeline::validate`),
//! sehingga error semantik (komponen tanpa `render()`, prop hilang, fungsi
//! gagal di-lower) terdeteksi — konsisten dengan `build`.

use std::path::Path;

pub fn run(_args: &[String]) -> i32 {
    let report = match gnative_compiler::pipeline::validate(Path::new(".")) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("check gagal: {e}");
            return 1;
        }
    };

    for d in &report.diagnostics {
        eprintln!("  {d}");
    }
    if report.has_errors {
        println!("check selesai: {} error", report.diagnostics.len());
        1
    } else {
        println!("check selesai: project OK");
        0
    }
}
