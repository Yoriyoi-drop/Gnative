//! Pipeline kompilasi native: `src/*.gn` → binary native.
//!
//! Tahapan (DESIGN.md §6):
//! ```text
//! src/*.gn (+ container/packages runtime .gn)
//!    │ lexer → parser → AST → checker (gnative-lang)
//!    ▼
//! Semantic IR (ir.rs + lower.rs)
//!    ▼
//! MIR (mir.rs + mir_lower.rs)
//!    ▼
//! codegen: Cranelift (dev) | LLVM (release) → object file
//!    ▼
//! linker (linker.rs) → binary native
//! ```

use std::path::{Path, PathBuf};

use gnative_core::{GnError, GnResult};
use gnative_lang::parse_source;

use crate::cache::Cache;
use crate::codegen::BuildMode;
use crate::fingerprint::{fnv1a_seeded, source_fingerprint};
use crate::ir::IrModule;
use crate::linker;
use crate::lower::lower;
use crate::mir_lower::lower_mir;
use crate::resolve::Resolver;

/// Lokasi runtime `.gn` bawaan compiler (diseed ke container/packages).
pub const RUNTIME_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/runtime");

/// Laporan hasil build native.
#[derive(Debug, Default)]
pub struct BuildReport {
    /// Path object file hasil codegen.
    pub object_file: Option<PathBuf>,
    /// Path binary native hasil link.
    pub binary: Option<PathBuf>,
    pub cache_hits: usize,
    pub regenerated: usize,
    pub diagnostics: Vec<String>,
    pub has_errors: bool,
}

/// Jalankan pipeline untuk sebuah project directory.
///
/// Layout: `<project>/gnative.toml`, `<project>/src/*.gn`,
/// runtime di `<project>/container/packages/@gnative/ui`,
/// output di `<project>/container/artifacts/`.
///
/// Target default = host (ISA native).
pub fn build(project_dir: &Path, mode: BuildMode) -> GnResult<BuildReport> {
    build_with_target(project_dir, mode, None)
}

/// Build untuk target tertentu; `target` = triple (mis.
/// `aarch64-unknown-linux-gnu`), `None` = host.
pub fn build_with_target(
    project_dir: &Path,
    mode: BuildMode,
    target: Option<&str>,
) -> GnResult<BuildReport> {
    let config = crate::config::Config::load(&project_dir.join("gnative.toml"))?;
    let container = project_dir.join("container");
    let artifacts = container.join("artifacts");
    std::fs::create_dir_all(&artifacts).map_err(GnError::Io)?;

    // Coba cache hit tingkat-project: jika semua input build tak berubah
    // (konten source + versi compiler + mode + target + runtime bawaan),
    // lewati parse/check/lower/codegen/link dan pakai artefak yang ada.
    let (build_key, file_count) = build_fingerprint(project_dir, mode, target)?;
    let cache = Cache::open(&container)?;
    let obj_name = sanitize_name(&config.project_name);
    let object_file = artifacts.join(format!("{obj_name}.o"));
    if cache.is_build_fresh(build_key) && object_file.exists() {
        let binary = artifacts.join(&obj_name);
        return Ok(BuildReport {
            object_file: Some(object_file.clone()),
            binary: if binary.exists() { Some(binary) } else { None },
            cache_hits: file_count,
            ..Default::default()
        });
    }

    let (mut parsed, mut cache) = collect_and_lower(project_dir, target)?;
    if parsed.report.has_errors {
        return Ok(parsed.report);
    }

    // Catat fingerprint setelah build sukses (cache hit berikutnya).
    for file in &parsed.gn_files {
        let rel = relative_to(file, &parsed.src_dir).unwrap_or_else(|| file.display().to_string());
        let source = read_gn_file(file)?;
        cache.record(&rel, target, source_fingerprint(&source))?;
    }

    // 4. IR → MIR.
    let mir = lower_mir(&parsed.ir);
    if mir.funcs.is_empty() {
        return Err(GnError::Config(
            "tidak ada `window` untuk dijadikan entry point".into(),
        ));
    }

    // 5. Codegen native → object file (target spesifik bila di-set).
    let obj_name = sanitize_name(&parsed.config.project_name);
    let object_file = parsed.artifacts.join(format!("{obj_name}.o"));
    crate::codegen::codegen(mode, target, &mir, &object_file)?;
    parsed.report.object_file = Some(object_file.clone());

    // 6. Link → binary native (object .gn + libruntime wgpu/cuda).
    //    Libruntime di-build dari repo Gnative (`cargo build -p gnative-runtime`).
    let binary = parsed.artifacts.join(&obj_name);
    let prefer_release = matches!(mode, BuildMode::Release);
    match linker::find_runtime_dir(project_dir, target, prefer_release) {
        Some(runtime_dir) => {
            match linker::link_with_target(&object_file, &runtime_dir, &binary, target) {
                Ok(()) => parsed.report.binary = Some(binary),
                Err(e) => {
                    parsed
                        .report
                        .diagnostics
                        .push(format!("link gagal (object tetap dibuat): {e}"));
                    parsed.report.has_errors = true;
                }
            }
        }
        None => {
            parsed.report.diagnostics.push(
                "libruntime belum dibangun — jalankan `cargo build -p gnative-runtime` \
                 (target/debug/libgnative_runtime.a), lalu build ulang"
                    .into(),
            );
        }
    }

    // Catat fingerprint build tingkat-project HANYA bila sukses (object ada
    // dan tak ada error). Build gagal tak di-cache agar build berikutnya
    // benar-benar mengulang, bukan melaporkan sukses palsu via cache hit.
    if !parsed.report.has_errors {
        cache.record_build(build_key)?;
    }

    Ok(parsed.report)
}

