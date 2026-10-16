//! Resolusi `use "@gnative/ui"` → path package di `container/packages`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gnative_core::{safe_join, GnError, GnResult};

/// Hasil resolusi: daftar package yang dibutuhkan.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// package → path lokasi source di container/packages.
    pub packages: BTreeMap<String, PathBuf>,
    /// package yang tidak ditemukan (roadmap: fetch dari registry).
    pub missing: Vec<String>,
}

/// Resolver namespace-style (`@scope/name`).
pub struct Resolver;

impl Resolver {
    /// Cari file `.gn` untuk setiap `use`.
    ///
    /// - `@gnative/xxx` → `container/packages/@gnative/xxx`
    /// - `@project/xxx` → `container/packages/@project/xxx` (atau src/)
    pub fn resolve(imports: &[String], container: &Path, src_dir: &Path) -> GnResult<Resolution> {
        let mut resolution = Resolution::default();
        for import in imports {
            match resolve_package(import, container, src_dir) {
                Ok(path) => {
                    resolution.packages.insert(import.clone(), path);
                }
                Err(_) => resolution.missing.push(import.clone()),
            }
        }
        Ok(resolution)
    }
}

fn resolve_package(import: &str, container: &Path, src_dir: &Path) -> GnResult<PathBuf> {
    // Namespace dengan scoped name: "@scope/name"
    if let Some(rest) = import.strip_prefix('@') {
        let (scope, name) = rest
            .split_once('/')
            .ok_or_else(|| GnError::Config(format!("package `{import}` harus `@scope/name`")))?;
        let candidate = safe_join(&container.join("packages"), &format!("@{scope}/{name}"))?;
        let main = candidate.join("src");
        if main.is_dir() {
            return Ok(main);
        }
        if candidate.is_dir() {
            return Ok(candidate);
        }
        return Err(GnError::Config(format!(
            "package `{import}` tidak ditemukan di {}",
            candidate.display()
        )));
    }
    // Import lokal tanpa scope → relatif ke src/.
    let local = safe_join(src_dir, import)?;
    if local.is_file() || local.is_dir() {
        return Ok(local);
    }
    Err(GnError::Config(format!(
        "`use` `{import}` tidak ditemukan (dicoba `{}`)",
        local.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unscoped_import_reported_missing() {
        // Import tanpa scope (@scope/name) → dilaporkan sebagai missing.
        let resolution = Resolver::resolve(
            &["ui".to_string()],
            Path::new("container"),
            Path::new("src"),
        )
        .unwrap();
        assert!(resolution.missing.contains(&"ui".to_string()));
        assert!(resolution.packages.is_empty());
    }

    #[test]
    fn resolved_scoped_package() {
        // Buat package @gnative/ui dummy.
        let dir = std::env::temp_dir().join(format!("gnative-resolve-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let container = dir.join("container");
        let pkg = container.join("packages/@gnative/ui");
        std::fs::create_dir_all(pkg.join("src")).unwrap();
        std::fs::write(pkg.join("src/button.gn"), "component button {}").unwrap();

        let resolution =
            Resolver::resolve(&["@gnative/ui".to_string()], &container, &dir.join("src")).unwrap();
        assert!(resolution.packages.contains_key("@gnative/ui"));
        assert!(resolution.missing.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn traversal_ditolak_jadi_missing() {
        // P0-09: `use` dengan `..` tak boleh keluar container/src → dilaporkan
        // missing, bukan resolve ke path di luar root.
        let resolution = Resolver::resolve(
            &["@scope/../../etc".to_string(), "../rahasia".to_string()],
            Path::new("container"),
            Path::new("src"),
        )
        .unwrap();
        assert!(resolution.packages.is_empty());
        assert_eq!(resolution.missing.len(), 2);
    }
}
