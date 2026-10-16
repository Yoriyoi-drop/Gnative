//! Utilitas path: cegah path traversal (P0-09).
//!
//! Nama package/dependency dari `gnative.toml`, statement `use` di `.gn`,
//! maupun arg CLI tidak boleh keluar dari root yang dituju. Fungsi di sini
//! menolak `..`, path absolut, dan separator yang mengecoh sebelum menggabung,
//! lalu memastikan hasil tetap di dalam root (secara leksikal, tanpa
//! bergantung canonicalize).

use std::path::{Component, Path, PathBuf};

use crate::error::{GnError, GnResult};

/// Gabung `root` + `rel` dengan aman.
///
/// Menolak bila `rel` absolut atau mengandung komponen `..`/`\`/`/` yang
/// memungkinkan keluar root. Hasil dijamin (leksikal) tetap di dalam `root`.
/// `rel` boleh mengandung `/` sebagai separator *internal* (mis. `@scope/name`).
pub fn safe_join(root: &Path, rel: &str) -> GnResult<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(GnError::Config(format!("path `{rel}` absolut (terlarang)")));
    }
    for c in rel_path.components() {
        match c {
            // `..` atau root (`/`, `C:\`) → tolak (potensi traversal).
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(GnError::Config(format!(
                    "path `{rel}` mengandung `..`/root (terlarang)"
                )));
            }
            _ => {}
        }
    }
    let joined = root.join(rel_path);
    if !joined.starts_with(root) {
        return Err(GnError::Config(format!(
            "path `{rel}` keluar dari root `{}` (terlarang)",
            root.display()
        )));
    }
    Ok(joined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn komponen_aman_diterima() {
        let root = Path::new("/proj/container/packages");
        assert!(safe_join(root, "@gnative/ui").is_ok());
        assert!(safe_join(root, "scope/name").is_ok());
        assert!(safe_join(root, "flat").is_ok());
    }

    #[test]
    fn traversal_ditolak() {
        let root = Path::new("/proj/container/packages");
        assert!(safe_join(root, "../../etc").is_err());
        assert!(safe_join(root, "../x").is_err());
        assert!(safe_join(root, "/abs/evil").is_err());
        assert!(safe_join(root, "a/../../b").is_err());
        // Separator Windows: hanya traversal di Windows (`\` bukan separator
        // di Unix, jadi di sana ia nama file biasa — tetap aman).
        #[cfg(windows)]
        assert!(safe_join(root, "a\\..\\b").is_err());
    }
}
