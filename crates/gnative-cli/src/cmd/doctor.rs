//! Perintah `doctor` — cek environment build native secara jujur (P2-14).
//!
//! Melaporkan versi Gnative, target triple host, toolchain (linker seragam
//! dengan `linker.rs`), LLVM, CUDA, dan libruntime. Kegagalan keras
//! (tanpa linker / libruntime) → exit code non-0.

use std::path::Path;
use std::process::Command;

use crate::log;

/// Cek `rustc -vV` → (versi, host triple).
fn rustc_info() -> Option<(String, String)> {
    let out = Command::new("rustc").arg("-vV").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text
        .lines()
        .find(|l| l.starts_with("release:"))
        .map(|l| l.trim_start_matches("release:").trim().to_string())
        .unwrap_or_default();
    let host = text
        .lines()
        .find(|l| l.starts_with("host:"))
        .map(|l| l.trim_start_matches("host:").trim().to_string())
        .unwrap_or_default();
    Some((version, host))
}

/// Versi toolchain via `cmd --version` (baris pertama).
fn tool_version(cmd: &str, arg: &str) -> Option<String> {
    let out = Command::new(cmd).arg(arg).output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|s| s.trim().to_string())
}

pub fn run(_args: &[String]) -> i32 {
    let mut ok = true;

    println!("── Gnative doctor ──");
    println!("  gnative: v{}", env!("CARGO_PKG_VERSION"));

    // Toolchain Rust + host triple.
    match rustc_info() {
        Some((v, host)) => {
            println!("  rustc:   {v}");
            println!("  target:  {host} (host)");
        }
        None => {
            println!("  rustc:   TIDAK DITEMUKAN");
            ok = false;
        }
    }

    // Linker sistem — seragam dengan `linker::find_linker` (dipakai saat link).
    match gnative_compiler::linker::find_linker() {
        Some(l) => println!("  linker:  {l} (tersedia)"),
        None => {
            println!("  linker:  TIDAK DITEMUKAN (butuh cc/clang/gcc untuk link binary)");
            ok = false;
        }
    }

    // Libruntime — staticlib yang di-link bersama object .gn menjadi binary.
    let runtime_candidates = [
        "target/debug/libgnative_runtime.a",
        "target/release/libgnative_runtime.a",
    ];
    match runtime_candidates.iter().find(|p| Path::new(p).exists()) {
        Some(p) => println!("  libruntime: ditemukan ({p})"),
        None => {
            println!(
                "  libruntime: TIDAK DITEMUKAN — jalankan `cargo build -p gnative-runtime` \
                 untuk bisa me-link binary native"
            );
            ok = false;
        }
    }

    // LLVM 18 (backend release). Bukan kegagalan keras — hanya untuk `--release`.
    let llvm = tool_version("llvm-config", "--version")
        .or_else(|| tool_version("llvm-config-18", "--version"));
    match &llvm {
        Some(v) => println!("  llvm:    {v} (backend release)"),
        None => println!("  llvm:    tidak ditemukan (hanya perlu untuk `build --release`)"),
    }

    // CUDA (backend fallback, fitur `cuda`). Opsional.
    match tool_version("nvcc", "--version") {
        Some(v) => println!("  cuda:    {v} (backend fallback)"),
        None => println!("  cuda:    tidak ditemukan (opsional; fitur `cuda`)"),
    }

    println!("  codegen: Cranelift (dev) + LLVM 18 (release)");

    if log::is_verbose() {
        println!("── detail (verbose) ──");
        println!("  PATH linker: cc/clang/gcc, cross: <triple>-cc/clang/gcc");
        println!(
            "  env GNATIVE_REQUIRE_RUNTIME: {}",
            std::env::var("GNATIVE_REQUIRE_RUNTIME").unwrap_or_else(|_| "(tidak diset)".into())
        );
    }

    println!("── selesai ──");
    if ok {
        0
    } else {
        1
    }
}
