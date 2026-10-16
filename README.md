# Gnative

GUI framework native — tulis UI ala **TypeScript** (`.gn`), kompiler menghasilkan
**binary native** langsung. Rendering: **wgpu** (primary) + **CUDA** (fallback).

- 🖥️ `.gn` → **AST → Semantic IR → MIR → Cranelift (dev) / LLVM (release) → object → linker → binary native**.
- 🦀 Kompiler & runtime ditulis dalam Rust; **project developer 100% `.gn`** — tanpa C/Rust.
- ⌨️ Bahasa `.gn` meniru TypeScript: `window`, elemen deklaratif `<vstack />`, `<text />`, `<button />`, `<input />`.
- 🔄 Backend rendering abstrak: **wgpu** (utama) → **CUDA** (fallback) — di-link sebagai libruntime.
- ⚡ Incremental build: fingerprint + cache, hanya yang berubah yang dikompilasi ulang.

> Status: **milestone 0.10 — bahasa lengkap + teks nyata**. `.gn` → binary
> native end-to-end (window native wgpu): **fungsi + params/return**, `let`, `if`/`else`,
> `for (x of [...])`, perbandingan lengkap (`< <= > >= == !=`), `&&`/`||`, `%`, concat string
> literal, **komponen** stateless (ekspansi) + stateful (multi-instance, props,
> `state`, `onClick` → rebuild), **`interface` penuh**, **teks nyata** (font atlas),
> backend release **LLVM 18**, CLI gaya npm, cross-compile `--target <triple>`.
>
> Rilis kandidat: **`v1.0.0-rc.1`** — seluruh temuan audit kesiapan rilis
> (`audit.md`: P0-01..09, P1-01..16, P2-01..14, P3-01..07) telah ditutup.
> Backend **CUDA dinyatakan kerangka / di luar cakupan v1.0** dan dipindah ke
> roadmap 1.1 (lihat [CHANGELOG.md](CHANGELOG.md) dan [SECURITY.md](SECURITY.md)).
> MSRV: **Rust 1.80**.

---

## Mulai Cepat

```bash
cargo build                          # build CLI `gnative`
cargo run -- help                    # daftar perintah
cargo run -- new demo && cd demo     # buat project .gn baru
cargo build -p gnative-runtime       # build libruntime (staticlib: window + wgpu/cuda)
gnative install                      # install package dari gnative.toml (gaya npm install)
gnative build                        # src/*.gn → container/artifacts/<nama> (binary native)
gnative run                          # build lalu buka jendela native (wgpu)
gnative dev                          # watch: rebuild + restart otomatis tiap perubahan (gaya npm run dev)
gnative build --target aarch64-unknown-linux-gnu   # cross-compile (Cranelift multi-arch)
gnative audit              # periksa kerentanan/cela keamanan library yang dipakai
gnative audit --live       # + basis data CVE langsung (cargo audit/cargo deny)
```

Perintah CLI gaya npm: `init` (alias `new`), `install`, `list`/`ls`,
`start` (alias `run`), `dev` (watch + auto-restart), `test` (alias `check`),
`publish` (roadmap). Jalankan `gnative help` untuk daftar lengkap.

Cross-compile `--target <triple>`: object target dihasilkan Cranelift
(murni Rust, tanpa toolkit); link butuh cross-toolchain
(`<triple>-gcc`/`-clang`/`-cc` di PATH) dan libruntime dibangun per-target
(`rustup target add <triple>` lalu `cargo build -p gnative-runtime --target <triple>`).

Contoh `src/main.gn` — fungsi nyata dikompilasi native:

```ts
use "@gnative/ui";

function status_label(count: number): string {
  if (count > 0) {
    return "aktif";
  }
  return "pasif";
}

function sum(): number {
  let total: number = 0;
  for (n of [1, 2, 3, 4, 5]) {
    total = total + n;
  }
  return total;
}

window App {
  title: "Halo Gnative";
  size: [800, 600];
  root: <vstack gap={16}>
    <text content={status_label(sum())} size={32} />
    <button label="Klik Saya" />
  </vstack>;
}
```

Hasil `gnative build`:

```
container/artifacts/
├── app.o          # object file hasil Cranelift
└── app            # binary native (app.o + libgnative_runtime.a)
```

