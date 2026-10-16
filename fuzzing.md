# Fuzzing.md — Rencana Validasi Fuzzing, Memori, dan Runtime (Gnative)

Status dokumen ini: **discovery Phase 1 + rancangan target**, bukan implementasi.
Belum ada harness yang dibuat. Dilarang memulai implementasi sebelum instruksi
eksplisit dari maintainer.

Semua klaim di bawah diberi label:
- **Terukur** = sudah diverifikasi dari repo/lingkungan saat penulisan ini.
- **Terinferensi** = disimpulkan dari struktur kode/docs, belum dibuktikan
  lewat implementasi/test.
- **Tidak terverifikasi** = asumsi/risiko yang harus dibuktikan nanti.

---

## 1. Ringkasan Repository (Discovery)

- Nama paket root: `gnative` versi workspace `1.0.0-rc.1` (Terukur).
- Edisi Rust 2021, MSRV `1.80`, resolver 2, lockfile di-commit (Terukur).
- Workspace berisi 8 crate inti + paket root (Terukur):
  `gnative-core`, `gnative-lang`, `gnative-compiler`, `gnative-gfx`,
  `gnative-gfx-wgpu`, `gnative-gfx-cuda`, `gnative-runtime`, `gnative-cli`.
- Entry point: `src/main.rs` hanya delegasi ke `gnative_cli::run()` (Terukur).
- Pipeline compiler: `.gn` → AST → Semantic IR → MIR → codegen Cranelift/LLVM →
  object → linker dengan `libgnative_runtime.a` (Terukur dari docs + `pipeline.rs`).
- Rendering utama: wgpu 30; CUDA hanya kerangka dan default mati (Terukur).
- Toolchain terukur di host:
  - `rustc 1.100.0-nightly`, `cargo 1.100.0-nightly`.
  - `cargo-fuzz 0.13.2` tersedia.
  - komponen `miri` terpasang.
  - LLVM 18 tersedia di `/usr/lib/llvm-18`.
  - `strip` dan `objcopy` tersedia.
- CI: `ci/ci.yml` menjalankan format, clippy (deny warnings), check workspace,
  build libruntime, test workspace, cargo audit live, provenance (Terukur).
- Belum ada direktori `fuzz/`, benchmark, harness, atau test coverage-guided
  (Terukur via pencarian pola `fuzz`).

---

## 2. Peta Subsystem dan Surface Fuzzable

### 2.1 `gnative-core`

File tanggung jawab (Terukur):
- `color.rs`, `vec.rs`, `rect.rs`, `size.rs`, `units.rs`
- `error.rs` — `GnError`, `GnResult`
- `event.rs` — `Event`, `WindowEvent`, `MouseEvent`, `KeyEvent`, `Key`
- `path.rs` — `safe_join` (anti path-traversal)

Surface fuzzable (Terukur):
- `safe_join(root, rel)` sangat cocok untuk fuzz property-based karena anti
  traversal dan harus menjamin hasil tetap di dalam `root`.
- Struktur geometri (`Rect`, `Size`, `Vec2`) cocok untuk fuzz pada layout/runtime.

### 2.2 `gnative-lang`

File tanggung jawab (Terukur):
- `lexer.rs`, `parser.rs`, `ast.rs`, `checker.rs`, `ty.rs`, `span.rs`,
  `diagnostic.rs`, `token.rs`.

Surface fuzzable (Terukur):
- `parse_source(source: &str) -> (Module, Vec<Diagnostic>)` adalah entry utama.
- Lexer dan parser dipanggil dari `parse_source`.
- AST dan checker tidak punya entry publik terpisah di level crate; semantic
  path dicapai melalui `parse_source`.
- `Diagnostic::render_all(diagnostics, source_name)` cocok untuk fuzz formatting.

Risiko tinggi (Terinferensi):
- stack overflow dari ekspresi/komponen nested, oversized tokens, malformed
  Unicode/escape.
- panic pada lexer/parser/checker.
- nontermination pada analisis scope/dependency.
- error recovery yang cascading.

### 2.3 `gnative-compiler`

