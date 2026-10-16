//! Perintah `audit` — periksa kerentanan/cela keamanan pada library yang dipakai.
//!
//! Sumber data:
//! 1. `Cargo.lock` (inventaris persis dependency yang terkunci).
//! 2. Basis data advisory lokal (`ci/advisories.toml`, dapat diganti `--db`).
//! 3. Pemeriksaan struktural offline: sumber non-registry, versi ganda.
//! 4. Opsional `--live`: jalankan `cargo audit`/`cargo deny` (basis data CVE
//!    langsung) bila terpasang — tanpa ini, CVE terbaru **tidak** diperiksa dan
//!    itu dilaporkan secara jujur.
//!
//! Keluar non-0 bila ada temuan pada/di atas ambang `--fail-on` (default
//! `high`) atau bila pemeriksaan tidak dapat dijalankan.

use std::path::{Path, PathBuf};
use std::process::Command;

use gnative_compiler::advisory::{AdvisoryDb, Severity};
use gnative_compiler::lockfile::Lockfile;

use crate::log;

/// Ambang gagal default.
const DEFAULT_FAIL_ON: &str = "high";

/// Kandidat pemeriksa CVE langsung (`<perintah> <subperintah>`).
const LIVE_CHECKERS: &[(&str, &[&str])] = &[
    ("cargo", &["audit"]),
    ("cargo", &["deny", "check", "advisories"]),
];