/// Hasil fase parse+check+lower (dipakai `build` dan `validate`).
struct ParsedProject {
    report: BuildReport,
    ir: IrModule,
    gn_files: Vec<PathBuf>,
    config: crate::config::Config,
    src_dir: PathBuf,
    artifacts: PathBuf,
}

/// Fase 1–3: seed runtime, kumpulkan file, parse + check + lower.
///
/// Mengembalikan IR + laporan (cache ikut dibuka; fingerprint di-record
/// hanya oleh `build`, bukan `validate`).
fn collect_and_lower(project_dir: &Path, target: Option<&str>) -> GnResult<(ParsedProject, Cache)> {
    let config = crate::config::Config::load(&project_dir.join("gnative.toml"))?;
    let src_dir = project_dir.join("src");
    let container = project_dir.join("container");
    let artifacts = container.join("artifacts");
    std::fs::create_dir_all(&artifacts).map_err(GnError::Io)?;

    // 1. Seed runtime .gn bawaan ke container/packages (jika belum ada).
    seed_runtime(&container)?;

    // 2. Kumpulkan semua file .gn: project + package runtime.
    let mut gn_files = collect_gn_files(&src_dir)?;
    let runtime_pkg = container.join("packages/@gnative/ui");
    for f in collect_gn_files(&runtime_pkg)? {
        if !gn_files.contains(&f) {
            gn_files.push(f);
        }
    }
    if gn_files.is_empty() {
        return Err(GnError::Config(format!(
            "tidak ada file .gn di {}",
            src_dir.display()
        )));
    }

    // 3. Parse + check setiap file (fingerprint untuk laporan incremental).
    let cache = Cache::open(&container)?;
    let mut report = BuildReport::default();
    // Peringatan config (mis. `[project]`/name hilang → default) — jangan
    // terapkan default diam-diam (audit P1-12).
    report.diagnostics.extend(config.warnings.iter().cloned());
    // P1-11: cek versi `[gnative].version` vs versi compiler. Major beda =
    // error (inkompatibel); beda minor/patch = peringatan.
    if !config.gnative_version.is_empty() {
        let declared = &config.gnative_version;
        let compiler = env!("CARGO_PKG_VERSION");
        let major = |v: &str| v.split('.').next().unwrap_or("").to_string();
        if major(declared) != major(compiler) {
            report.diagnostics.push(format!(
                "versi `gnative.toml` (`{declared}`) tidak kompatibel dengan compiler (`{compiler}`) — major berbeda"
            ));
            report.has_errors = true;
        } else if declared != compiler {
            report.diagnostics.push(format!(
                "peringatan: versi `gnative.toml` (`{declared}`) ≠ compiler (`{compiler}`)"
            ));
        }
    }
    let mut ir = IrModule::default();
    let mut dep_graph = crate::graph::DepGraph::new();

    for file in &gn_files {
        let rel = relative_to(file, &src_dir).unwrap_or_else(|| file.display().to_string());
        let source = read_gn_file(file)?;
        let fingerprint = source_fingerprint(&source);

        if cache.is_fresh(&rel, target, fingerprint) {
            report.cache_hits += 1;
        } else {
            report.regenerated += 1;
        }

        // Parse + check (ringan; wajib untuk membangun IR).
        let (module, diagnostics) = parse_source(&source);
        for d in &diagnostics {
            report
                .diagnostics
                .push(format!("{}: {}", file.display(), d));
            if d.is_error() {
                report.has_errors = true;
            }
        }

        // Resolusi `use` (package runtime tersedia setelah seed).
        let imports: Vec<String> = module.imports.iter().map(|i| i.path.clone()).collect();
        let resolution = Resolver::resolve(&imports, &container, &src_dir)?;
        // P1-11: package yang hilang = error build (bukan sekadar diagnostik).
        for missing in &resolution.missing {
            report.diagnostics.push(format!(
                "{}: package `{missing}` tidak ditemukan",
                file.display()
            ));
            report.has_errors = true;
        }
        // Graf dependency: file → target (rel path lokal bila ada, else nama paket).
        let mut edges: Vec<String> = Vec::new();
        for (imp, path) in &resolution.packages {
            match relative_to(path, &src_dir) {
                Some(rel_target) => edges.push(rel_target),
                None => edges.push(imp.clone()),
            }
        }
        dep_graph.add_node(&rel, &edges);

        // Lowering → Semantic IR (project dan runtime digabung).
        let out = lower(&module)?;
        // Semua diagnostic dari lower adalah error (fungsi/window gagal
        // di-lower) — tanpa ini pipeline lanjut ke codegen dan panic.
        if !out.diagnostics.is_empty() {
            report.has_errors = true;
        }
        // P1-10: sertakan file pada diagnostic lowering (sebelumnya tanpa
        // konteks file) — span deklarasi sudah ada di pesan lower.
        report.diagnostics.extend(
            out.diagnostics
                .iter()
                .map(|d| format!("{}: {}", file.display(), d)),
        );
        if let Some(window) = out.module.window {
            // P1-06/P1-11: multi-window tidak didukung — jangan timpa diam.
            if ir.window.is_some() {
                report.diagnostics.push(format!(
                    "{}: `window` kedua ditemukan — hanya satu window didukung",
                    file.display()
                ));
                report.has_errors = true;
            } else {
                ir.window = Some(window);
            }
        }
        ir.funcs.extend(out.module.funcs);
        ir.components.extend(out.module.components);
    }

    // P1-11: deteksi siklus dependency → gagalkan build.
    for cycle in dep_graph.detect_cycles() {
        report
            .diagnostics
            .push(format!("siklus dependency: {}", cycle.join(" → ")));
        report.has_errors = true;
    }

    Ok((
        ParsedProject {
            report,
            ir,
            gn_files,
            config,
            src_dir,
            artifacts,
        },
        cache,
    ))
}

