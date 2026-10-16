//! Pembacaan `Cargo.lock` — inventaris dependency yang benar-benar dipakai.
//!
//! Tanggung jawab tunggal: mengubah `Cargo.lock` menjadi daftar paket
//! (`name`, `version`, `source`) plus pemeriksaan struktural murah yang tidak
//! butuh basis data eksternal (sumber non-registry, versi ganda).
//!
//! Dipakai oleh `gnative audit` (lihat `crates/gnative-cli/src/cmd/audit.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gnative_core::{GnError, GnResult};

/// Indeks registry resmi (kriteria "dari crates.io").
pub const CRATES_IO_INDEX: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Satu entri `[[package]]` di `Cargo.lock`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockPackage {
    /// Nama crate.
    pub name: String,
    /// Versi persis yang terkunci.
    pub version: String,
    /// Sumber; `None` untuk crate anggota workspace (path lokal).
    pub source: Option<String>,
}

impl LockPackage {
    /// True bila berasal dari registry crates.io (sumber tepercaya bawaan).
    pub fn is_crates_io(&self) -> bool {
        self.source.as_deref() == Some(CRATES_IO_INDEX)
    }

    /// True bila crate anggota workspace (tanpa `source`).
    pub fn is_workspace_member(&self) -> bool {
        self.source.is_none()
    }
}

/// Isi `Cargo.lock` yang sudah di-parse.
#[derive(Debug, Clone, Default)]
pub struct Lockfile {
    /// Semua paket, urut sebagaimana di berkas.
    pub packages: Vec<LockPackage>,
}

impl Lockfile {
    /// Cari `Cargo.lock` naik dari `start` (project → root repo).
    pub fn find(start: &Path) -> Option<PathBuf> {
        let root = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
        let mut dir = Some(root.as_path());
        while let Some(current) = dir {
            let candidate = current.join("Cargo.lock");
            if candidate.is_file() {
                return Some(candidate);
            }
            dir = current.parent();
        }
        None
    }

    /// Muat dari berkas.
    pub fn load(path: &Path) -> GnResult<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| GnError::Config(format!("tidak bisa membaca {}: {e}", path.display())))?;
        Self::parse(&text)
    }

    /// Parse isi `Cargo.lock`.
    pub fn parse(text: &str) -> GnResult<Self> {
        let value: toml::Value = toml::from_str(text)
            .map_err(|e| GnError::Config(format!("Cargo.lock tidak valid: {e}")))?;
        let mut packages = Vec::new();
        if let Some(entries) = value.get("package").and_then(|v| v.as_array()) {
            for entry in entries {
                let name = entry.get("name").and_then(|v| v.as_str());
                let version = entry.get("version").and_then(|v| v.as_str());
                let (Some(name), Some(version)) = (name, version) else {
                    // Entri tanpa name/version = lock rusak; jangan diam-diam.
                    return Err(GnError::Config(
                        "Cargo.lock: entri `[[package]]` tanpa `name`/`version`".into(),
                    ));
                };
                packages.push(LockPackage {
                    name: name.to_string(),
                    version: version.to_string(),
                    source: entry
                        .get("source")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                });
            }
        }
        Ok(Lockfile { packages })
    }

    /// Paket dari luar crates.io (git / registry alternatif).
    pub fn non_registry(&self) -> Vec<&LockPackage> {
        self.packages
            .iter()
            .filter(|p| match &p.source {
                Some(s) => s != CRATES_IO_INDEX,
                None => false,
            })
            .collect()
    }

    /// Crate yang punya lebih dari satu versi terkunci (`name` → versi-versi),
    /// terurut menurut nama. Versi ganda memperbesar permukaan serangan dan
    /// ukuran binary.
    pub fn duplicates(&self) -> Vec<(String, Vec<String>)> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for p in &self.packages {
            map.entry(p.name.clone())
                .or_default()
                .push(p.version.clone());
        }
        map.into_iter()
            .filter(|(_, versions)| versions.len() > 1)
            .map(|(name, mut versions)| {
                versions.sort();
                versions.dedup();
                (name, versions)
            })
            .collect()
    }

    /// Cari paket dengan nama + versi tertentu.
    pub fn get(&self, name: &str, version: &str) -> Option<&LockPackage> {
        self.packages
            .iter()
            .find(|p| p.name == name && p.version == version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
version = 4

[[package]]
name = "gnative-core"
version = "1.0.0-rc.1"

[[package]]
name = "wgpu"
version = "30.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "abc"

[[package]]
name = "wgpu"
version = "30.0.1"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "curang"
version = "0.1.0"
source = "git+https://example.com/curang#deadbeef"
"#;

    #[test]
    fn parse_menghitung_paket() {
        let lock = Lockfile::parse(SAMPLE).unwrap();
        assert_eq!(lock.packages.len(), 4);
        assert!(lock.get("gnative-core", "1.0.0-rc.1").is_some());
    }

    #[test]
    fn non_registry_terdeteksi() {
        let lock = Lockfile::parse(SAMPLE).unwrap();
        let nr = lock.non_registry();
        assert_eq!(nr.len(), 1);
        assert_eq!(nr[0].name, "curang");
        // Crate workspace (tanpa source) bukan "non-registry".
        assert!(!nr.iter().any(|p| p.name == "gnative-core"));
    }

    #[test]
    fn duplikat_versi_terdeteksi() {
        let lock = Lockfile::parse(SAMPLE).unwrap();
        let dup = lock.duplicates();
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].0, "wgpu");
        assert_eq!(dup[0].1, vec!["30.0.0", "30.0.1"]);
    }

    #[test]
    fn entri_rusak_ditolak() {
        assert!(Lockfile::parse("[[package]]\nname = \"x\"\n").is_err());
    }

    #[test]
    fn is_crates_io_benar() {
        let lock = Lockfile::parse(SAMPLE).unwrap();
        assert!(lock.get("wgpu", "30.0.0").unwrap().is_crates_io());
        assert!(!lock.get("curang", "0.1.0").unwrap().is_crates_io());
        assert!(lock
            .get("gnative-core", "1.0.0-rc.1")
            .unwrap()
            .is_workspace_member());
    }
}
