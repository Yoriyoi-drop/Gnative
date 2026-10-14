//! Cache incremental: `container/cache/fingerprint/<file>.hash`.
//!
//! Menyimpan fingerprint per source file. Jika fingerprint tidak berubah,
//! kompiler bisa melewati fase parse → codegen (cache hit).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gnative_core::{GnError, GnResult};

use crate::fingerprint::to_hex;

/// Cache fingerprint sederhana di atas filesystem.
pub struct Cache {
    root: PathBuf,
    entries: BTreeMap<String, String>,
}

impl Cache {
    /// Buka (atau buat) cache di `container/cache/fingerprint`.
    pub fn open(container: &Path) -> GnResult<Self> {
        let root = container.join("cache").join("fingerprint");
        std::fs::create_dir_all(&root).map_err(GnError::Io)?;
        let mut cache = Self {
            root,
            entries: BTreeMap::new(),
        };
        cache.load_all()?;
        Ok(cache)
    }

    /// Apakah source sudah pernah diproses dengan fingerprint ini?
    ///
    /// Key di-normalisasi `cache_key` agar konsisten dengan yang dibaca
    /// `load_all` (file_stem dari `<key>.hash`).
    pub fn is_fresh(&self, rel_path: &str, target: Option<&str>, fingerprint: u64) -> bool {
        self.entries.get(&cache_key(rel_path, target)) == Some(&to_hex(fingerprint))
    }

    /// Catat fingerprint baru.
    pub fn record(
        &mut self,
        rel_path: &str,
        target: Option<&str>,
        fingerprint: u64,
    ) -> GnResult<()> {
        let hex = to_hex(fingerprint);
        let key = cache_key(rel_path, target);
        self.entries.insert(key.clone(), hex.clone());
        let file = self.root.join(format!("{key}.hash"));
        std::fs::write(&file, hex).map_err(GnError::Io)?;
        Ok(())
    }

    fn load_all(&mut self) -> GnResult<()> {
        if !self.root.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(&self.root).map_err(GnError::Io)? {
            let entry = entry.map_err(GnError::Io)?;
            if entry
                .path()
                .extension()
                .map(|e| e == "hash")
                .unwrap_or(false)
            {
                if let Ok(hex) = std::fs::read_to_string(entry.path()) {
                    let name = entry
                        .path()
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.entries.insert(name, hex.trim().to_string());
                }
            }
        }
        Ok(())
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }
}

/// Key normal dari path source + target (`src/main.gn` @ host →
/// `src_main.gn`, @ `aarch64-unknown-linux-gnu` → `src_main.gn_aarch64...`).
///
/// Target disertakan agar cache tidak mencampur object arch berbeda.
/// Key dipakai sebagai lookup internal DAN file_stem dari `<key>.hash`,
/// sehingga konsisten lintas sesi (record → load_all).
pub fn cache_key(rel_path: &str, target: Option<&str>) -> String {
    let mut name = String::with_capacity(rel_path.len() + 16);
    for c in rel_path.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
            name.push(c);
        } else {
            name.push('_');
        }
    }
    if let Some(t) = target {
        name.push('_');
        for c in t.chars() {
            if c.is_ascii_alphanumeric() || c == '-' {
                name.push(c);
            } else {
                name.push('_');
            }
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_then_stale() {
        let dir = std::env::temp_dir().join(format!("gnative-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cache = Cache::open(&dir).unwrap();
        assert!(!cache.is_fresh("src/main.gn", None, 42));
        cache.record("src/main.gn", None, 42).unwrap();
        assert!(cache.is_fresh("src/main.gn", None, 42));
        assert!(!cache.is_fresh("src/main.gn", None, 43));
        // Target berbeda → tidak dianggap fresh (object arch berbeda).
        assert!(!cache.is_fresh("src/main.gn", Some("aarch64-unknown-linux-gnu"), 42));
        cache
            .record("src/main.gn", Some("aarch64-unknown-linux-gnu"), 42)
            .unwrap();
        assert!(cache.is_fresh("src/main.gn", Some("aarch64-unknown-linux-gnu"), 42));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