## Ukuran artefak & strip

Binary native **menautkan libruntime secara statis** (wgpu + winit + winit/CRT),
sehingga tanpa strip ukurannya besar karena debug info libruntime ikut masuk.
Gunakan `--strip` (buang debug info) atau `--split-debug` (pindahkan debug info
ke `<binary>.debug` + `--add-gnu-debuglink`, tetap dapat di-debug dengan gdb):

```bash
gnative build --strip                  # buang debug info
gnative build --split-debug            # strip + simpan <binary>.debug
gnative build --release --strip        # backend LLVM 18 + ukuran minimal
```

Contoh terukur (contoh `vstack`+`text`, host x86_64-linux, `gnative build`,
libruntime debug) — hasil nyata pada Iterasi 20 audit:

| Artefak | Ukuran |
|---|---|
| `app` tanpa strip | **207.9 MB** (not stripped, with debug_info) |
| `app` setelah `--strip --split-debug` | **29.1 MB** (hemat 178.8 MB) |
| `app.debug` (debug info terpisah) | 198.3 MB |

Catatan jujur:

- `--release` kini memilih libruntime dari `target/release/` lebih dulu (dulu
  selalu memakai `target/debug/`). Bangun dulu:
  `cargo build --release -p gnative-runtime`.
- **Startup** belum diukur secara kuantitatif: menjalankan binary butuh
  GPU/display. Uji lokal hanya membuktikan binary stripped tetap boot ke event
  loop (log `[gnative] backend render: wgpu`, proses hidup sampai dihentikan).
- `--strip`/`--split-debug` memakai tool sistem `strip`/`objcopy`
  (atau `llvm-strip`/`llvm-objcopy`). Bila tool tidak ada, build **gagal dengan
  pesan jelas** — bukan sukses semu.

## Struktur

```
src/main.rs                 → hanya entry point (mendelegasikan ke CLI)
crates/
├── gnative-core/           → tipe dasar (geometri, warna, error) — zero-dep
├── gnative-lang/           → bahasa .gn: lexer, parser, AST, type checker
├── gnative-compiler/       → pipeline native: IR → MIR → codegen → linker
│   └── runtime/            → runtime .gn bawaan (@gnative/ui)
├── gnative-gfx/            → trait RenderBackend, Scene, FrameStats
├── gnative-gfx-wgpu/       → backend wgpu (PRIMARY)
├── gnative-gfx-cuda/       → backend CUDA (FALLBACK, fitur `cuda`)
├── gnative-runtime/        → winit, window, event loop, intrinsik C-ABI → libruntime
└── gnative-cli/            → perintah CLI
```

Pipeline kompiler:

```
.gn
  │  Gnative Parser
  ▼
Gnative AST
  │  Semantic lowering
  ▼
Semantic IR
  │  Lowering
  ▼
Gnative MIR
  ├──────────────┬──────────────┐
  │              │              │
Development    Release        (runtime .gn)
  │              │              │
Cranelift       LLVM          dikompilasi
  │              │              │
  ▼              ▼              ▼
Object File   Object File   Object File
  │              │              │
  └──────┬───────┴───────┬──────┘
         ▼               ▼
   libruntime      Linker (cc + libgnative_runtime.a)
   (wgpu/cuda)          │
                        ▼
                   Native Binary
```

## CUDA (fallback)

CUDA dinyatakan **kerangka / belum didukung** pada rilis ini — berada di luar
cakupan `v1.0.0`. `CudaRenderer::new()` selalu mengembalikan
`BackendUnavailable` dengan pesan jelas (bukan stub yang tampak berhasil).
Rendering nyata hanya lewat **wgpu**.

Fitur `cuda` tetap ada untuk pengembangan lanjutan dan membutuhkan driver +
toolkit CUDA:

```bash
cargo build -p gnative-runtime --features cuda
```

Tanpa fitur itu, build tetap sukses di mesin mana pun.

## Dokumentasi

- [DESIGN.md](DESIGN.md) — arsitektur lengkap (3 dunia, bahasa .gn, pipeline, render engine).
- [container.md](container.md) — detail Gnative Container World (packages, cache, artifacts).
- [AGENTS.md](AGENTS.md) — panduan untuk agen AI yang mengerjakan codebase ini.
- [CHANGELOG.md](CHANGELOG.md) — riwayat perubahan per rilis.
- [SECURITY.md](SECURITY.md) — pelaporan kerentanan, batas kepercayaan, kebijakan rollback.