File tanggung jawab (Terukur):
- `config.rs`, `resolve.rs`, `graph.rs`, `fingerprint.rs`, `cache.rs`,
  `ir.rs`, `lower.rs`, `mir.rs`, `mir_lower.rs`, `codegen/`, `linker.rs`,
  `artifact.rs`, `lockfile.rs`, `advisory.rs`, `install.rs`, `pipeline.rs`.

Surface publik fuzzable (Terukur):
- `lower::lower(module) -> GnResult<LowerOutput>`
- `mir_lower::lower_mir(ir) -> MirModule`
- `codegen::used_intrinsics`, `codegen::backend_for`, `codegen::codegen`
- `config::Config::parse` via tests? perlu verifikasi method publik.
- `fingerprint::fnv1a`, `source_fingerprint`, `to_hex`
- `advisory::compare_versions`, `satisfies`, `group_by_advisory`
- `path`-independent helpers: `human_size`, `mono_method_name`, `instance_key`
- `cache`, `resolve`, `graph`, `lockfile`, `install`, `pipeline` melibatkan I/O
  dan filesystem; harus diisolasi subprocess atau temporary directory.

Boundary penting (Terukur):
- Pipeline publik: `pipeline::build`, `pipeline::build_with_target`,
  `pipeline::validate`.
- Codegen publik: `codegen::codegen(mode, target, mir, out)`.
- Linker publik: `linker::link`, `link_with_target`, `find_linker`,
  `find_runtime_dir`.

Batas keamanan (Terukur):
- Unsafe Rust hanya di `codegen/llvm.rs` (1 lokasi `unsafe`) dan runtime FFI.
- Codegen LLVM bergantung eksternal LLVM 18 (`LLVM_SYS_180_PREFIX`).
- CLI/build berinteraksi dengan filesystem dan binary eksternal (`strip`,
  `objcopy`, linker, runtime dir).

### 2.4 `gnative-gfx`

Surface fuzzable (Terukur):
- `backend.rs`, `frame.rs`, `scene.rs`
- layout: `Scene::layout()` likely target CPU tinggi, perlu inspeksi lebih lanjut.

### 2.5 `gnative-gfx-wgpu`

File tanggung jawab (Terukur):
- `device.rs`: `create_instance`, `request_adapter`, `request_device`
- `surface.rs`, `vertex.rs`, `shader.rs`, `atlas.rs`, `renderer.rs`

Surface fuzzable (Terukur, tanpa GPU-native call):
- `atlas::glyph_index`, `glyph_pixel`, `glyph_uv`, `solid_uv`, `build_image`
  (Terukur).
- `device::create_instance` bisa dipanggil, tetapi adanya adapter/device
  bergantung environment (Terukur inferensi).
- Renderer penuh membutuhkan GPU/display, bukan target fuzzing awal.

Risiko (Terukur inferensi):
- device lost, surface lost, resize, repeated init; butuh harness integrasi
  terbatas dan deteksi lingkungan.

### 2.6 `gnative-gfx-cuda`

File tanggung jawab (Terukur):
- `renderer.rs`: `CudaRenderer::new()` default `BackendUnavailable`.

Surface:
- hanya fallback kerangka; target baseline = pengujian error path/availability
  (Terukur).

### 2.7 `gnative-runtime`

File tanggung jawab (Terukur):
- `backend_select.rs`: `select_backend`
- `event_loop.rs`: `run_app`, `run_app_dynamic`
- `input.rs`: `map`
- `intrinsics.rs`: C-ABI + `read_cstr`, state tamu
- `state_store.rs`: `gn_state_*`
- `ui_builder.rs`: `UiNode`, `build_scene`

Surface fuzzable CPU-friendly (Terukur):
- `input::map(WindowEvent-like, Vec2)` (perlu signature pasti via code graph)
- `ui_builder::UiNode::to_scene_node`, `build_scene`
- `state_store::gn_state_*` via FFI pointer validation in unit tests possible.
- `intrinsics::read_cstr` (pub(crate), but testable as unit within crate).

Risiko tinggi (Terukur inferensi):
- C-style pointer validation, locking/poisoned mutex, panik di batas ABI.
- state corruption / invalid geometry.
- unbounded resource growth dalam state store (sudah ada cap 4096, tetapi
  multi-instance belum; Terukur).

### 2.8 `gnative-cli`