/// Kumpulkan semua file `.gn` yang masuk pipeline (project + runtime bawaan),
/// diurutkan agar fingerprint deterministik lintas platform.
fn collect_all_gn(project_dir: &Path) -> GnResult<Vec<PathBuf>> {
    let src_dir = project_dir.join("src");
    let container = project_dir.join("container");
    let mut gn_files = collect_gn_files(&src_dir)?;
    let runtime_pkg = container.join("packages/@gnative/ui");
    for f in collect_gn_files(&runtime_pkg)? {
        if !gn_files.contains(&f) {
            gn_files.push(f);
        }
    }
    gn_files.sort();
    Ok(gn_files)
}

/// Fingerprint build tingkat-project: hash atas (versi compiler + epoch)
/// × (mode: dev/release) × (target) × (konten + rel-path setiap file `.gn`,
/// project maupun runtime) × (runtime bawaan di `RUNTIME_DIR`).
///
/// Mengubah salah satu — termasuk ganti versi/ABI codegen atau isi runtime
/// bawaan — membatalkan cache → rebuild penuh (tanpa artefak basi lintas
/// versi). Dibaca oleh `build_with_target` untuk skip nyata.
fn build_fingerprint(
    project_dir: &Path,
    mode: BuildMode,
    target: Option<&str>,
) -> GnResult<(u64, usize)> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h = OFFSET;
    // Versi compiler + epoch — invalidasi saat toolchain/ABI berubah.
    // Epoch ditanam literal (concat! butuh literal, bukan const).
    let version = concat!(env!("CARGO_PKG_VERSION"), ":epoch-7");
    h = fnv1a_seeded(h, version.as_bytes());
    // Mode codegen (dev = Cranelift, release = LLVM) → output berbeda.
    let mode_str = match mode {
        BuildMode::Dev => "dev",
        BuildMode::Release => "release",
    };
    h = fnv1a_seeded(h, mode_str.as_bytes());
    // Target (host = "").
    h = fnv1a_seeded(h, target.unwrap_or("").as_bytes());

    let container = project_dir.join("container");
    seed_runtime(&container)?;
    let src_dir = project_dir.join("src");
    let gn_files = collect_all_gn(project_dir)?;
    for file in &gn_files {
        let rel = relative_to(file, &src_dir).unwrap_or_else(|| file.display().to_string());
        h = fnv1a_seeded(h, rel.as_bytes());
        let source = read_gn_file(file)?;
        h = fnv1a_seeded(h, source.as_bytes());
    }
    // Runtime bawaan compiler (sumber otoritatif) — invalidasi saat berubah
    // walau copy ter-seed di container belum diperbarui.
    let runtime_src = Path::new(RUNTIME_DIR).join("@gnative/ui/ui.gn");
    if let Ok(text) = std::fs::read_to_string(&runtime_src) {
        h = fnv1a_seeded(h, text.as_bytes());
    }
    Ok((h, gn_files.len()))
}

/// Validasi project: seed + kumpulkan + parse + check + lower (tanpa
/// codegen/link). Menangkap error semantik yang tidak terlihat parse-level
/// (mis. komponen tanpa `render()`, prop hilang). Dipakai `gnative check`.
pub fn validate(project_dir: &Path) -> GnResult<BuildReport> {
    let (parsed, _cache) = collect_and_lower(project_dir, None)?;
    Ok(parsed.report)
}

/// Seed runtime `.gn` bawaan compiler ke `container/packages/@gnative/ui`
/// (jika belum ada) — paket ini ikut dikompilasi pipeline.
fn seed_runtime(container: &Path) -> GnResult<()> {
    let dest = container.join("packages/@gnative/ui");
    if dest.join("ui.gn").exists() {
        return Ok(());
    }
    let src = Path::new(RUNTIME_DIR).join("@gnative/ui/ui.gn");
    std::fs::create_dir_all(&dest).map_err(GnError::Io)?;
    let text = std::fs::read_to_string(&src).map_err(GnError::Io)?;
    std::fs::write(dest.join("ui.gn"), text).map_err(GnError::Io)?;
    Ok(())
}

fn relative_to(file: &Path, base: &Path) -> Option<String> {
    file.strip_prefix(base)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Batas ukuran satu file `.gn` (anti-OOM dari input jahat). Lihat audit P1-15.
const MAX_GN_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Baca file `.gn` dengan batas ukuran; tolak file raksasa dengan error jelas.
fn read_gn_file(path: &Path) -> GnResult<String> {
    let len = std::fs::metadata(path).map_err(GnError::Io)?.len();
    if len > MAX_GN_FILE_BYTES {
        return Err(GnError::Config(format!(
            "file `{}` terlalu besar ({} byte > batas {} byte)",
            path.display(),
            len,
            MAX_GN_FILE_BYTES
        )));
    }
    std::fs::read_to_string(path).map_err(GnError::Io)
}

/// Nama file output aman (hanya alfanumerik, `-` → `_`).
fn sanitize_name(project_name: &str) -> String {
    let mut name: String = project_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, 'a');
    }
    name
}

