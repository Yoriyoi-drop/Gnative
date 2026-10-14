//! Konfigurasi project: `gnative.toml`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gnative_core::{GnError, GnResult};

/// `gnative.toml` yang sudah di-parse.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub project_name: String,
    pub project_version: String,
    pub gnative_version: String,
    /// dependency: nama → versi (`@gnative/ui` → "0.1.0").
    pub dependencies: BTreeMap<String, String>,
    /// dev-dependency lokal (path).
    pub dev_dependencies: BTreeMap<String, String>,
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

        if let Some(project) = value.get("project").and_then(|v| v.as_table()) {
            cfg.project_name = project
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("app")
                .to_string();
            cfg.project_version = project
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.1.0")
                .to_string();
        }
        if let Some(gnative) = value.get("gnative").and_then(|v| v.as_table()) {
            cfg.gnative_version = gnative
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.1.0")
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
    pub fn template(name: &str) -> String {
        format!(
            r#"[project]
name = "{name}"
version = "0.1.0"

[gnative]
version = "0.1.0"

[dependencies]
"@gnative/ui" = "0.1.0"
"#
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
            Some("0.1.0")
        );
    }

    #[test]
    fn invalid_toml() {
        assert!(Config::parse("{{{{").is_err());
    }
}