## Keamanan dependency — `gnative audit`

`gnative audit` memeriksa apakah library yang dipakai punya cela keamanan,
sudah tak dipelihara, atau berasal dari sumber yang tidak tepercaya:

```bash
gnative audit                      # offline: inventaris + basis data lokal
gnative audit --live               # + basis data CVE (cargo audit/cargo deny)
gnative audit --live --deny-warnings   # peringatan "unmaintained" ikut menggagalkan
gnative audit --fail-on medium     # ambang gerbang lebih ketat (default: high)
gnative audit --db ci/advisories.toml  # basis data advisory kustom
```

Yang diperiksa:

1. **Inventaris persis** dari `Cargo.lock` (302 paket pada RC ini).
2. **Basis data advisory lokal** `ci/advisories.toml` (offline, deterministik,
   tanpa panggilan jaringan). Skemanya terdokumentasi di berkas itu.
3. **Pemeriksaan struktural**: dependency dari **luar crates.io**
   (git/registry alternatif = permukaan supply-chain) dan **versi ganda**
   crate yang sama (memperbesar permukaan serangan + ukuran binary).
4. **CVE langsung** dengan `--live` (memakai `cargo audit`/`cargo deny`).

Exit code non-0 bila ada temuan pada/di atas ambang `--fail-on`
(default `high`). Tingkat keparahan yang tak dikenal diperlakukan **konservatif**
setara `high`.

> Jujur: tanpa `--live`, kerentanan yang baru terbit (**zero-day**) **tidak**
> tercakup — basis data lokal hanya memuat entri yang diisi maintainer. Tool
> menyatakan hal ini secara eksplisit di akhir keluaran.

Lihat juga [SECURITY.md](SECURITY.md) untuk pelaporan kerentanan dan kebijakan
rollback.

## Lisensi & rilis

- Lisensi: **MIT OR Apache-2.0** (lihat [LICENSE-MIT](LICENSE-MIT) dan
  [LICENSE-APACHE](LICENSE-APACHE)).
- MSRV: **Rust 1.80** (`rust-version` di setiap crate).
- `Cargo.lock` **di-commit** agar build rilis reproducible.
- Checksum + provenance artefak rilis dibuat lokal (tanpa publikasi otomatis):

```bash
cargo build --release
cargo build --release -p gnative-runtime
ci/provenance.sh            # → dist/SHA256SUMS.txt + dist/PROVENANCE.txt
sha256sum -c dist/SHA256SUMS.txt
```

## Roadmap

| Milestone | Isi |
|---|---|
| 0.1–0.6 | Pondasi: workspace, core, bahasa .gn, layout runtime, komponen, event, state (pipeline lama `.gn → .rs`) |
| 0.7 | Kompiler native: `.gn → IR → MIR → Cranelift → object → linker → binary`; hapus generate Rust; libruntime wgpu/CUDA di-link (intrinsik C-ABI); window native nyata |
| **0.8** | **Bahasa lengkap (parsial): `function` + params/return, `let`, `if`/`else`, `for (x of [...])`, perbandingan lengkap, `&&`/`||`, concat string literal — MIR → Cranelift** |
| **0.9** | **Komponen + ekosistem CLI: `component` diekspansi compile-time; `install`/`dev`/`list`/`init`/`test`/`publish`; cross-compile `--target` (Cranelift multi-arch)** |
| 0.9 | Bahasa lengkap: `state` + event + multi-instance + props; `interface` penuh; backend LLVM 18 release nyata |
| 0.10 | Teks nyata: font atlas bitmap 5x7 ASCII ganti placeholder |
| **1.0.0-rc.1** | **Kandidat rilis: temuan audit P0/P1/P2/P3 ditutup, CI + lisensi + lock + changelog + provenance; CUDA dikeluarkan dari cakupan** |
| 1.1 | CUDA penuh (kernel rasterizer, blit framebuffer, parity wgpu); registry package, hot reload, animasi, layout flex/grid, font vektor |
