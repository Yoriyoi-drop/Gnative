# AGENTS.md — Panduan Agen AI untuk Codebase Gnative

Dokumen ini adalah kontrak kerja untuk agen AI (dan developer) yang mengerjakan repository ini.
Baca penuh sebelum mengubah apa pun. Detail arsitektur ada di [DESIGN.md](./DESIGN.md).

---

## 1. Identitas Proyek

- **Gnative**: GUI framework native. Developer menulis UI ala TypeScript (`.gn`) →
  **binary native** (bukan bahasa pemrograman native — DSL UI yang dikompilasi).
- Pipeline: `.gn` → AST → **Semantic IR** → **MIR** → **Cranelift** (dev) / **LLVM** (release)
  → object file → **linker** → binary native. **Tanpa generate kode Rust.**
- Engine: **Rust murni**; rendering **wgpu (primary)** + **CUDA (kerangka, belum
  bekerja — `CudaRenderer::new()` selalu `BackendUnavailable`)** —
  di-link ke binary sebagai libruntime (`libgnative_runtime.a`).
- Status: **milestone 0.10 + rilis kandidat `v1.0.0-rc.1`** (temuan audit
  `audit.md` P0/P1/P2/P3 sudah ditutup). MSRV **Rust 1.80**; lisensi
  **MIT OR Apache-2.0**; `Cargo.lock` di-commit.
- **CUDA di luar cakupan v1.0** (kerangka saja) — dipindah ke roadmap 1.1.

## 2. Aturan Emas (WAJIB)

1. **`src/` hanya berisi `main.rs`.** Jangan pernah menambah file lain di `src/`. Entry point hanya mendelegasikan ke `gnative_cli::run()`.
2. **Semua logika ada di `crates/`.** Crate baru boleh dibuat, tetapi logika wajib di dalam `crates/`.
3. **1 file = 1 tanggung jawab.** Satu file `.rs` melakukan satu hal yang jelas. Jika sebuah file mulai mengerjakan dua hal, pisahkan.
4. **Jangan mengedit artifact/output kompiler** (`container/artifacts/`, `container/cache/`). Semua ditulis ulang kompiler.
5. **`DESIGN.md` adalah dokumen hidup.** Setiap perubahan arsitektur yang material harus tercermin di sana (struktur repo, daftar file→tanggung jawab, pipeline).
6. **Dokumen & komentar ditulis dalam Bahasa Indonesia** (istilah teknis boleh bahasa Inggris).

## 3. Struktur yang Wajib Diingat

```
src/main.rs                  → hanya entry point
crates/
├── gnative-core/            → tipe dasar (tanpa dependency eksternal)
├── gnative-lang/            → lexer, parser, AST, checker bahasa .gn
├── gnative-compiler/        → pipeline native: config → resolve → cache →
│   │                          IR → MIR → codegen (cranelift/llvm) → linker
│   └── runtime/             → runtime .gn bawaan (@gnative/ui) → di-seed ke container
├── gnative-gfx/             → trait RenderBackend, Scene, FrameStats
├── gnative-gfx-wgpu/        → backend wgpu (PRIMARY)
├── gnative-gfx-cuda/        → backend CUDA (FALLBACK, fitur "cuda")
├── gnative-runtime/         → winit, window, event loop, intrinsik C-ABI → staticlib
└── gnative-cli/             → perintah CLI
```

Dependensi: `core` adalah fondasi; `gfx` hanya bergantung `core`; `gfx-wgpu`/`gfx-cuda` → `gfx`;
`runtime` → `gfx-wgpu` + `gfx-cuda`. **Jangan buat dependensi siklik.**
`gnative-cli` hanya bergantung `gnative-compiler` — libruntime **tidak** di-dep oleh CLI;
ia di-build terpisah (`cargo build -p gnative-runtime`) lalu di-link oleh `linker.rs`.

## 4. Konvensi Kode

- Nama: file & fungsi `snake_case`; struct/enum/trait `UpperCamelCase`; konstanta `SCREAMING_SNAKE_CASE`.
- Setiap item `pub` wajib punya doc comment `///` (Bahasa Indonesia, ringkas).
- Error: gunakan `GnError` dari `gnative-core`. Tidak ada `unwrap()`/`panic!` di jalur publik — kembalikan `Result`.
- `gnative-core` harus tetap **tanpa dependency eksternal** (0-dep). Jika membutuhkan crate, diskusikan dulu.
- `gnative-gfx` juga dijaga ringan (hanya `gnative-core`).
- Dependency GPU (wgpu, winit) hanya di `gnative-gfx-wgpu` dan `gnative-runtime`. Jangan bocorkan `wgpu`/`winit` types ke crate lain.

