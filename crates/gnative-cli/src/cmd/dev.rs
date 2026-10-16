//! Perintah `dev` — analog `npm run dev` (watch + auto-restart).
//!
//! Pantau perubahan semua file `.gn` (src/ + container/packages), lalu
//! build ulang dan **restart** binary native secara otomatis. Realtime
//! tanpa perlu Ctrl+C di tiap iterasi.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use gnative_compiler::BuildMode;

use crate::log;

/// Interval polling perubahan file.
const POLL_MS: u64 = 300;
/// Jeda debounce: tunggu perubahan "mengendap" sebelum rebuild (P3-05).
const DEBOUNCE_MS: u64 = 200;

pub fn run(args: &[String]) -> i32 {
    let mode = BuildMode::from_args(args);
    let target = match BuildMode::target_from_args(args) {
        Ok(t) => t,
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    log::info("dev: memantau perubahan .gn (Ctrl+C untuk berhenti)");

    let mut running: Option<Child> = None;
    // Build pertama.
    match rebuild(&mode, target.as_deref()) {
        Ok(Some(binary)) => running = spawn(&binary),
        Ok(None) => {
            log::warn("dev: build pertama gagal — menunggu perubahan untuk mencoba lagi");
        }
        Err(e) => {
            log::error(format!("dev: build pertama gagal: {e}"));
        }
    }

    let mut last_state = snapshot();
    loop {
        sleep(Duration::from_millis(POLL_MS));
        let state = snapshot();
        if state == last_state {
            continue;
        }
        // Debounce: tunggu hingga tak ada perubahan lagi (editor sering
        // menulis beberapa kali per simpan — P3-05).
        let mut settled = state;
        loop {
            sleep(Duration::from_millis(DEBOUNCE_MS));
            let s = snapshot();
            if s == settled {
                break;
            }
            settled = s;
        }
        last_state = settled;
        log::info("dev: perubahan terdeteksi — rebuild + restart");

        // Hentikan binary lama sebelum build ulang (window harus ditutup
        // dulu agar berkas window tak terkunci).
        if let Some(child) = running.take() {
            let _ = kill(child);
        }

        match rebuild(&mode, target.as_deref()) {
            Ok(Some(binary)) => running = spawn(&binary),
            Ok(None) => {
                log::warn("dev: build gagal — menunggu perubahan berikutnya");
            }
            Err(e) => {
                log::error(format!("dev: build gagal: {e}"));
            }
        }
    }
}

/// Snapshot mtime seluruh file `.gn` yang dipantau.
fn snapshot() -> Vec<(PathBuf, u64)> {
    let mut files = collect_gn(Path::new("src"));
    if let Ok(pkgs) = std::fs::read_dir(Path::new("container/packages")) {
        for entry in pkgs.flatten() {
            files.extend(collect_gn(&entry.path()));
        }
    }
    let mut out: Vec<(PathBuf, u64)> = files
        .into_iter()
        .filter_map(|f| {
            std::fs::metadata(&f)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|d| (f, d.as_millis() as u64))
        })
        .collect();
    out.sort();
    out
}

/// Kumpulkan semua file `.gn` di bawah direktori.
fn collect_gn(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(collect_gn(&path));
            } else if path.extension().map(|e| e == "gn").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    out
}

/// Build ulang; kembalikan path binary bila sukses.
fn rebuild(mode: &BuildMode, target: Option<&str>) -> Result<Option<PathBuf>, String> {
    match gnative_compiler::build_with_target(Path::new("."), *mode, target) {
        Ok(report) => {
            for d in &report.diagnostics {
                log::warn(d);
            }
            if report.has_errors {
                return Ok(None);
            }
            Ok(report.binary)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Jalankan binary (verifikasi path — P3-06); `None` bila gagal spawn.
fn spawn(binary: &Path) -> Option<Child> {
    if !binary.is_file() {
        log::error(format!("dev: binary tidak ditemukan: {}", binary.display()));
        return None;
    }
    log::info(format!("dev: menjalankan {} ...", binary.display()));
    match Command::new(binary).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            log::error(format!("dev: gagal menjalankan binary: {e}"));
            None
        }
    }
}

/// Hentikan child: `kill()` mengirim SIGKILL di Unix; di Windows `taskkill /T`
/// juga mematikan proses anak (tree) sebelum `kill()` sebagai jaring pengaman.
fn kill(mut child: Child) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid, "/T", "/F"])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