pub fn run(args: &[String]) -> i32 {
    let db_path = match arg_value(args, "--db") {
        Ok(p) => p,
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    let fail_on = match arg_value(args, "--fail-on") {
        Ok(Some(s)) => Severity::parse(&s),
        Ok(None) => Severity::parse(DEFAULT_FAIL_ON),
        Err(e) => {
            log::error(e);
            return 1;
        }
    };
    let live = args.iter().any(|a| a == "--live");
    let deny_warnings = args.iter().any(|a| a == "--deny-warnings");

    println!("gnative audit — kerentanan library yang dipakai");
    println!("==============================================");

    // 1. Inventaris dependency.
    let Some(lock_path) = Lockfile::find(Path::new(".")) else {
        log::error(
            "Cargo.lock tidak ditemukan — dependency tidak terkunci (tidak reproducible). \
             Jalankan `cargo generate-lockfile` dan commit hasilnya.",
        );
        return 1;
    };
    let lock = match Lockfile::load(&lock_path) {
        Ok(l) => l,
        Err(e) => {
            log::error(format!("{e}"));
            return 1;
        }
    };
    let non_registry = lock.non_registry();
    let duplicates = lock.duplicates();
    let workspace_members = lock
        .packages
        .iter()
        .filter(|p| p.is_workspace_member())
        .count();
    let crates_io = lock.packages.iter().filter(|p| p.is_crates_io()).count();

    println!("lock        : {}", lock_path.display());
    println!(
        "paket       : {} total ({} crates.io, {} anggota workspace, {} non-registry)",
        lock.packages.len(),
        crates_io,
        workspace_members,
        non_registry.len()
    );

    let mut findings_high = 0usize;
    let mut findings_total = 0usize;

    // 2. Basis data advisory lokal.
    let db = match resolve_db(db_path.as_deref(), &lock_path) {
        Ok(Some(path)) => match AdvisoryDb::load(&path) {
            Ok(db) => {
                println!("basis data  : {} ({} entri)", path.display(), db.len());
                if db.is_empty() {
                    println!(
                        "              catatan: basis data kosong — hanya pemeriksaan struktural \
                         yang berjalan"
                    );
                }
                Some(db)
            }
            Err(e) => {
                log::error(format!("{e}"));
                return 1;
            }
        },
        Ok(None) => {
            println!("basis data  : (tidak ada `ci/advisories.toml`)");
            None
        }
        Err(e) => {
            log::error(e);
            return 1;
        }
    };

    println!();
    println!("--- temuan advisory ---");
    if let Some(db) = &db {
        let findings = db.check(&lock.packages);
        findings_total += findings.len();
        if findings.is_empty() {
            println!("  tidak ada kecocokan ({} advisory diperiksa)", db.len());
        } else {
            for f in &findings {
                if f.severity().rank() >= fail_on.rank() {
                    findings_high += 1;
                }
                println!(
                    "  [{}] {} — {} {}",
                    f.severity().label(),
                    f.advisory.id,
                    f.advisory.package,
                    f.version
                );
                println!("      {}", f.advisory.summary);
                if let Some(p) = &f.advisory.patched {
                    println!("      diperbaiki pada: {p}");
                }
                if let Some(u) = &f.advisory.url {
                    println!("      referensi: {u}");
                }
            }
        }
    } else {
        println!("  dilewati (tidak ada basis data)");
    }

    // 3. Pemeriksaan struktural (selalu jalan, tanpa basis data).
    println!();
    println!("--- pemeriksaan struktural ---");
    if non_registry.is_empty() {
        println!("  sumber non-registry: tidak ada (semua dari crates.io)");
    } else {
        findings_total += non_registry.len();
        for p in &non_registry {
            println!(
                "  [sedang] sumber non-registry: {} {} — {}",
                p.name,
                p.version,
                p.source.as_deref().unwrap_or("(tanpa source)")
            );
            println!(
                "      dependency git/registry alternatif tidak tercakup pemeriksaan crates.io"
            );
        }
        // Sumber non-registry tidak otomatis "gagal tinggi": hanya jika
        // ambang diminta lebih rendah dari `sedang`.
        if fail_on.rank() <= Severity::Medium.rank() {
            findings_high += non_registry.len();
        }
    }

    if duplicates.is_empty() {
        println!("  versi ganda: tidak ada");
    } else {
        println!("  versi ganda ({} crate):", duplicates.len());
        for (name, versions) in &duplicates {
            println!("    - {name}: {}", versions.join(", "));
        }
        println!("      catatan: versi ganda memperbesar permukaan serangan + ukuran binary");
    }

    // 4. Pemeriksaan langsung (basis data CVE) — jujur bila tak tersedia.
    println!();
    println!("--- pemeriksaan langsung (basis data CVE) ---");
    let checker = LIVE_CHECKERS.iter().find(|(cmd, sub)| {
        Command::new(cmd)
            .args(*sub)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    match (live, checker) {
        (false, Some((cmd, sub))) => {
            println!(
                "  tersedia: `{cmd} {}` — jalankan `gnative audit --live` untuk basis data CVE terbaru",
                sub.join(" ")
            );
        }
        (false, None) => {
            println!(
                "  TIDAK DIPERIKSA: tidak ada `cargo audit`/`cargo deny` di PATH. \
                 Pasang (mis. `cargo install cargo-audit`) lalu jalankan `gnative audit --live`."
            );
        }
        (true, None) => {
            log::error(
                "`--live` diminta tetapi tidak ada `cargo audit`/`cargo deny` di PATH. \
                 Pasang: `cargo install cargo-audit`",
            );
            return 1;
        }
        (true, Some((cmd, sub))) => {
            // `--deny-warnings` menjadikan peringatan (mis. crate
            // "unmaintained") ikut menggagalkan gerbang — `cargo audit` default
            // hanya menggagalkan pada vulnerability.
            let mut argv: Vec<String> = sub.iter().map(|s| s.to_string()).collect();
            if deny_warnings && *cmd == "cargo" && sub == &["audit"] {
                argv.push("--deny".into());
                argv.push("warnings".into());
            }
            let output = Command::new(cmd)
                .args(&argv)
                .current_dir(lock_path.parent().unwrap_or(Path::new(".")))
                .output();
            match output {
                Ok(out) => {
                    // Tampilkan verbatim apa pun yang dilaporkan pemeriksa.
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    if !stdout.trim().is_empty() {
                        println!("{stdout}");
                    }
                    if !stderr.trim().is_empty() {
                        eprintln!("{stderr}");
                    }
                    let combined = format!("{stdout}{stderr}");
                    let has_warning = combined.contains("warning");
                    if out.status.success() {
                        if has_warning {
                            println!(
                                "  `{cmd} {}` → tidak ada vulnerability, TETAPI ada peringatan \
                                 (mis. crate unmaintained) — lihat keluaran di atas. \
                                 Gunakan `--deny-warnings` untuk menggagalkannya.",
                                argv.join(" ")
                            );
                        } else {
                            println!("  `{cmd} {}` → bersih", argv.join(" "));
                        }
                    } else {
                        println!(
                            "  `{cmd} {}` → melaporkan temuan (exit {})",
                            argv.join(" "),
                            out.status.code().unwrap_or(-1)
                        );
                        findings_total += 1;
                        findings_high += 1;
                    }
                }
                Err(e) => {
                    log::error(format!("menjalankan `{cmd}` gagal: {e}"));
                    return 1;
                }
            }
        }
    }

    // 5. Ringkasan + gerbang.
    println!();
    println!("--- ringkasan ---");
    println!(
        "  temuan total: {findings_total} (pada/di atas ambang `{}`: {findings_high})",
        fail_on.label()
    );
    if findings_high > 0 {
        log::error(format!(
            "audit GAGAL: {findings_high} temuan pada/di atas ambang `{}`",
            fail_on.label()
        ));
        return 1;
    }
    if !live {
        println!(
            "  catatan jujur: tanpa `--live`, kerentanan yang baru terbit (zero-day) \
             TIDAK tercakup. Basis data lokal hanya memuat entri yang diisi maintainer."
        );
    }
    println!("audit selesai: tidak ada temuan pada/di atas ambang");
    0
}

/// Ambil nilai flag `--nama <nilai>`; `Ok(None)` bila flag tak ada.
///
/// `Err` bila flag ada tetapi nilainya kosong/hilang (jangan diam-diam
/// memakai default — konsisten dengan `--target`, audit P3-02).
fn arg_value(args: &[String], flag: &str) -> Result<Option<String>, String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == flag {
            return match it.next() {
                Some(v) if !v.is_empty() && !v.starts_with('-') => Ok(Some(v.clone())),
                _ => Err(format!(
                    "`{flag}` butuh nilai (mis. `{flag} nilai`) — nilai kosong/hilang"
                )),
            };
        }
    }
    Ok(None)
}

/// Tentukan berkas basis data: `--db`, atau `ci/advisories.toml` di sekitar
/// `Cargo.lock` (root repo).
fn resolve_db(explicit: Option<&str>, lock_path: &Path) -> Result<Option<PathBuf>, String> {
    if let Some(p) = explicit {
        let path = PathBuf::from(p);
        if !path.is_file() {
            return Err(format!(
                "berkas basis data `{}` tidak ditemukan",
                path.display()
            ));
        }
        return Ok(Some(path));
    }
    let mut dir = lock_path.parent();
    while let Some(current) = dir {
        let candidate = current.join("ci").join("advisories.toml");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        dir = current.parent();
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_value_mengambil_nilai() {
        let args = vec!["--db".to_string(), "a.toml".to_string()];
        assert_eq!(arg_value(&args, "--db").unwrap().as_deref(), Some("a.toml"));
        assert_eq!(arg_value(&args, "--fail-on").unwrap(), None);
    }

    #[test]
    fn arg_value_kosong_ditolak() {
        let args = vec!["--db".to_string()];
        assert!(arg_value(&args, "--db").is_err());
        let args = vec!["--db".to_string(), "--live".to_string()];
        assert!(arg_value(&args, "--db").is_err());
    }

    #[test]
    fn resolve_db_tidak_ada_ok() {
        // Lock di /tmp tidak punya ci/advisories.toml.
        let r = resolve_db(None, Path::new("/tmp/gnative-tidak-ada/Cargo.lock")).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn resolve_db_eksplisit_hilang_error() {
        assert!(resolve_db(Some("/tidak/ada.toml"), Path::new("Cargo.lock")).is_err());
    }
}
