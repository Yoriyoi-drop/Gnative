//! Helper menjalankan binary hasil build (P3-06).
//!
//! Verifikasi path sebelum spawn (hindari TOCTOU/`ENOENT` mentah) dan
//! laporkan penghentian oleh sinyal (bukan menyamarkan jadi exit 1).

use std::path::Path;
use std::process::{Command, ExitStatus};

use crate::log;

/// Jalankan binary dengan verifikasi path; kembalikan exit code.
pub fn run_binary(binary: &Path) -> i32 {
    if !binary.is_file() {
        log::error(format!("binary tidak ditemukan: {}", binary.display()));
        return 1;
    }
    log::info(format!("menjalankan {} ...", binary.display()));
    match Command::new(binary).status() {
        Ok(status) => exit_code(status),
        Err(e) => {
            log::error(format!("gagal menjalankan binary: {e}"));
            1
        }
    }
}

/// Exit code dari `ExitStatus`: `code`, atau `128 + sinyal` bila dihentikan
/// sinyal (Unix); 1 bila tak diketahui.
pub fn exit_code(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            log::warn(format!("binary dihentikan sinyal {sig}"));
            return 128 + sig;
        }
    }
    1
}
