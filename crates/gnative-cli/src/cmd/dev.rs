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

/// Interval polling perubahan file.
const POLL_MS: u64 = 300;

pub fn run(args: &[String]) -> i32 {
    let mode = BuildMode::from_args(args);
    let target = BuildMode::target_from_args(args).filter(|t| !t.is_empty());
    println!("[dev] memantau perubahan .gn (Ctrl+C untuk berhenti)");

    let mut running: Option<Child> = None;
    // Build pertama.
    match rebuild(&mode, target.as_deref()) {
        Ok(Some(binary)) => running = spawn(&binary),
        Ok(None) => {
            eprintln!("[dev] build pertama gagal — menunggu perubahan untuk mencoba lagi");
        }
        Err(e) => {
            eprintln!("[dev] build pertama gagal: {e}");
        }
    }

    let mut last_state = snapshot();
    loop {
        sleep(Duration::from_millis(POLL_MS));
        let state = snapshot();
        if state == last_state {
            continue;
        }
        last_state = state;
        println!("\n[dev] perubahan terdeteksi — rebuild + restart");

        // Hentikan binary lama sebelum build ulang (window harus ditutup
        // dulu agar port/file window tidak terkunci).
        if let Some(child) = running.take() {
            let _ = kill(child);
        }

        match rebuild(&mode, target.as_deref()) {
            Ok(Some(binary)) => running = spawn(&binary),
            Ok(None) => {
                eprintln!("[dev] build gagal — menunggu perubahan berikutnya");
            }
            Err(e) => {
                eprintln!("[dev] build gagal: {e}");
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
                eprintln!("  {d}");
            }
            if report.has_errors {
                return Ok(None);
            }
            Ok(report.binary)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Jalankan binary; `None` bila gagal spawn.
fn spawn(binary: &Path) -> Option<Child> {
    println!("[dev] menjalankan {} ...", binary.display());
    match Command::new(binary).spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            eprintln!("[dev] gagal menjalankan binary: {e}");
            None
        }
    }
}

/// Hentikan child dengan grace (SIGKILL via drop) — `child.kill()` cukup di
/// Unix; di Windows `taskkill` lebih andal untuk menutup window tree.
fn kill(mut child: Child) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        if let Some(pid) = child.id().into() {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}
