//! Basis data advisory lokal + pencocokan ke paket `Cargo.lock`.
//!
//! Tanggung jawab tunggal: memuat basis data advisory (TOML), mencocokkan
//! paket terhadap rentang versi yang terdampak, dan melaporkan temuan.
//!
//! **Sifat**: offline dan deterministik — tidak ada panggilan jaringan, tidak
//! ada dependency baru. Basis data default ada di `ci/advisories.toml` dan
//! diisi oleh maintainer (mis. transkripsi entri RUSTSEC). Untuk pemeriksaan
//! terhadap basis data CVE **langsung**, jalankan `gnative audit --live`
//! (memakai `cargo audit`/`cargo deny` bila terpasang).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::Path;

use gnative_core::{GnError, GnResult};

use crate::lockfile::LockPackage;

/// Tingkat keparahan temuan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Informasi (tidak memengaruhi keamanan).
    Info,
    /// Rendah.
    Low,
    /// Sedang.
    Medium,
    /// Tinggi.
    High,
    /// Kritis.
    Critical,
    /// Tidak dinyatakan di basis data — diperlakukan **konservatif** setara
    /// `High` agar tidak lolos gerbang `--fail-on` secara tak sengaja.
    Unknown,
}

impl Severity {
    /// Parse dari string (case-insensitive). Tak dikenal → [`Severity::Unknown`].
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "info" | "informational" => Severity::Info,
            "low" => Severity::Low,
            "medium" | "moderate" => Severity::Medium,
            "high" => Severity::High,
            "critical" => Severity::Critical,
            _ => Severity::Unknown,
        }
    }

    /// Peringkat untuk perbandingan gerbang (`Unknown` = setara `High`).
    pub fn rank(self) -> u8 {
        match self {
            Severity::Info => 0,
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::Unknown | Severity::High => 3,
            Severity::Critical => 4,
        }
    }

    /// Label untuk output.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Low => "rendah",
            Severity::Medium => "sedang",
            Severity::High => "TINGGI",
            Severity::Critical => "KRITIS",
            Severity::Unknown => "TIDAK DIKETAHUI",
        }
    }
}

/// Satu entri advisory.
#[derive(Debug, Clone)]
pub struct Advisory {
    /// Identitas advisory (mis. `RUSTSEC-2024-0001` atau ID internal).
    pub id: String,
    /// Nama crate terdampak.
    pub package: String,
    /// Rentang versi terdampak (mis. `["<1.2.3", ">=2.0.0, <2.0.4"]`).
    pub affected: Vec<String>,
    /// Keparahan.
    pub severity: Severity,
    /// Ringkasan singkat.
    pub summary: String,
    /// Tautan referensi (opsional).
    pub url: Option<String>,
    /// Versi yang sudah diperbaiki (opsional, untuk saran).
    pub patched: Option<String>,
}

impl Advisory {
    /// True bila `version` termasuk rentang terdampak.
    pub fn affects(&self, version: &str) -> bool {
        self.affected.iter().any(|req| satisfies(req, version))
    }
}

/// Temuan: advisory × paket terkunci.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Advisory yang cocok.
    pub advisory: Advisory,
    /// Versi paket yang terkunci.
    pub version: String,
}

impl Finding {
    /// Tingkat keparahan temuan.
    pub fn severity(&self) -> Severity {
        self.advisory.severity
    }
}

/// Basis data advisory.
#[derive(Debug, Clone, Default)]
pub struct AdvisoryDb {
    /// Semua advisory.
    pub advisories: Vec<Advisory>,
}

