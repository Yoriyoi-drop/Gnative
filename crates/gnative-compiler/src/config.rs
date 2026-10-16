//! Konfigurasi project: `gnative.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gnative_core::{GnError, GnResult};

/// Versi compiler ini — satu-satunya sumber kebenaran untuk `gnative.toml`
/// yang dihasilkan (`[gnative].version`) dan untuk paket runtime bawaan
/// `@gnative/ui`. Wajib dipakai agar project hasil `gnative new` selalu
/// kompatibel dengan compiler yang membuatnya (audit: template tak boleh
/// langsung gagal di cek versi pipeline).
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `gnative.toml` yang sudah di-parse.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub project_name: String,
    pub project_version: String,
    pub gnative_version: String,
    /// dependency: nama → versi (`@gnative/ui` → versi compiler, lihat
    /// [`COMPILER_VERSION`]).
    pub dependencies: BTreeMap<String, String>,
    /// dev-dependency lokal (path).
    pub dev_dependencies: BTreeMap<String, String>,
    /// Peringatan parse (mis. `[project]`/`name` hilang → default dipakai)
    /// agar default tak diterapkan diam-diam (audit P1-12).
    pub warnings: Vec<String>,
}

impl Config {
    /// Muat dari `gnative.toml`.
    pub fn load(path: &Path) -> GnResult<Self> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| GnError::Config(format!("tidak bisa membaca {}: {e}", path.display())))?;
        Self::parse(&text)
    }

    /// Parse isi TOML.
    pub fn parse(text: &str) -> GnResult<Self> {
        let value: toml::Value = toml::from_str(text)
            .map_err(|e| GnError::Config(format!("gnative.toml tidak valid: {e}")))?;
        let mut cfg = Config::default();

        match value.get("project").and_then(|v| v.as_table()) {
            Some(project) => {
                cfg.project_name = match project.get("name").and_then(|v| v.as_str()) {
                    Some(n) => n.to_string(),
                    None => {
                        cfg.warnings
                            .push("`[project].name` tidak ada — memakai default `app`".into());
                        "app".to_string()
                    }
                };
                cfg.project_version = match project.get("version").and_then(|v| v.as_str()) {
                    Some(v) => v.to_string(),
                    None => {
                        cfg.warnings
                            .push("`[project].version` tidak ada — memakai default `0.1.0`".into());
                        "0.1.0".to_string()
                    }
                };
            }
            None => {
                cfg.warnings.push(
                    "bagian `[project]` tidak ada — memakai default (name=`app`, version=`0.1.0`)"
                        .into(),
                );
                cfg.project_name = "app".to_string();
                cfg.project_version = "0.1.0".to_string();
            }
        }
        if let Some(gnative) = value.get("gnative").and_then(|v| v.as_table()) {
            // Fallback = versi compiler sendiri: `[gnative]` ada tapi tanpa
            // `version` tidak boleh menghasilkan mismatch major palsu.
            cfg.gnative_version = gnative
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or(COMPILER_VERSION)
                .to_string();
        }
        if let Some(deps) = value.get("dependencies").and_then(|v| v.as_table()) {
            for (k, v) in deps {
                if let Some(ver) = v.as_str() {
                    cfg.dependencies.insert(k.clone(), ver.to_string());
                }
            }
        }
        if let Some(deps) = value.get("dev-dependencies").and_then(|v| v.as_table()) {
            for (k, v) in deps {
                if let Some(ver) = v.as_str() {
                    cfg.dev_dependencies.insert(k.clone(), ver.to_string());
                }
            }
        }
        Ok(cfg)
    }

    /// Default `gnative.toml` untuk project baru.
    ///
    /// `[gnative].version` dan versi paket runtime bawaan `@gnative/ui`
    /// memakai [`COMPILER_VERSION`] — bukan konstanta hardcode — supaya project
    /// hasil `gnative new` tidak pernah langsung gagal cek versi pipeline.
    pub fn template(name: &str) -> String {
        format!(
            r#"[project]
name = "{name}"
version = "0.1.0"

[gnative]
version = "{version}"

[dependencies]
"@gnative/ui" = "{version}"
"#,
            version = COMPILER_VERSION
        )
    }
}

/// Lokasi standar `gnative.toml`.
pub fn config_path(project_dir: &Path) -> PathBuf {
    project_dir.join("gnative.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_template() {
        let cfg = Config::parse(&Config::template("demo")).unwrap();
        assert_eq!(cfg.project_name, "demo");
        assert_eq!(
            cfg.dependencies.get("@gnative/ui").map(String::as_str),
            Some(COMPILER_VERSION)
        );
    }

    #[test]
    fn template_selalu_kompatibel_dengan_compiler() {
        // Regresi: `gnative new` tidak boleh menghasilkan project yang
        // langsung ditolak cek versi pipeline (major berbeda).
        let cfg = Config::parse(&Config::template("demo")).unwrap();
        assert_eq!(cfg.gnative_version, COMPILER_VERSION);
        let major = |v: &str| v.split('.').next().unwrap_or("").to_string();
        assert_eq!(major(&cfg.gnative_version), major(COMPILER_VERSION));
    }

    #[test]
    fn gnative_tanpa_version_pakai_versi_compiler() {
        let cfg = Config::parse("[gnative]\n").unwrap();
        assert_eq!(cfg.gnative_version, COMPILER_VERSION);
    }

    #[test]
    fn invalid_toml() {
        assert!(Config::parse("{{{{").is_err());
    }

    #[test]
    fn project_hilang_memberi_warning() {
        // P1-12: `[project]` hilang → default dipakai tapi TIDAK diam.
        let cfg = Config::parse("[dependencies]\n\"@gnative/ui\" = \"0.1.0\"\n").unwrap();
        assert_eq!(cfg.project_name, "app");
        assert!(
            cfg.warnings.iter().any(|w| w.contains("[project]")),
            "harus ada warning default, warnings: {:?}",
            cfg.warnings
        );
    }

    #[test]
    fn name_hilang_memberi_warning() {
        let cfg = Config::parse("[project]\nversion = \"1.2.3\"\n").unwrap();
        assert_eq!(cfg.project_name, "app");
        assert!(cfg.warnings.iter().any(|w| w.contains("name")));
    }
}
