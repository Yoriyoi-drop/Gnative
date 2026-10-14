# Gnative

GUI framework native — tulis UI ala **TypeScript** (`.gn`), kompiler menghasilkan
**binary native** langsung. Rendering: **wgpu** (primary) + **CUDA** (fallback).

- 🖥️ `.gn` → **AST → Semantic IR → MIR → Cranelift (dev) / LLVM (release) → object → linker → binary native**.
- 🦀 Kompiler & runtime ditulis dalam Rust; **project developer 100% `.gn`** — tanpa C/Rust.
- ⌨️ Bahasa `.gn` meniru TypeScript: `window`, elemen deklaratif `<vstack />`, `<text />`, `<button />`, `<input />`.
- 🔄 Backend rendering abstrak: **wgpu** (utama) → **CUDA** (fallback) — di-link sebagai libruntime.
- ⚡ Incremental build: fingerprint + cache, hanya yang berubah yang dikompilasi ulang.

> Status: **milestone 0.9 — komponen + cross-compile (parsial)**. `.gn` → binary
> native end-to-end (window native wgpu): **fungsi + params/return**, `let`, `if`/`else`,
> `for (x of [...])`, perbandingan lengkap (`< <= > >= == !=`), `&&`/`||`, concat string
> literal, **komponen** (`<Badge label="x"/>` diekspansi compile-time), CLI gaya npm
> (`install`, `dev`, `list`...), cross-compile `--target <triple>` (contoh: `apps/hello`).

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

CUDA diaktifkan lewat fitur `cuda` — membutuhkan driver + toolkit CUDA:

```bash
cargo build -p gnative-runtime --features cuda
```

Tanpa fitur itu, backend CUDA nonaktif dan build tetap sukses di mesin mana pun.

## Dokumentasi

- [DESIGN.md](DESIGN.md) — arsitektur lengkap (3 dunia, bahasa .gn, pipeline, render engine).
- [container.md](container.md) — detail Gnative Container World (packages, cache, artifacts).
- [AGENTS.md](AGENTS.md) — panduan untuk agen AI yang mengerjakan codebase ini.

## Roadmap

| Milestone | Isi |
|---|---|
| 0.1–0.6 | Pondasi: workspace, core, bahasa .gn, layout runtime, komponen, event, state (pipeline lama `.gn → .rs`) |
| 0.7 | Kompiler native: `.gn → IR → MIR → Cranelift → object → linker → binary`; hapus generate Rust; libruntime wgpu/CUDA di-link (intrinsik C-ABI); window native nyata |
| **0.8** | **Bahasa lengkap (parsial): `function` + params/return, `let`, `if`/`else`, `for (x of [...])`, perbandingan lengkap, `&&`/`||`, concat string literal — MIR → Cranelift** |
| **0.9** | **Komponen + ekosistem CLI: `component` diekspansi compile-time; `install`/`dev`/`list`/`init`/`test`/`publish`; cross-compile `--target` (Cranelift multi-arch)** |
| 0.9 | Bahasa lengkap (lanjutan): `state` komponen (mutasi + re-render), `interface` penuh; backend LLVM release nyata |
| 1.0 | CUDA penuh: kernel rasterizer, blit framebuffer, parity wgpu |
| 1.1 | Ekosistem: registry package, hot reload, animasi, layout flex/grid, font atlas |
