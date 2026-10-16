//! Linker — object file `.gn` → binary native.
//!
//! Menggantikan peran Cargo/rustc pada pipeline lama. Binary hasil compiler
//! memanggil intrinsik C-ABI (`gn_window_create`, `gn_window_run`) dari
//! **libruntime** (`libgnative_runtime.a`, dibangun dari `crates/gnative-runtime`
//! yang berisi wgpu + CUDA fallback).
//!
//! Link: `cc app.o libruntime.a -o app` (driver linker sistem menyediakan
//! CRT + libc; libruntime menyediakan window native + render).

use std::path::{Path, PathBuf};
use std::process::Command;

use gnative_core::{GnError, GnResult};

/// Nama driver linker yang dicoba, berurutan.
const LINKERS: &[&str] = &["cc", "clang", "gcc"];

/// Nama berkas libruntime (staticlib Rust).
const RUNTIME_LIB: &str = "libgnative_runtime.a";

/// Link `object` + libruntime menjadi executable `out`.
///
/// `runtime_dir` = direktori berisi `libgnative_runtime.a` (dibangun dari
/// repo Gnative: `cargo build -p gnative-runtime`).
pub fn link(object: &Path, runtime_dir: &Path, out: &Path) -> GnResult<()> {
    link_with_target(object, runtime_dir, out, None)
}

/// Link dengan target spesifik; `target` = triple (mis.
/// `aarch64-unknown-linux-gnu`), `None` = host. Linker target dicari
/// sebagai `<triple>-cc`/`<triple>-clang`/`<triple>-gcc` (cross-toolchain).
pub fn link_with_target(
    object: &Path,
    runtime_dir: &Path,
    out: &Path,
    target: Option<&str>,
) -> GnResult<()> {
    let runtime = runtime_dir.join(RUNTIME_LIB);
    if !runtime.exists() {
        return Err(GnError::BackendUnavailable(format!(
            "libruntime tidak ditemukan di {} — bangun dulu: \
             `cargo build -p gnative-runtime` (berisi wgpu + CUDA fallback)",
            runtime.display()
        )));
    }

    let linker = find_linker_for(target).ok_or_else(|| {
        GnError::BackendUnavailable(match target {
            Some(t) => format!(
                "tidak ada linker target untuk `{t}` (`{t}-cc`/`{t}-clang`/`{t}-gcc`) \
                 di PATH — pasang cross-toolchain (mis. mingw-w64 / cross-gcc)"
            ),
            None => "tidak ada linker sistem (`cc`/`clang`/`gcc`) di PATH — \
                 wajib untuk menghasilkan binary native"
                .into(),
        })
    })?;

    // Rust staticlib butuh libs sistem tambahan saat di-link oleh driver C.
    let status = Command::new(&linker)
        .arg(object)
        .arg(&runtime)
        .args(["-lpthread", "-ldl", "-lm"])
        .arg("-o")
        .arg(out)
        .status()
        .map_err(|e| GnError::Other(format!("menjalankan linker `{linker}` gagal: {e}")))?;

    if !status.success() {
        return Err(GnError::Other(format!(
            "linker `{linker}` gagal (exit code {:?})",
            status.code()
        )));
    }
    Ok(())
}

/// Cari driver linker pertama yang tersedia.
pub fn find_linker() -> Option<&'static str> {
    LINKERS.iter().copied().find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Cari linker untuk target.
///
/// - `None` (host): `cc`/`clang`/`gcc` host.
/// - `Some(triple)`: **hanya** `<triple>-cc`/`-clang`/`-gcc`. Tidak ada
///   fallback ke host linker — memakai `cc` host untuk target lain
///   menghasilkan link yang menyesatkan (audit P1-13). Bila tak ada
///   cross-toolchain → `None` → pemanggil melaporkan error jelas.
pub fn find_linker_for(target: Option<&str>) -> Option<String> {
    let candidates: Vec<String> = match target {
        Some(t) => LINKERS.iter().map(|base| format!("{t}-{base}")).collect(),
        None => LINKERS.iter().map(|s| s.to_string()).collect(),
    };

    candidates.into_iter().find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Cari libruntime dari lokasi umum (target/ Cargo), naik ke root repo.
///
/// Libruntime dibangun dari repo Gnative (`cargo build -p gnative-runtime`),
/// jadi `target/debug` bisa berada di project `.gn` maupun di direktori
/// induknya (root repo). Untuk cross-compile, cari `target/<triple>/…`
/// (audit P1-13) — jangan pakai libruntime host untuk target lain.
///
/// `prefer_release` = true (build `--release`) mencari `release/` lebih dulu:
/// memakai libruntime debug untuk binary release menghasilkan artefak jauh
/// lebih besar dan tidak konsisten (audit Tahap 4).
pub fn find_runtime_dir(
    project_dir: &Path,
    target: Option<&str>,
    prefer_release: bool,
) -> Option<PathBuf> {
    // Pakai path absolut agar walk ke parent sampai root repo benar.
    let root = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let profiles: [&str; 2] = if prefer_release {
        ["release", "debug"]
    } else {
        ["debug", "release"]
    };
    let subs: Vec<String> = match target {
        Some(t) => profiles.iter().map(|p| format!("target/{t}/{p}")).collect(),
        None => profiles.iter().map(|p| format!("target/{p}")).collect(),
    };
    let mut dir = Some(root.as_path());
    while let Some(current) = dir {
        for sub in &subs {
            let candidate = current.join(sub);
            if candidate.join(RUNTIME_LIB).exists() {
                return Some(candidate);
            }
        }
        dir = current.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linker_detected() {
        // Lingkungan build harus punya setidaknya satu linker.
        assert!(find_linker().is_some(), "tidak ada cc/clang/gcc");
    }

    #[test]
    fn cross_tanpa_toolchain_tidak_fallback_host() {
        // P1-13: target tanpa cross-toolchain → None (BUKAN host cc).
        assert!(
            find_linker_for(Some("definitely-not-a-real-triple-xyz")).is_none(),
            "target asing tak boleh fallback ke linker host"
        );
        // Host tetap menemukan linker.
        assert!(find_linker_for(None).is_some());
    }
}