impl AdvisoryDb {
    /// Muat dari berkas TOML.
    pub fn load(path: &Path) -> GnResult<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            GnError::Config(format!(
                "tidak bisa membaca basis data advisory {}: {e}",
                path.display()
            ))
        })?;
        Self::parse(&text)
    }

    /// Parse TOML berisi `[[advisory]]`.
    pub fn parse(text: &str) -> GnResult<Self> {
        let value: toml::Value = toml::from_str(text)
            .map_err(|e| GnError::Config(format!("basis data advisory tidak valid (TOML): {e}")))?;
        let mut advisories = Vec::new();
        let entries = value
            .get("advisory")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for entry in entries {
            let id = entry.get("id").and_then(|v| v.as_str());
            let package = entry.get("package").and_then(|v| v.as_str());
            let (Some(id), Some(package)) = (id, package) else {
                return Err(GnError::Config(
                    "advisory: setiap `[[advisory]]` wajib punya `id` dan `package`".into(),
                ));
            };
            let affected: Vec<String> = entry
                .get("affected")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            if affected.is_empty() {
                return Err(GnError::Config(format!(
                    "advisory `{id}`: `affected` wajib berisi minimal satu rentang versi"
                )));
            }
            advisories.push(Advisory {
                id: id.to_string(),
                package: package.to_string(),
                affected,
                severity: entry
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .map(Severity::parse)
                    .unwrap_or(Severity::Unknown),
                summary: entry
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(tanpa ringkasan)")
                    .to_string(),
                url: entry
                    .get("url")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                patched: entry
                    .get("patched")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
            });
        }
        Ok(AdvisoryDb { advisories })
    }

    /// Jumlah advisory dalam basis data.
    pub fn len(&self) -> usize {
        self.advisories.len()
    }

    /// True bila basis data kosong.
    pub fn is_empty(&self) -> bool {
        self.advisories.is_empty()
    }

    /// Cocokkan paket terkunci → daftar temuan (urut keparahan menurun).
    pub fn check(&self, packages: &[LockPackage]) -> Vec<Finding> {
        let mut findings = Vec::new();
        for advisory in &self.advisories {
            for pkg in packages.iter().filter(|p| p.name == advisory.package) {
                if advisory.affects(&pkg.version) {
                    findings.push(Finding {
                        advisory: advisory.clone(),
                        version: pkg.version.clone(),
                    });
                }
            }
        }
        findings.sort_by(|a, b| {
            b.severity()
                .rank()
                .cmp(&a.severity().rank())
                .then_with(|| a.advisory.id.cmp(&b.advisory.id))
        });
        findings
    }
}

// ---------------------------------------------------------------------------
// Perbandingan versi (semver terbatas) — tanpa dependency.
// ---------------------------------------------------------------------------

/// Parse versi menjadi (segmen numerik, prerelease).
fn parse_version(v: &str) -> (Vec<u64>, Option<String>) {
    let v = v.trim();
    // Buang metadata build (`+...`) dan prefix `v`.
    let v = v.split('+').next().unwrap_or(v);
    let v = v.strip_prefix('v').unwrap_or(v);
    match v.split_once('-') {
        Some((core, pre)) => (numeric_segments(core), Some(pre.to_string())),
        None => (numeric_segments(v), None),
    }
}

/// Segmen numerik; komponen non-numerik diabaikan (jadi 0).
fn numeric_segments(core: &str) -> Vec<u64> {
    core.split('.')
        .map(|s| s.trim().parse::<u64>().unwrap_or(0))
        .collect()
}

/// Bandingkan versi ala semver: prerelease < release.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (na, pa) = parse_version(a);
    let (nb, pb) = parse_version(b);
    let len = na.len().max(nb.len());
    for i in 0..len {
        let x = na.get(i).copied().unwrap_or(0);
        let y = nb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match (pa, pb) {
        (None, None) => Ordering::Equal,
        // Versi tanpa prerelease lebih tinggi.
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => compare_prerelease(&x, &y),
    }
}

