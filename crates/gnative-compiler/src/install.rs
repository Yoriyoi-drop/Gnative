//! Instalasi package project (analog `npm install`).
//!
//! Baca `gnative.toml` → untuk tiap dependency, pastikan package tersedia
//! di `container/packages`. Package bawaan compiler (`@gnative/ui`)
//! di-seed dari runtime compiler (`runtime/`); package lain yang belum
//! tersedia dilaporkan `missing` (roadmap: fetch dari registry).

use std::collections::BTreeMap;
use std::path::Path;

use gnative_core::{GnError, GnResult};

use crate::config::Config;

/// Lokasi package bawaan compiler (`runtime/@scope/name/`).
pub const RUNTIME_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime");

/// Laporan hasil `install`.
#[derive(Debug, Default)]
pub struct InstallReport {
    /// Package baru di-seed ke `container/packages`.
    pub installed: Vec<String>,
    /// Package sudah ada (tidak diubah).
    pub up_to_date: Vec<String>,
    /// Package tidak tersedia (roadmap: registry fetch).
    pub missing: Vec<String>,
}

/// Install semua dependency yang terdaftar di `gnative.toml`.
pub fn install(project_dir: &Path) -> GnResult<InstallReport> {
    let config = Config::load(&project_dir.join("gnative.toml"))?;
    let container = project_dir.join("container");

    // Dependency utama + dev-dependency (lokal) sama-sama diinstall.
    let mut deps: BTreeMap<String, String> = config.dependencies.clone();
    for (k, v) in &config.dev_dependencies {
        deps.entry(k.clone()).or_insert_with(|| v.clone());
    }

    let mut report = InstallReport::default();
    for name in deps.keys() {
        let dest = container.join("packages").join(name);
        if package_exists(&dest) {
            report.up_to_date.push(name.clone());
            continue;
        }
        let src = Path::new(RUNTIME_DIR).join(name);
        if src.is_dir() {
            copy_tree(&src, &dest)?;
            report.installed.push(name.clone());
        } else {
            report.missing.push(name.clone());
        }
    }
    Ok(report)
}

/// Cek apakah package sudah punya minimal satu file `.gn`.
fn package_exists(dir: &Path) -> bool {
    fn walk(dir: &Path) -> bool {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    if walk(&p) {
                        return true;
                    }
                } else if p.extension().map(|e| e == "gn").unwrap_or(false) {
                    return true;
                }
            }
        }
        false
    }
    walk(dir)
}

/// Salin isi direktori secara rekursif.
fn copy_tree(src: &Path, dest: &Path) -> GnResult<()> {
    std::fs::create_dir_all(dest).map_err(GnError::Io)?;
    for entry in std::fs::read_dir(src).map_err(GnError::Io)? {
        let entry = entry.map_err(GnError::Io)?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(GnError::Io)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_builtin_package() {
        let dir = std::env::temp_dir().join(format!("gnative-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("gnative.toml"), Config::template("demo")).unwrap();

        let report = install(&dir).unwrap();
        assert_eq!(report.installed, vec!["@gnative/ui"]);
        assert!(report.missing.is_empty());

        let ui = dir.join("container/packages/@gnative/ui/ui.gn");
        assert!(ui.exists(), "package @gnative/ui ter-seed");

        // Install kedua → up-to-date, tidak duplikat.
        let report2 = install(&dir).unwrap();
        assert_eq!(report2.up_to_date, vec!["@gnative/ui"]);
        assert!(report2.installed.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_package_reported_missing() {
        let dir = std::env::temp_dir().join(format!("gnative-install2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("gnative.toml"),
            r#"[project]
name = "demo"

[dependencies]
"@gnative/nonexistent" = "0.1.0"
"#,
        )
        .unwrap();

        let report = install(&dir).unwrap();
        assert_eq!(report.missing, vec!["@gnative/nonexistent"]);
        assert!(report.installed.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
