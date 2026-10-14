//! Perintah `list` / `ls` — analog `npm list`.
//!
//! Tampilkan dependency dari `gnative.toml` + status keberadaan di
//! `container/packages`.

use std::path::Path;

pub fn run(_args: &[String]) -> i32 {
    let config = match gnative_compiler::config::Config::load(Path::new("gnative.toml")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("list gagal: {e}");
            return 1;
        }
    };

    println!("{}@{}", config.project_name, config.project_version);
    if config.dependencies.is_empty() {
        println!("└── (tidak ada dependency)");
        return 0;
    }

    let container = Path::new("container");
    let count = config.dependencies.len();
    for (i, (name, version)) in config.dependencies.iter().enumerate() {
        let last = i + 1 == count;
        let branch = if last { "└──" } else { "├──" };
        let pkg_dir = container.join("packages").join(name);
        let status = if pkg_dir.join("ui.gn").exists() || pkg_dir.join("src").is_dir() {
            "terpasang"
        } else {
            "belum terpasang"
        };
        println!("{branch} {name}@{version} ({status})");
    }
    0
}
