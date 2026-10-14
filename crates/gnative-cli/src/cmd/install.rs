//! Perintah `install` — analog `npm install`.
//!
//! Baca `gnative.toml`, seed package bawaan compiler ke `container/packages`,
//! dan lapor package yang belum tersedia (roadmap: registry fetch).

use std::path::Path;

pub fn run(_args: &[String]) -> i32 {
    let report = match gnative_compiler::install::install(Path::new(".")) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("install gagal: {e}");
            return 1;
        }
    };

    for pkg in &report.installed {
        println!("+ {pkg}");
    }
    for pkg in &report.up_to_date {
        println!("  {pkg} (sudah ada)");
    }
    for pkg in &report.missing {
        println!("! {pkg} tidak tersedia (roadmap: fetch dari registry)");
    }

    if !report.missing.is_empty() {
        eprintln!("install selesai dengan package yang hilang");
        return 1;
    }
    println!(
        "install selesai ({} dipasang, {} up-to-date)",
        report.installed.len(),
        report.up_to_date.len()
    );
    0
}