## 5. Fitur (Feature Flags)

- Fitur `cuda` menghidupkan backend CUDA: `cargo build -p gnative-runtime --features cuda`
  (chain: root → gnative-cli → gnative-compiler; libruntime CUDA di-build sendiri).
- **Default build harus selalu sukses tanpa CUDA toolkit.** Kode CUDA di-gate `#[cfg(feature = "cuda")]`;
  jika fitur mati, `CudaRenderer::new()` mengembalikan `GnError::BackendUnavailable`.
- wgpu diaktifkan dengan fitur `wgsl` + `winit` + backend native (vulkan/metal/dx12/gles).

## 6. Perintah Verifikasi

```bash
cargo check --workspace            # wajib hijau setelah setiap perubahan
cargo test --workspace             # unit test (tanpa GPU)
cargo fmt --check                  # format konsisten
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p gnative-runtime     # build libruntime (staticlib) — diperlukan untuk link
cargo run -- help                  # daftar perintah CLI
cargo run -- new demo              # buat project .gn contoh
cargo run -- build                 # kompilasi project .gn → binary native
cargo run -- build --strip         # + buang debug info (ukuran turun drastis)
cargo run -- build --split-debug   # + simpan debug ke <binary>.debug (debuglink)
cargo run -- run                   # buka jendela native (butuh GPU/display)
cargo run -- audit                 # periksa kerentanan library (offline)
cargo run -- audit --live          # + basis data CVE (cargo audit/cargo deny)
ci/provenance.sh                   # checksum + provenance artefak (lokal, tanpa publish)
```

> Catatan: compile pertama `cargo check` memakan waktu karena wgpu/winit/cranelift
> (sekitar 1–5 menit). Gunakan `--locked` (`cargo check --workspace --locked`)
> untuk menegakkan `Cargo.lock` yang di-commit.

## 7. Alur Kerja yang Disarankan untuk Agen

1. Baca `DESIGN.md` dan file crate yang relevan sebelum mengedit.
2. Ikuti prinsip 1 file = 1 tanggung jawab; jika perlu fungsi baru, cek apakah sudah ada di crate lain (jangan duplikasi).
3. Setelah mengubah arsitektur → perbarui `DESIGN.md` (bagian 4 dan 10).
4. Jalankan `cargo check --workspace` dan `cargo test --workspace`.
5. Jangan menambah dependency eksternal tanpa kebutuhan yang jelas; prefer std + crate yang sudah ada.

## 8. Jebakan yang Diketahui

- **Jangan pakai API wgpu yang sudah deprecated** — cek versi di `crates/gnative-gfx-wgpu/Cargo.toml` (wgpu 30).
- **`request_adapter`/`request_device` bersifat async** — gunakan `pollster::block_on` (sudah dipakai di `device.rs`).
- **Winit 0.30 memakai `ApplicationHandler`** (bukan `EventLoop::run` callback lama) — lihat `crates/gnative-runtime/src/event_loop.rs`.
- **`Window` winit 0.30 tidak `Clone`** — surface wgpu memakai `Arc<Window>` agar `'static`.
- **Cranelift dipecah beberapa crate** — `FunctionBuilder` dari `cranelift-frontend`;
  `Module`/`ObjectBuilder` dari `cranelift-module`/`cranelift-object`; lihat `codegen/cranelift.rs`.
- **CUDA tidak bisa dikompilasi tanpa toolkit** — selalu test dengan fitur `cuda` NONAKTIF terlebih dahulu.
- **Linker butuh libruntime** — `gnative build` mencari `libgnative_runtime.a` ke atas dari project
  (hingga repo root `target/debug`). Tanpa libruntime, build **gagal-jujur** (exit non-0 + pesan
  cara memperbaiki) — bukan sukses semu (audit P1-12). Set `GNATIVE_REQUIRE_RUNTIME=1`
  (dipakai CI) untuk menjadikannya prasyarat keras juga di jalur `check`/test.
- **Jangan menaruh logika di `main.rs`** — hanya delegasi.
- **Basis data advisory tidak boleh dikarang.** `ci/advisories.toml` hanya
  boleh diisi entri dengan ID + rujukan yang dapat diverifikasi (mis. RUSTSEC).
  Pemeriksaan CVE langsung dilakukan lewat `gnative audit --live`; jangan
  menyalin data CVE tanpa sumber.

## 9. Komunikasi

- Laporan singkat dalam Bahasa Indonesia.
- Sebutkan file yang diubah dan alasan (terkait tanggung jawab file).
- Jika menemukan desain yang tidak konsisten dengan DESIGN.md, perbaiki atau tandai dengan jelas.
