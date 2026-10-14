//! Perintah `doctor` — cek environment build native.

use std::path::Path;
use std::process::Command;

pub fn run(_args: &[String]) -> i32 {
    let mut ok = true;

    println!("── Gnative doctor ──");

    // Toolchain Rust (untuk membangun CLI Gnative itu sendiri).
    match Command::new("rustc").arg("--version").output() {
        Ok(out) => {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("  rustc:   {v}");
            if v.is_empty() {
                ok = false;
            }
        }
        Err(_) => {
            println!("  rustc:   TIDAK DITEMUKAN");
            ok = false;
        }
    }

    // Linker sistem — wajib untuk tahap link object → binary native.
    let mut linker_found = false;
    for candidate in ["cc", "clang", "gcc"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            println!("  linker:  {candidate} (tersedia)");
            linker_found = true;
            break;
        }
    }
    if !linker_found {
        println!("  linker:  TIDAK DITEMUKAN (butuh cc/clang/gcc untuk link binary)");
        ok = false;
    }

    // Libruntime — staticlib yang di-link bersama object .gn menjadi binary.
    let runtime_candidates = [
        "target/debug/libgnative_runtime.a",
        "target/release/libgnative_runtime.a",
    ];
    let runtime_found = runtime_candidates.iter().any(|p| Path::new(p).exists());
    if runtime_found {
        println!("  libruntime: ditemukan (staticlib wgpu/cuda)");
    } else {
        println!(
            "  libruntime: TIDAK DITEMUKAN — jalankan `cargo build -p gnative-runtime` \
             untuk bisa me-link binary native"
        );
    }

    // Backend codegen.
    println!("  codegen: Cranelift (dev) + LLVM 18 (release)");

    println!("── selesai ──");
    if ok {
        0
    } else {
        1
    }
}