File tanggung jawab (Terukur):
- cmd/*: audit, build, check, clean, container, dev, doctor, graph, install,
  list, new, publish, run, mod.
- lib.rs `run(args)`.

Surface fuzzable (Terukur):
- CLI argument dispatch, traversal rejection, audit DB missing, help.
- Buang generated binaries pada host tanpa isolasi dilarang; test CLI harus
  di temp dir.

---

## 3. Infrastruktur Fuzzing (Desain, Belum Diimplementasikan)

Format workspace:
- Saran: tambahkan direktori `fuzz/` di root repo, terpisah dari crate inti:
  - `fuzz/Cargo.toml` dengan dependencies antar crate workspace (`path = ".."`,
    `default-features` off jika perlu).
  - `fuzz/fuzz_targets/*.rs`
  - `fuzz/corpus/<target>/{seed,regression}` atau `corpus/` + `artifacts/`.
  - `fuzz/seeds/` untuk initial seeds.
- `cargo-fuzz` / libFuzzer compatible dengan nightly (Terukur), tetapi target Rust
  native daurat membutuhkan sanitizer yang mendukung path yang relevan.
- Gunakan target library narrow; jangan launch app/window per input.

Bounded execution:
- Batasi input size (misalnya 1–64 KiB per target; angka ini awal, harus
  diukur dan disesuaikan).
- Batasi recursion/nesting di generator structured.
- Timeout libFuzzer (`-timeout`), RSS cap (`-rss_limit_mb`), dan max total time.
- Hindari arbitrary large allocations via generators.

Oracles wajib:
- no panic.
- no crash/OOM.
- no hang.
- deterministic output bila kontrak mensyaratkan.
- check invariant struktural.
- sanitizer clean pada path compatible.

Regression:
- simpan original repro.
- minimization via `cargo fuzz tmin` atau manual.
- focused unit test.
- tambahkan seed ke corpus permanen bila sesuai.

---

## 4. Matriks Target Prioritas (Target Order)

### P0 — Wajib sebelum release, risiko tinggi, biaya rendah

1. `gn_src_parse`
   - API: `gnative_lang::parse_source`
   - Tipe: string raw
   - Oracles: no panic, no recursion crash/OOM, diagnostics tidak panic,
     span valid.
   - Risiko: parser recursion, malformed strings/escape.

2. `gn_lexer_tokens`
   - API: `Lexer::new(src).tokenize()` (perlu verifikasi tipe return dan
     diagnostics) dan/atau `parse_source`.
   - Tipe: string raw
   - Oracles: token spans within source, no panic.

3. `gn_util_safe_join`
   - API: `gnative_core::safe_join`
   - Tipe: random `(root Path, rel String)`
   - Oracles: relative/not traversal invariant; `starts_with(root)`.

4. `gn_core_geometry`
   - API: `Rect`/`Size` interactions jika digunakan parser/runtime.
   - Tipe: structured numeric
   - Oracles: finite/expected nonnegative where documented.

5. `gn_runtime_ui_builder`
   - API: `UiNode::to_scene_node`, `build_scene`
   - Tipe: structured generated `UiNode`
   - Oracles: no panic, coordinates valid, traversal terminates.
   - Risiko: deep nested tree causing recursion/memory.

6. `gn_runtime_state_store`
   - API: `gn_state_*` C-ABI
   - Tipe: sequence of set/get with random C strings/nulls/large values
   - Oracles: no panic, no dangling pointer use in test harness,
     cap 4096 terhormat.

7. `gn_input_event_mapping`
   - API: `gnative_runtime::input::map`
   - Tipe: random events
   - Oracles: panic-free, valid output or None.

### P1 — Compiler IR/MIR dan determinism

8. `gn_ir_lower`
   - API: `gnative_lang::parse_source` lalu `gnative_compiler::lower::lower`
   - Tipe: valid/invalid .gn
   - Oracles: failure handling, no panic, spans, determinism.

9. `gn_mir_lower`
   - API: `lower_mir`
   - Tipe: generated `IrModule` via supported path.
   - Caveat: jangan bangun invalid internal structure kecuali target eksplisit
     IR validation.

10. `gn_codegen_cranelift`
    - API: `codegen::codegen(BuildMode::Dev, None, mir, out)` di temp file.
    - Tipe: MirModule dari valid source.
    - Oracles: object emits or expected error; no crash; instruction validity.

11. `gn_codegen_llvm`
    - API: same dengan `BuildMode::Release`
    - Oracles: build succeeds or structured error; backend parity where
      semantically documented.

12. `gn_fingerprint_fnv`
    - API hash determinism.
    - Oracles: avalanche? minimal/no crash, collision understanding.

13. `gn_advisory_versions`
    - API: `compare_versions`, `satisfies`, `group_by_advisory`
    - Tipe: random strings.
    - Oracles: total order properties where required, no panic.

### P2 — Runtime sequences and lifecycle

14. `gn_runtime_sequences`
    - API: generate finite sequence valid operations:
      app/window create, mount tree, set state, event dispatch, resize, focus,
      update text, unload resource, destroy widget, close window, shutdown.
    - Oracles: no use-after-destroy, no deadlock, bounded event queues,
      resource cleanup.
    - Status: API sequence harus dimodelkan manual (Terinferensi; perlu API).

15. `gn_layout_properties`
    - API: `Scene::layout()` and widget geometries.
    - Oracles: terminates, coordinates valid.

16. `gn_text_atlas`
    - API: glyph functions/build_image.
    - Oracles: bounds, no OOM, unicode handling.

### P3 — GPU/renderer dan integrasi

17. `gn_wgpu_device_lifecycle`
    - API: `create_instance`, `request_adapter`, `request_device`
    - Environment-dependent.
    - Oracles: clean error handling no panic.

18. `gn_render_lifecycle`
    - API: controlled harness with bounded repeated redraw/resizes.
    - Oracles: no resource leak proxy, validation errors classified.

19. `gn_backend_select`
    - API: `select_backend`
    - Oracles: deterministic fallback.

### P4 — Build/install/container

20. `gn_config_parse`
    - API: config parsing obscure; verifikasi method signature.
21. `gn_safe_build_pipeline_subprocess`
    - CLI/build integrated test in temp project under timeout.
    - Risiko: expensive WGPU/LLVM; jangan di setiap PR kecuali smoke.
22. `gn_container_path_rejection`
    - Already tested via CLI tests (Terukur).

---

## 5. Strategi Memori dan Leak

Wajib:
- Jangan gunakan RSS semata sebagai bukti leak; gunakan alloc tracing, profilers,
  object lifecycle counters.
- Baseline warm-up, repeated lifecycle, checkpoint trend fitting.
- Pisahkan kategori: heap, live alloc count, FD/handle, threads, temp files,
  cache, GPU proxies.
- Distinguish retained arena/cache/fragmentation from leak.

Tools (kompatibilitas yang perlu divergence):
- `cargo-fuzz` libFuzzer + ASAN/LSAN pada target native yang compatible.
- Miri pada unit test yang kompatibel.
- Valgrind/Massif untuk workload terpilih.
- Process metrics (`/proc` atau `getrusage`) untuk soak tests.
- wgpu validation/diagnostics.

Limitasi:
- GPU memory tidak sepenuhnya observable.
- runtime staticlib + wgpu mungkin tidak fully LSAN-compatible; harus dites.
- LSAN dapat mendeteksi retained global cache; klassifikasi wajib.

Kriteria leak (awal; harus diukur):
- Setelah warm-up dan repeated workload, pertumbuhan post-warm-up harus stabil
  dalam toleransi berbasis noise.
- Pertumbuhan linear dengan iterasi + unmatched destroyed objects = finding.
- Pertumbuhan hanya pada cache terdokumentasi = bukan finding, but verify cache
  eviction.

---

## 6. Spek Memori: Workloads Must-Run

1. compile-and-destroy same source N kali.
2. compile many distinct sources.
3. create/destroy window N kali (bila API tersedia headless? perlu verifikasi).
4. mount/unmount UI tree N kali.
5. repeated state updates N kali.
6. load/release fonts, images, textures, buffers.
7. resize/reconfig surfaces.
8. error paths during init/shutdown.
9. repeated lexer/parser same input.

Measurement fields:
- iterations
- RSS before/after warm-up
- live allocations
- peak RSS
- FD count
- thread count
- cache size
- GPU resource proxies
- duration
- timeout/OOM.

---

## 7. Runtime Stateful Fuzzing

Model finite sequence of operations terikat sangat. Minimal reproducer sequence
wajib (< say 20 steps). Stateful oracle list di Phase 5 user mandate sudah
diadopsi. Concurrency tests hanya bila arsitektur mengizinkan; TSAN hanya
pada compatible builds.

---

## 8. Layout, Widget, Text

Property tests:
- zero/negative/extreme dimensions jika API menerima.
- nested/contradictory constraints.
- long text/unicode.
- empty/large trees.
- rapid state updates.
- insertion/removal.
- focus transition.
- clipping/overflow.
- DPI changes.
- repeated font/resource loads.
- invalid property combos.

Properties:
- layout terminates.
- coordinates valid by contract.
- constraints respected.
- rendering tidak mengakses invalid resources.
- state updates bounded.
- deterministic consistent results.

Visual correctness tidak boleh disimpulkan dari geometry saja; perlu screenshot
compare bila didukung (belum ada; Terukur tidak ada snapshot tests).

---

## 9. GPU/Renderer Stress

GPU fuzzing terpisah domain integrasi:
- repeated device/surface init.
- resize/reconfig.
- buffer/texture create/destroy.
- pipeline/shader compile failure.
- large-bounded requests.
- minimize/restore.
- device lost/surface lost.
- adapter failures.
- shutdown during pending work.
- fallback behavior.
- long deterministic render harness.
- track CPU/GPU proxies, handles, latency, errors.

Jangan fuzz arbitrary raw GPU commands tanpa model resource valid dan bounded.

---

## 10. Complexity/DoS

Target kompleksitas:
- parser recursion.
- symbol lookup repeated.
- semantic dependency traversal.
- graph algorithms (`graph.rs`).
- layout recalculation.
- state invalidation.
- cache invalidation.
- build dependency resolution.

Record input size, time, peak memory, allocation count, timeout/OOM outcomes,
growth pattern. Jangan ubah algoritma sebelum cause confirmed.

---

## 11. CI Tiers

Tier 1 PR:
- unit/integration.
- lint + clippy.
- small deterministic fuzz smoke.
- existing corpus.
- targeted memory/lifecycle tests.

Tier 2 nightly:
- longer coverage-guided fuzz.
- sanitizer builds.
- prop tests.
- extended lifecycle stress.
- selected renderer integration.

Tier 3 RC:
- extended campaigns.
- soak/profiling.
- clean build/install.
- GPU/backend matrix claimed platforms.
- full corpus.
- reproduce unresolved high-severity issues.

Budgets must follow actual measured hardware throughput (belum diukur;
Tidak terverifikasi).

---

## 12. Klasifikasi Bug & Severity

Severity P0-P3. P0: crash UB memory corruption security. P1: correctness/
leak/pathological. P2: performance/resource. P3: diagnostics/usability.
Findings require:
- original reproducer
- minimized repro
- regression test
- verified before/after
- fix commit evidence
- unaffected paths.

---

## 13. Limitasi Saat Ini (Terukur)

- Tidak ada harness, corpus, benchmark, snapshot tests.
- GPU tests butuh display (belum ditentukan headless support; Terukur tidak ada).
- LLVM backend butuh dependency sistem (Terukur).
- Runtime event loop butuh GPU/display (Terukur).
- CUDA default mati (Terukur).
- Leaky behavior global state di `state_store` (global `OnceLock`) membuat
  cross-test isolation/state reset challenge (Terukur).
- Build pipeline memerlukan libruntime dan subprocess isolation (Terukur).

---

## 14. Definition of Done — v1.0.0 Fuzz Gate

Belum terpenuhi; ini target. Release recommendation saat ini:
**TIDAK LAYAK** untuk mengeklaim fuzz-validated sampai implementasi, pengukuran,
dan regression tests selesai. Semua hasil harus dilabel measured/inferred/
unverified.

## 15. Catatan Metodologis

- Jangan konstruksi IR internal invalid kecuali target khusus.
- Untuk test input invalid, gunakan compile/source-level.
- Jangan ubah semantics agar fuzzer pass.
- Jangan mass-edit.
- Dokumentasikan trade-off sanitizers.
- Semua command HARUS deterministic dan dapat diulang.