fn collect_gn_files(src_dir: &Path) -> GnResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> GnResult<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir).map_err(GnError::Io)? {
            let path = entry.map_err(GnError::Io)?.path();
            if path.is_dir() {
                walk(&path, files)?;
            } else if path.extension().map(|e| e == "gn").unwrap_or(false) {
                files.push(path);
            }
        }
        Ok(())
    }
    walk(src_dir, &mut files)?;
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Path libruntime (`target/debug/libgnative_runtime.a`).
    ///
    /// `Some` bila ada; `None` → test di-skip (dev tanpa runtime). Bila env
    /// `GNATIVE_REQUIRE_RUNTIME` di-set (CI), prasyarat absen = **gagal keras**
    /// (bukan skip diam) — lihat audit P1-14.
    fn runtime_lib_or_skip(name: &str) -> Option<PathBuf> {
        let lib = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/libgnative_runtime.a");
        if lib.exists() {
            return Some(lib);
        }
        if std::env::var("GNATIVE_REQUIRE_RUNTIME").is_ok() {
            panic!(
                "{name}: libruntime wajib ada (GNATIVE_REQUIRE_RUNTIME di-set) — \
                 jalankan `cargo build -p gnative-runtime`"
            );
        }
        eprintln!("skip {name}: libruntime belum di-build");
        None
    }

    /// Compile fungsi `.gn` → object → link (libc saja) → eksekusi → stdout.
    ///
    /// Membuktikan logika runtime (if/for/aritmatika/perbandingan/call)
    /// benar, bukan hanya bisa dikompilasi: `main` buatan memanggil fungsi
    /// `.gn` lalu mencetak `OK` bila hasilnya truthy, `GAGAL` bila tidak.
    fn e2e_run(gn_source: &str, callee: &str) -> String {
        let (module, diags) = gnative_lang::parse_source(gn_source);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = crate::lower::lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );

        let mut mir = crate::mir_lower::lower_mir(&out.module);
        mir.strings.push("OK".into());
        mir.strings.push("GAGAL".into());
        // Index pool sebenarnya (source bisa sudah meng-intern string lain).
        let ok_idx = mir.strings.iter().position(|s| s == "OK").unwrap();
        let fail_idx = mir.strings.iter().position(|s| s == "GAGAL").unwrap();

        let mut body: Vec<crate::mir::MirInst> = Vec::new();
        let ok_ptr = body.len() as u32;
        body.push(crate::mir::MirInst::StrAddr(ok_idx));
        let call = body.len() as u32;
        body.push(crate::mir::MirInst::Call {
            callee: callee.into(),
            args: vec![],
        });
        let then = 100u32;
        let els = 101u32;
        let end = 102u32;
        body.push(crate::mir::MirInst::Branch {
            cond: call,
            then,
            els,
        });
        body.push(crate::mir::MirInst::Label(then));
        body.push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![ok_ptr],
        });
        body.push(crate::mir::MirInst::Jump(end));
        body.push(crate::mir::MirInst::Label(els));
        let fail_ptr = body.len() as u32;
        body.push(crate::mir::MirInst::StrAddr(fail_idx));
        body.push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![fail_ptr],
        });
        body.push(crate::mir::MirInst::Jump(end));
        body.push(crate::mir::MirInst::Label(end));
        // `main` mengembalikan false (I8 0) agar exit code = 0.
        let zero = body.len() as u32;
        body.push(crate::mir::MirInst::ConstBool(false));
        body.push(crate::mir::MirInst::Ret(Some(zero)));
        mir.funcs.push(crate::mir::MirFunc {
            name: "main".into(),
            params: vec![],
            ret: crate::mir::MirTy::Bool,
            body,
        });

        let dir = std::env::temp_dir().join(format!("gnative-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let object = dir.join("e2e.o");
        crate::codegen::codegen(BuildMode::Dev, None, &mir, &object).unwrap();
        let bin = dir.join("e2e");
        let linker = crate::linker::find_linker().expect("linker tersedia");
        let status = std::process::Command::new(linker)
            .arg(&object)
            .arg("-o")
            .arg(&bin)
            .status()
            .unwrap();
        assert!(status.success(), "link gagal");

        let output = std::process::Command::new(&bin).output().unwrap();
        assert!(output.status.success(), "binary gagal dijalankan");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        stdout
    }

    #[test]
    fn e2e_function_logic_executes_correctly() {
        // aritmatika + perbandingan sederhana.
        assert_eq!(
            e2e_run(
                "function verify(): boolean { return 1 + 1 == 2; }",
                "verify"
            ),
            "OK"
        );
        // let + load variabel.
        assert_eq!(
            e2e_run(
                "function verify(): boolean { let t: number = 5; return t == 5; }",
                "verify"
            ),
            "OK"
        );
        // assignment ulang tanpa aritmatika.
        assert_eq!(
            e2e_run(
                "function verify(): boolean { let t: number = 0; t = 5; return t == 5; }",
                "verify"
            ),
            "OK"
        );
        // let + assignment + aritmatika.
        assert_eq!(
            e2e_run(
                "function verify(): boolean { let t: number = 0; t = t + 5; return t == 5; }",
                "verify"
            ),
            "OK"
        );
        // if/for/aritmatika/perbandingan → true.
        let src_ok = r#"
function verify(): boolean {
  let total: number = 0;
  for (n of [1, 2, 3, 4, 5]) {
    total = total + n;
  }
  if (total == 15) {
    return true;
  }
  return false;
}
"#;
        assert_eq!(e2e_run(src_ok, "verify"), "OK");

        // Cabang else (perbandingan salah) → false.
        let src_fail = r#"
function verify(): boolean {
  return 1 == 2;
}
"#;
        assert_eq!(e2e_run(src_fail, "verify"), "GAGAL");

        // Param + pemanggilan antar-fungsi.
        let src_param = r#"
function add(a: number, b: number): number {
  return a + b;
}
function verify(): boolean {
  return add(20, 22) == 42;
}
"#;
        assert_eq!(e2e_run(src_param, "verify"), "OK");

        // Operator relasional lengkap: `<`, `<=`, `>`, `>=`.
        assert_eq!(
            e2e_run(
                "function verify(): boolean { return 2 < 3 && 3 <= 3 && 4 > 3 && 4 >= 4; }",
                "verify"
            ),
            "OK"
        );
        assert_eq!(
            e2e_run(
                "function verify(): boolean { return 3 < 2 || 2 >= 3; }",
                "verify"
            ),
            "GAGAL"
        );
        // Concat string literal (folding konstanta di lower).
        assert_eq!(
            e2e_run(
                "function verify(): boolean { let s: string = \"ha\" + \"lo\"; return s == \"halo\"; }",
                "verify"
            ),
            "OK"
        );
        // Modulo (P1-05): `%` dikompilasi nyata (frem).
        assert_eq!(
            e2e_run(
                "function verify(): boolean { return 10 % 3 == 1; }",
                "verify"
            ),
            "OK"
        );
        assert_eq!(
            e2e_run(
                "function verify(): boolean { return 10 % 3 == 2; }",
                "verify"
            ),
            "GAGAL"
        );
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_name("demo"), "demo");
        assert_eq!(sanitize_name("my-app"), "my-app");
        assert_eq!(sanitize_name("a b"), "a_b");
        assert_eq!(sanitize_name("123app"), "a123app");
    }

    #[test]
    fn build_sample_to_binary() {
        let dir = std::env::temp_dir().join(format!("gnative-pipe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("gnative.toml"),
            crate::config::Config::template("demo"),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.gn"),
            r#"use "@gnative/ui";
window App {
  title: "Halo";
  size: [800, 600];
  root: <vstack gap={8}>
    <text content="Hi" size={32} />
  </vstack>;
}
"#,
        )
        .unwrap();

        let report = build(&dir, BuildMode::Dev).unwrap();
        assert!(!report.has_errors, "diagnostics: {:?}", report.diagnostics);
        let object = report.object_file.expect("object ada");
        assert!(object.exists(), "object `{}` ada", object.display());

        // Object hasil codegen memuat intrinsik runtime (ter-resolve saat link).
        let bytes = std::fs::read(&object).unwrap();
        assert_eq!(&bytes[0..4], b"\x7fELF");

        // Link ke binary native: butuh libruntime + display; diuji manual
        // (`cargo build -p gnative-runtime` lalu `gnative run`).
        if let Some(binary) = &report.binary {
            assert!(binary.exists(), "binary `{}` ada", binary.display());
        }

        // Build kedua → cache hit tingkat-project (skip nyata: parse/lower/
        // codegen/link dilewati, artefak dipakai ulang).
        let report2 = build(&dir, BuildMode::Dev).unwrap();
        assert!(report2.cache_hits > 0, "build kedua harus cache hit");
        assert!(report2.regenerated == 0, "semua file cache hit");
        assert!(report2.object_file.as_ref().unwrap().exists());

        // Ubah isi source → build ketiga harus rebuild (cache tidak basi).
        std::fs::write(
            dir.join("src/main.gn"),
            r#"use "@gnative/ui";
window App {
  title: "Halo2";
  size: [800, 600];
  root: <vstack gap={8}>
    <text content="Hi" size={32} />
  </vstack>;
}
"#,
        )
        .unwrap();
        let report3 = build(&dir, BuildMode::Dev).unwrap();
        assert!(
            !report3.has_errors,
            "diagnostics: {:?}",
            report3.diagnostics
        );
        // Build tingkat-project tidak lagi cache hit; file yang berubah
        // (main.gn) di-rebuild → `regenerated > 0`.
        assert!(
            report3.regenerated > 0,
            "perubahan source → minimal satu file rebuild"
        );
        // Build keempat (tanpa perubahan lagi) → cache hit kembali.
        let report4 = build(&dir, BuildMode::Dev).unwrap();
        assert!(report4.cache_hits > 0, "setelah stabil → cache hit lagi");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_fingerprint_stabil_dan_sensitif_versi() {
        let dir = std::env::temp_dir().join(format!("gnative-fp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("gnative.toml"),
            crate::config::Config::template("demo"),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.gn"),
            r#"use "@gnative/ui"; window App { title: "x"; root: <text content="y"/>; }"#,
        )
        .unwrap();

        // Sama input → key sama (deterministik).
        let (k1, n1) = build_fingerprint(&dir, BuildMode::Dev, None).unwrap();
        let (k2, n2) = build_fingerprint(&dir, BuildMode::Dev, None).unwrap();
        assert_eq!(k1, k2);
        assert_eq!(n1, n2);
        let base_count = n1;

        // Mode berbeda → key berbeda (flag codegen mengubah output).
        let (k_rel, _) = build_fingerprint(&dir, BuildMode::Release, None).unwrap();
        assert_ne!(k1, k_rel, "ganti flag dev→release harus beda key");

        // Target berbeda → key berbeda.
        let (k_tgt, _) =
            build_fingerprint(&dir, BuildMode::Dev, Some("aarch64-unknown-linux-gnu")).unwrap();
        assert_ne!(k1, k_tgt, "ganti target harus beda key");

        // Ubah isi source → key berbeda (invalidasi konten).
        std::fs::write(
            dir.join("src/main.gn"),
            r#"use "@gnative/ui"; window App { title: "z"; root: <text content="y"/>; }"#,
        )
        .unwrap();
        let (k3, n3) = build_fingerprint(&dir, BuildMode::Dev, None).unwrap();
        assert_ne!(k1, k3, "ubah source harus beda key");
        assert_eq!(n3, base_count, "jumlah file tetap");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_stateful_to_object() {
        // Komponen stateful + klik end-to-end: `.gn` → object ELF (init state,
        // render inline, `Counter__inc`, clickable + registrasi). Link + run manual.
        let dir = std::env::temp_dir().join(format!("gnative-state-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("gnative.toml"),
            crate::config::Config::template("stateful"),
        )
        .unwrap();
        std::fs::write(
            dir.join("src/main.gn"),
            r#"use "@gnative/ui";
component Counter {
    state { count: number = 0; pesan: string = "awal"; }
    render(): Element {
        return <vstack gap={8}>
            <text content={this.pesan} size={20} />
            <button label="+" onClick={this.inc} />
        </vstack>;
    }
    inc(): void { count += 1; }
}
window App {
  title: "State";
  size: [800, 600];
  root: <vstack gap={8}>
    <Counter/>
  </vstack>;
}
"#,
        )
        .unwrap();

        let report = build(&dir, BuildMode::Dev).unwrap();
        assert!(!report.has_errors, "diagnostics: {:?}", report.diagnostics);
        let object = report.object_file.expect("object ada");
        let bytes = std::fs::read(&object).unwrap();
        assert_eq!(&bytes[0..4], b"\x7fELF");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn e2e_state_mutation_executes() {
        // Mutasi state end-to-end: init 0 → `Counter__0__inc` 2x → 2.
        // Membuktikan state instance runtime bertahan antar panggilan.
        // Butuh libruntime (`cargo build -p gnative-runtime`); skip bila absen.
        let src = r#"
component Counter {
  state { count: number = 0; }
  render(): Element { return <text content="x" />; }
  inc(): number { count += 1; return count; }
}
window App { title: "x"; root: <Counter/>; }
"#;
        let (module, diags) = gnative_lang::parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = crate::lower::lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );

        let mut mir = crate::mir_lower::lower_mir(&out.module);
        assert!(
            mir.funcs.iter().any(|f| f.name == "Counter__0__inc"),
            "Counter__0__inc ada: {:?}",
            mir.funcs.iter().map(|f| &f.name).collect::<Vec<_>>()
        );
        // Sisakan hanya method instance; `main`+build bawaan diganti uji.
        mir.funcs.retain(|f| f.name.starts_with("Counter__"));
        for s in ["OK", "GAGAL"] {
            if !mir.strings.iter().any(|x| x == s) {
                mir.strings.push(s.into());
            }
        }
        let idx = |s: &str| mir.strings.iter().position(|x| x == s).unwrap();
        let (ok_idx, fail_idx, comp_idx, field_idx) =
            (idx("OK"), idx("GAGAL"), idx("Counter#0"), idx("count"));

        let mut body: Vec<crate::mir::MirInst> = Vec::new();
        let mut push = |inst: crate::mir::MirInst| -> u32 {
            let slot = body.len() as u32;
            body.push(inst);
            slot
        };
        let c = push(crate::mir::MirInst::StrAddr(comp_idx));
        let f = push(crate::mir::MirInst::StrAddr(field_idx));
        let zero = push(crate::mir::MirInst::ConstF64(0.0));
        push(crate::mir::MirInst::Call {
            callee: "gn_state_set_f64".into(),
            args: vec![c, f, zero],
        });
        push(crate::mir::MirInst::Call {
            callee: "Counter__0__inc".into(),
            args: vec![],
        });
        let r2 = push(crate::mir::MirInst::Call {
            callee: "Counter__0__inc".into(),
            args: vec![],
        });
        let two = push(crate::mir::MirInst::ConstF64(2.0));
        let cond = push(crate::mir::MirInst::CmpF64 {
            op: crate::mir::MirCmpOp::Eq,
            a: r2,
            b: two,
        });
        let (then, els, end) = (200u32, 201u32, 202u32);
        push(crate::mir::MirInst::Branch { cond, then, els });
        push(crate::mir::MirInst::Label(then));
        let ok = push(crate::mir::MirInst::StrAddr(ok_idx));
        push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![ok],
        });
        push(crate::mir::MirInst::Jump(end));
        push(crate::mir::MirInst::Label(els));
        let fail = push(crate::mir::MirInst::StrAddr(fail_idx));
        push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![fail],
        });
        push(crate::mir::MirInst::Jump(end));
        push(crate::mir::MirInst::Label(end));
        let zero_b = push(crate::mir::MirInst::ConstBool(false));
        push(crate::mir::MirInst::Ret(Some(zero_b)));
        mir.funcs.push(crate::mir::MirFunc {
            name: "main".into(),
            params: vec![],
            ret: crate::mir::MirTy::Bool,
            body,
        });

        let Some(runtime_lib) = runtime_lib_or_skip("e2e_state") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("gnative-state-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let object = dir.join("e2e.o");
        crate::codegen::codegen(BuildMode::Dev, None, &mir, &object).unwrap();
        let bin = dir.join("e2e");
        let linker = crate::linker::find_linker().expect("linker tersedia");
        let status = std::process::Command::new(linker)
            .arg(&object)
            .arg(&runtime_lib)
            .args(["-lpthread", "-ldl", "-lm"])
            .arg("-o")
            .arg(&bin)
            .status()
            .unwrap();
        assert!(status.success(), "link dengan libruntime gagal");
        let output = std::process::Command::new(&bin).output().unwrap();
        assert!(output.status.success(), "binary gagal dijalankan");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(stdout, "OK", "state harus 2 setelah 2x inc");
    }

    #[test]
    fn e2e_funcaddr_indirect_executes() {
        // Alamat fungsi kompilasi bisa dipanggil tak-langsung — mekanisme
        // yang dipakai dispatch klik runtime (`gn_register_method` + call).
        // Butuh libruntime (`cargo build -p gnative-runtime`); skip bila absen.
        let src = r#"
component Counter {
  state { count: number = 0; }
  render(): Element { return <text content="x" />; }
  inc(): number { count += 1; return count; }
}
window App { title: "x"; root: <Counter/>; }
"#;
        let (module, diags) = gnative_lang::parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = crate::lower::lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );

        let mut mir = crate::mir_lower::lower_mir(&out.module);
        assert!(
            mir.funcs.iter().any(|f| f.name == "Counter__0__inc"),
            "Counter__0__inc ada"
        );
        // Sisakan hanya method instance; `main`+build bawaan diganti uji.
        mir.funcs.retain(|f| f.name.starts_with("Counter__"));
        for s in ["OK", "GAGAL"] {
            if !mir.strings.iter().any(|x| x == s) {
                mir.strings.push(s.into());
            }
        }
        let idx = |s: &str| mir.strings.iter().position(|x| x == s).unwrap();
        let (ok_idx, fail_idx, comp_idx, field_idx) =
            (idx("OK"), idx("GAGAL"), idx("Counter#0"), idx("count"));

        let mut body: Vec<crate::mir::MirInst> = Vec::new();
        let mut push = |inst: crate::mir::MirInst| -> u32 {
            let slot = body.len() as u32;
            body.push(inst);
            slot
        };
        let c = push(crate::mir::MirInst::StrAddr(comp_idx));
        let f = push(crate::mir::MirInst::StrAddr(field_idx));
        let zero = push(crate::mir::MirInst::ConstF64(0.0));
        push(crate::mir::MirInst::Call {
            callee: "gn_state_set_f64".into(),
            args: vec![c, f, zero],
        });
        let addr = push(crate::mir::MirInst::FuncAddr {
            func: "Counter__0__inc".into(),
        });
        // Dua panggilan tak-langsung; hasil kedua harus 2.0.
        for _ in 0..1 {
            push(crate::mir::MirInst::CallIndirect {
                target: addr,
                params: vec![],
                ret: crate::mir::MirTy::F64,
                args: vec![],
            });
        }
        let r2 = push(crate::mir::MirInst::CallIndirect {
            target: addr,
            params: vec![],
            ret: crate::mir::MirTy::F64,
            args: vec![],
        });
        let two = push(crate::mir::MirInst::ConstF64(2.0));
        let cond = push(crate::mir::MirInst::CmpF64 {
            op: crate::mir::MirCmpOp::Eq,
            a: r2,
            b: two,
        });
        let (then, els, end) = (300u32, 301u32, 302u32);
        push(crate::mir::MirInst::Branch { cond, then, els });
        push(crate::mir::MirInst::Label(then));
        let ok = push(crate::mir::MirInst::StrAddr(ok_idx));
        push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![ok],
        });
        push(crate::mir::MirInst::Jump(end));
        push(crate::mir::MirInst::Label(els));
        let fail = push(crate::mir::MirInst::StrAddr(fail_idx));
        push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![fail],
        });
        push(crate::mir::MirInst::Jump(end));
        push(crate::mir::MirInst::Label(end));
        let zero_b = push(crate::mir::MirInst::ConstBool(false));
        push(crate::mir::MirInst::Ret(Some(zero_b)));
        mir.funcs.push(crate::mir::MirFunc {
            name: "main".into(),
            params: vec![],
            ret: crate::mir::MirTy::Bool,
            body,
        });

        let Some(runtime_lib) = runtime_lib_or_skip("e2e_indirect") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("gnative-ind-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let object = dir.join("e2e.o");
        crate::codegen::codegen(BuildMode::Dev, None, &mir, &object).unwrap();
        let bin = dir.join("e2e");
        let linker = crate::linker::find_linker().expect("linker tersedia");
        let status = std::process::Command::new(linker)
            .arg(&object)
            .arg(&runtime_lib)
            .args(["-lpthread", "-ldl", "-lm"])
            .arg("-o")
            .arg(&bin)
            .status()
            .unwrap();
        assert!(status.success(), "link dengan libruntime gagal");
        let output = std::process::Command::new(&bin).output().unwrap();
        assert!(output.status.success(), "binary gagal dijalankan");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(stdout, "OK", "panggil tak-langsung harus jalan");
    }

    #[test]
    fn e2e_release_executes() {
        // Backend LLVM nyata: logika .gn → object release → link → jalan.
        // Butuh libruntime + LLVM 18; skip bila libruntime absen.
        let src = r#"
function verify(): boolean {
  let total: number = 0;
  for (n of [1, 2, 3, 4, 5]) {
    total = total + n;
  }
  if (total == 15) {
    return true;
  }
  return false;
}
"#;
        let (module, diags) = gnative_lang::parse_source(src);
        assert!(diags.is_empty(), "diagnostics: {diags:?}");
        let out = crate::lower::lower(&module).unwrap();
        assert!(
            out.diagnostics.is_empty(),
            "diagnostics: {:?}",
            out.diagnostics
        );

        let mut mir = crate::mir_lower::lower_mir(&out.module);
        mir.strings.push("OK".into());
        mir.strings.push("GAGAL".into());
        let ok_idx = mir.strings.iter().position(|s| s == "OK").unwrap();
        let fail_idx = mir.strings.iter().position(|s| s == "GAGAL").unwrap();

        let mut body: Vec<crate::mir::MirInst> = Vec::new();
        let ok_ptr = body.len() as u32;
        body.push(crate::mir::MirInst::StrAddr(ok_idx));
        let call = body.len() as u32;
        body.push(crate::mir::MirInst::Call {
            callee: "verify".into(),
            args: vec![],
        });
        let (then, els, end) = (400u32, 401u32, 402u32);
        body.push(crate::mir::MirInst::Branch {
            cond: call,
            then,
            els,
        });
        body.push(crate::mir::MirInst::Label(then));
        body.push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![ok_ptr],
        });
        body.push(crate::mir::MirInst::Jump(end));
        body.push(crate::mir::MirInst::Label(els));
        let fail_ptr = body.len() as u32;
        body.push(crate::mir::MirInst::StrAddr(fail_idx));
        body.push(crate::mir::MirInst::Call {
            callee: "puts".into(),
            args: vec![fail_ptr],
        });
        body.push(crate::mir::MirInst::Jump(end));
        body.push(crate::mir::MirInst::Label(end));
        let zero = body.len() as u32;
        body.push(crate::mir::MirInst::ConstBool(false));
        body.push(crate::mir::MirInst::Ret(Some(zero)));
        mir.funcs.push(crate::mir::MirFunc {
            name: "main".into(),
            params: vec![],
            ret: crate::mir::MirTy::Bool,
            body,
        });

        let Some(runtime_lib) = runtime_lib_or_skip("e2e_release") else {
            return;
        };
        let dir = std::env::temp_dir().join(format!("gnative-rel-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let object = dir.join("e2e.o");
        crate::codegen::codegen(BuildMode::Release, None, &mir, &object).unwrap();
        let bin = dir.join("e2e");
        let linker = crate::linker::find_linker().expect("linker tersedia");
        let status = std::process::Command::new(linker)
            .arg(&object)
            .arg(&runtime_lib)
            .args(["-lpthread", "-ldl", "-lm"])
            .arg("-o")
            .arg(&bin)
            .status()
            .unwrap();
        assert!(status.success(), "link dengan libruntime gagal");
        let output = std::process::Command::new(&bin).output().unwrap();
        assert!(output.status.success(), "binary gagal dijalankan");
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(stdout, "OK", "logika release harus benar");
    }

    // --- P1-11/12: graph, package hilang, multi-window, siklus ---

    fn write_project(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gnative-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("gnative.toml"),
            crate::config::Config::template("demo"),
        )
        .unwrap();
        for (f, content) in files {
            let p = dir.join("src").join(f);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, content).unwrap();
        }
        dir
    }

    #[test]
    fn package_hilang_gagalkan_build() {
        // P1-11: `use` ke package tak ada → has_errors (bukan hanya diagnostik).
        let dir = write_project(
            "missdep",
            &[(
                "main.gn",
                r#"use "@tak/ada";
window App { title: "x"; root: <text content="y"/>; }"#,
            )],
        );
        let report = build(&dir, BuildMode::Dev).unwrap();
        assert!(report.has_errors, "package hilang harus menggagalkan build");
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.contains("tidak ditemukan")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_window_ditolak() {
        // P1-06/11: dua window → error, tidak menimpa diam.
        let dir = write_project(
            "multiwin",
            &[(
                "main.gn",
                r#"use "@gnative/ui";
window A { title: "a"; root: <text content="x"/>; }
window B { title: "b"; root: <text content="y"/>; }"#,
            )],
        );
        let report = build(&dir, BuildMode::Dev).unwrap();
        assert!(report.has_errors, "multi-window harus error");
        assert!(report.diagnostics.iter().any(|d| d.contains("window")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn siklus_dependency_ditolak() {
        // P1-11: a.gn ↔ b.gn saling `use` → siklus → error.
        let dir = write_project(
            "cycle",
            &[
                (
                    "a.gn",
                    r#"use "b.gn";
window App { title: "x"; root: <text content="y"/>; }"#,
                ),
                ("b.gn", r#"use "a.gn";"#),
            ],
        );
        let report = build(&dir, BuildMode::Dev).unwrap();
        assert!(
            report.diagnostics.iter().any(|d| d.contains("siklus")),
            "siklus harus terdeteksi, diagnostics: {:?}",
            report.diagnostics
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
