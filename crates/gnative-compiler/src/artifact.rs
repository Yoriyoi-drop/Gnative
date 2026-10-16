//! Pasca-link artefak: strip debug info, pisah debug (`.debug`), dan lapor ukuran.
//!
//! Tanggung jawab tunggal: setelah binary native selesai di-link, modul ini
//! mengurus *packaging* artefak supaya distribusi masuk akal. Libruntime
//! (`libgnative_runtime.a`) di-link statis beserta debug info-nya, sehingga
//! binary hasil `gnative build` bisa berukuran ratusan MB. Modul ini
//! menyediakan:
//!
//! - `strip` — buang debug info (tool `strip`/`llvm-strip`).
//! - pisah debug — `objcopy --only-keep-debug` + `--add-gnu-debuglink`
//!   sehingga simbol tetap dapat di-debug dari berkas `.debug` terpisah.
//!
//! Semua operasi memakai tool sistem yang sudah ada (tanpa dependency baru).
//! Bila tool tidak tersedia, hasilnya error jelas — bukan sukses semu.

use std::path::{Path, PathBuf};
use std::process::Command;

use gnative_core::{GnError, GnResult};

/// Nama tool strip yang dicoba, berurutan.
const STRIP_TOOLS: &[&str] = &["strip", "llvm-strip"];

/// Nama tool objcopy yang dicoba, berurutan.
const OBJCOPY_TOOLS: &[&str] = &["objcopy", "llvm-objcopy"];

/// Hasil pemrosesan artefak pasca-link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactReport {
    /// Ukuran binary sebelum diproses (byte).
    pub original_size: u64,
    /// Ukuran binary setelah diproses (byte).
    pub final_size: u64,
    /// True bila debug info dibuang dari binary.
    pub stripped: bool,
    /// Berkas debug terpisah, bila diminta dan berhasil dibuat.
    pub debug_file: Option<PathBuf>,
}

impl ArtifactReport {
    /// Byte yang dihemat (0 bila tidak ada perubahan).
    pub fn saved(&self) -> u64 {
        self.original_size.saturating_sub(self.final_size)
    }
}

/// Cari tool pertama yang tersedia di PATH.
fn find_tool(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find(|candidate| {
            Command::new(candidate)
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .map(|s| s.to_string())
}

/// Jalankan perintah, kembalikan error jelas bila gagal (tanpa panic).
fn run_tool(tool: &str, args: &[&str]) -> GnResult<()> {
    let out = Command::new(tool)
        .args(args)
        .output()
        .map_err(|e| GnError::Other(format!("menjalankan `{tool}` gagal: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(GnError::Other(format!(
            "`{tool} {}` gagal: {}",
            args.join(" "),
            stderr.trim()
        )));
    }
    Ok(())
}

/// Ukuran berkas dalam byte (0 bila tidak dapat dibaca).
fn size_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Buang debug info dari `binary`; bila `split_debug`, simpan debug info ke
/// `<binary>.debug` + tautkan lewat `--add-gnu-debuglink`.
///
/// Mengembalikan [`ArtifactReport`] dengan ukuran sebelum/sesudah. Error bila
/// tool tidak tersedia atau gagal — pemanggil memutuskan apakah itu fatal.
pub fn strip(binary: &Path, split_debug: bool) -> GnResult<ArtifactReport> {
    if !binary.is_file() {
        return Err(GnError::Other(format!(
            "artefak `{}` tidak ada — tidak bisa di-strip",
            binary.display()
        )));
    }
    let original_size = size_of(binary);

    let strip_tool = find_tool(STRIP_TOOLS).ok_or_else(|| {
        GnError::BackendUnavailable(
            "tidak ada tool `strip`/`llvm-strip` di PATH — tidak bisa membuang debug info".into(),
        )
    })?;

    let binary_str = binary.to_string_lossy().to_string();
    let mut debug_file = None;

    if split_debug {
        let objcopy = find_tool(OBJCOPY_TOOLS).ok_or_else(|| {
            GnError::BackendUnavailable(
                "tidak ada tool `objcopy`/`llvm-objcopy` di PATH — tidak bisa memisah debug info"
                    .into(),
            )
        })?;
        let debug_path = PathBuf::from(format!("{binary_str}.debug"));
        let debug_str = debug_path.to_string_lossy().to_string();

        // 1) Simpan seluruh debug info ke berkas terpisah.
        run_tool(&objcopy, &["--only-keep-debug", &binary_str, &debug_str])?;
        // 2) Buang debug info dari binary.
        run_tool(&strip_tool, &["--strip-debug", &binary_str])?;
        // 3) Tautkan binary → berkas debug (agar gdb bisa menemukannya).
        let link_arg = format!("--add-gnu-debuglink={debug_str}");
        // Tautan best-effort: bila gagal, debug tetap ada (hanya tak otomatis).
        let _ = run_tool(&objcopy, &[&link_arg, &binary_str]);
        debug_file = Some(debug_path);
    } else {
        run_tool(&strip_tool, &["--strip-debug", &binary_str])?;
    }

    Ok(ArtifactReport {
        original_size,
        final_size: size_of(binary),
        stripped: true,
        debug_file,
    })
}

/// Format ukuran byte menjadi string ringkas (mis. `218.0 MB`).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_memformat() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn strip_berkas_hilang_error() {
        let err = strip(Path::new("/tidak/ada/berkas-ini"), false);
        assert!(err.is_err(), "berkas hilang harus error, bukan panic");
    }

    #[test]
    fn report_saved_tidak_underflow() {
        let r = ArtifactReport {
            original_size: 100,
            final_size: 250,
            stripped: false,
            debug_file: None,
        };
        assert_eq!(r.saved(), 0);
    }

    #[test]
    fn strip_binary_nyata_mengecil() {
        // Binary nyata dari libruntime: strip harus berhasil dan tidak membesar.
        // Di-skip bila tool strip tidak ada (bukan kegagalan test).
        let Some(_) = find_tool(STRIP_TOOLS) else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("gnative-strip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("contoh");
        std::fs::copy(std::env::current_exe().unwrap(), &bin).unwrap();

        let report = strip(&bin, false).unwrap();
        assert!(report.stripped);
        assert!(
            report.final_size <= report.original_size,
            "strip tidak boleh memperbesar: {} → {}",
            report.original_size,
            report.final_size
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