/// Bandingkan identifier prerelease (`rc.1` vs `beta`).
fn compare_prerelease(a: &str, b: &str) -> Ordering {
    let sa: Vec<&str> = a.split('.').collect();
    let sb: Vec<&str> = b.split('.').collect();
    let len = sa.len().max(sb.len());
    for i in 0..len {
        match (sa.get(i), sb.get(i)) {
            (Some(x), Some(y)) => {
                let nx = x.parse::<u64>();
                let ny = y.parse::<u64>();
                let ord = match (nx, ny) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    // Numerik < alfanumerik (semver).
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => break,
        }
    }
    Ordering::Equal
}

/// True bila `version` memenuhi `req`.
///
/// Sintaks `req`: satu atau beberapa constraint dipisah koma (AND), masing-masing
/// `=x.y.z`, `<x.y.z`, `<=x.y.z`, `>x.y.z`, `>=x.y.z`, atau `x.y.z` (setara `=`).
/// `*` cocok dengan apa pun.
pub fn satisfies(req: &str, version: &str) -> bool {
    let req = req.trim();
    if req.is_empty() || req == "*" {
        return true;
    }
    req.split(',').all(|part| {
        let part = part.trim();
        if part.is_empty() {
            return true;
        }
        let (op, rest) = if let Some(r) = part.strip_prefix(">=") {
            (">=", r)
        } else if let Some(r) = part.strip_prefix("<=") {
            ("<=", r)
        } else if let Some(r) = part.strip_prefix('>') {
            (">", r)
        } else if let Some(r) = part.strip_prefix('<') {
            ("<", r)
        } else if let Some(r) = part.strip_prefix('=') {
            ("=", r)
        } else {
            ("=", part)
        };
        let rest = rest.trim();
        if rest == "*" {
            return true;
        }
        let ord = compare_versions(version, rest);
        match op {
            ">=" => ord != Ordering::Less,
            "<=" => ord != Ordering::Greater,
            ">" => ord == Ordering::Greater,
            "<" => ord == Ordering::Less,
            // `=` juga menerima versi dalam segmen yang sama bila ditulis
            // sebagai prefix (mis. `=1.2` cocok dengan `1.2.9`).
            _ => ord == Ordering::Equal || is_prefix_of(rest, version),
        }
    })
}

/// True bila `prefix` adalah awalan segmen dari `version` (mis. `1.2` vs `1.2.9`).
fn is_prefix_of(prefix: &str, version: &str) -> bool {
    let (pv, _) = parse_version(prefix);
    let (vv, _) = parse_version(version);
    if pv.len() >= vv.len() {
        return false;
    }
    pv.iter().zip(vv.iter()).all(|(a, b)| a == b)
}

/// Kelompokkan temuan menurut advisory (id → temuan).
pub fn group_by_advisory(findings: &[Finding]) -> BTreeMap<String, Vec<Finding>> {
    let mut map: BTreeMap<String, Vec<Finding>> = BTreeMap::new();
    for f in findings {
        map.entry(f.advisory.id.clone())
            .or_default()
            .push(f.clone());
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> LockPackage {
        LockPackage {
            name: name.to_string(),
            version: version.to_string(),
            source: None,
        }
    }

    #[test]
    fn bandingkan_versi_dasar() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("1.2.3", "1.2.10"), Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
    }

    #[test]
    fn prerelease_lebih_kecil_dari_release() {
        assert_eq!(compare_versions("1.0.0-rc.1", "1.0.0"), Ordering::Less);
        assert_eq!(compare_versions("1.0.0", "1.0.0-rc.1"), Ordering::Greater);
        assert_eq!(compare_versions("1.0.0-rc.1", "1.0.0-rc.2"), Ordering::Less);
        assert_eq!(
            compare_versions("1.0.0-rc.1", "1.0.0-rc.1"),
            Ordering::Equal
        );
    }

    #[test]
    fn rentang_versi_dicocokkan() {
        assert!(satisfies("<1.2.3", "1.2.2"));
        assert!(!satisfies("<1.2.3", "1.2.3"));
        assert!(satisfies("<=1.2.3", "1.2.3"));
        assert!(satisfies(">=2.0.0, <2.0.4", "2.0.1"));
        assert!(!satisfies(">=2.0.0, <2.0.4", "2.0.4"));
        assert!(satisfies("=1.2.3", "1.2.3"));
        assert!(satisfies("1.2.3", "1.2.3"));
        assert!(satisfies("*", "9.9.9"));
        // Prefix: `=1.2` cocok dengan 1.2.9 (segmen sama).
        assert!(satisfies("=1.2", "1.2.9"));
        assert!(!satisfies("=1.2", "1.3.0"));
    }

    #[test]
    fn severity_unknown_konservatif() {
        assert_eq!(Severity::parse("kritis"), Severity::Unknown);
        assert_eq!(Severity::parse("Critical"), Severity::Critical);
        assert_eq!(Severity::parse("HIGH").rank(), Severity::Unknown.rank());
    }

    #[test]
    fn db_parse_dan_check() {
        let db = AdvisoryDb::parse(
            r#"
[[advisory]]
id = "RUSTSEC-CONTOH-0001"
package = "crate-contoh"
affected = ["<1.2.3"]
severity = "high"
summary = "contoh"
patched = "1.2.3"
"#,
        )
        .unwrap();
        assert_eq!(db.len(), 1);
        let findings = db.check(&[pkg("crate-contoh", "1.2.0"), pkg("lain", "0.1.0")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].version, "1.2.0");
        assert_eq!(findings[0].severity(), Severity::High);
        // Versi yang sudah dipatch tidak dilaporkan.
        assert!(db.check(&[pkg("crate-contoh", "1.2.3")]).is_empty());
    }

    #[test]
    fn db_entri_tanpa_affected_ditolak() {
        let err = AdvisoryDb::parse("[[advisory]]\nid = \"x\"\npackage = \"y\"\n");
        assert!(err.is_err(), "affected wajib ada");
    }

    #[test]
    fn db_kosong_ok() {
        let db = AdvisoryDb::parse("# hanya komentar\n").unwrap();
        assert!(db.is_empty());
        assert!(db.check(&[pkg("apa", "1.0.0")]).is_empty());
    }

    #[test]
    fn urut_keparahan_menurun() {
        let db = AdvisoryDb::parse(
            r#"
[[advisory]]
id = "A-RENDAH"
package = "c"
affected = ["*"]
severity = "low"
summary = "l"

[[advisory]]
id = "A-KRITIS"
package = "c"
affected = ["*"]
severity = "critical"
summary = "c"
"#,
        )
        .unwrap();
        let f = db.check(&[pkg("c", "1.0.0")]);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].severity(), Severity::Critical);
    }
}
