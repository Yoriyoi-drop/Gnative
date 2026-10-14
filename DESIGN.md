# DESIGN.md — Gnative

> **Gnative** — GUI framework native. Developer menulis UI ala TypeScript (`.gn`),
> kompiler menghasilkan **binary native** langsung (bukan bahasa pemrograman native,
> melainkan DSL UI → aplikasi GUI native). Rendering: **wgpu** (primary) / **CUDA** (fallback).
> Aturan emas: **1 file = 1 tanggung jawab**, **semua logika di `crates/`**,
> **`src/` hanya berisi `main.rs`**, **project developer 100% `.gn` (tanpa C/Rust)**.

---

## 1. Ringkasan

Gnative adalah platform untuk membangun aplikasi GUI native:

1. Developer menulis UI dalam file `.gn` — sintaks meniru TypeScript
   (`interface`, `component`, `window`, elemen deklaratif `<Button />`).
2. Kompiler Gnative mengubah `.gn` menjadi **object file native**
   (dev: **Cranelift**; release: **LLVM**) — **tanpa generate kode Rust**.
3. **Linker** menggabungkan object file `.gn` + **libruntime**
   (`libgnative_runtime.a`, berisi window native + render **wgpu** / **CUDA**)
   menjadi **binary native**.
4. Saat dijalankan, binary membuka **window native** yang dirender
   **wgpu** (Vulkan/Metal/DX12). Jika wgpu tidak tersedia, engine jatuh ke
   **CUDA** (rasterizer GPU NVIDIA).

Pipeline baru:

```
.gn
  │ Gnative Parser
  ▼
Gnative AST
  │ Semantic lowering
  ▼
Semantic IR
  │ Lowering
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
         │               │
         ▼               ▼
   libruntime      Linker (cc + libgnative_runtime.a)
   (wgpu/cuda)          │
                        ▼
                   Native Binary
```

---

## 2. Prinsip Desain

| Prinsip | Arti |
|---|---|
| **Native compiler** | `.gn` → IR → MIR → Cranelift/LLVM → object → linker. **Tidak ada** generate kode Rust; Cargo/rustc tidak terlibat dalam pipeline project. |
| **1 file = 1 tanggung jawab** | Setiap file `.rs` melakukan satu hal yang jelas dan diberi nama sesuai hal itu. |
| **Semua logika di `crates/`** | `crates/*` adalah rumah seluruh implementasi. |
| **`src/` hanya `main.rs`** | Entry point root hanya mendelegasikan ke `gnative_cli`. |
| **Bahasa `.gn` ala TypeScript** | Familiar bagi developer TS/JS; dikompilasi, bukan diinterpretasi. |
| **Project 100% `.gn`** | Developer tidak pernah menulis C/Rust di project; libruntime dibundel saat link. |
| **Backend rendering dapat diganti** | Abstraksi `RenderBackend`; wgpu = primary, CUDA = fallback. |
| **Incremental & cache** | Fingerprint + cache → hanya regenerate yang berubah. |

---

## 3. Tiga Dunia (diperbarui)

```
┌──────────────────────────────┐
│  1. DEVELOPER WORLD          │  src/*.gn, gnative.toml
└──────────────┬───────────────┘
               │ gnative build
               ▼
┌──────────────────────────────┐
│  2. GNATIVE CONTAINER WORLD  │  container/{packages, cache, artifacts}
│                              │  runtime .gn bawaan (@gnative/ui) di-seed
└──────────────┬───────────────┘
               │ object file (.gn) + libruntime (wgpu/cuda) via linker
               ▼
┌──────────────────────────────┐
│  3. NATIVE BUILD WORLD       │  cc/ld + libgnative_runtime.a → binary
└──────────────────────────────┘
```

- **Dunia 1 → 2** ditangani `gnative-compiler` (config, resolve, cache, IR/MIR).
- **Dunia 2 → 3** ditangani `linker.rs`: object `.gn` + libruntime → **binary native**.
- **Libruntime** (`crates/gnative-runtime`) dibangun terpisah dengan Cargo
  (`cargo build -p gnative-runtime`) menjadi **staticlib** — window + render
  wgpu/CUDA. Binary `.gn` memanggil intrinsik C-ABI (`gn_window_create`,
  `gn_window_run`) dari libruntime.

---

## 4. Struktur Repository

```
Gnative/
├── Cargo.toml              # workspace root + package root (bin "gnative")
├── DESIGN.md / AGENTS.md / README.md / container.md
├── src/
│   └── main.rs             # SATU-SATUNYA file di src/ — memanggil gnative_cli
└── crates/                 # SEMUA logika
    ├── gnative-core/       # tipe dasar: geometri, warna, unit, error — zero-dep
    ├── gnative-lang/       # bahasa .gn: lexer, parser, AST, type checker
    ├── gnative-compiler/   # pipeline native: config → resolve → cache →
    │                       #   IR → MIR → codegen (cranelift/llvm) → linker
    │   └── runtime/        # runtime .gn bawaan (@gnative/ui) → di-seed ke container
    ├── gnative-gfx/        # abstraksi render: trait RenderBackend, Scene, FrameStats
    ├── gnative-gfx-wgpu/   # backend wgpu (PRIMARY)
    ├── gnative-gfx-cuda/   # backend CUDA (FALLBACK, feature-gated)
    ├── gnative-runtime/    # winit + event loop + backend select + intrinsik C-ABI
    └── gnative-cli/        # perintah CLI: new/build/run/check/clean/graph/container/doctor
```

### Dependensi antar-crate

```
gnative-cli
└── gnative-compiler
    ├── gnative-lang
    └── gnative-core

gnative-runtime
├── gnative-gfx-wgpu ── gnative-gfx ── gnative-core
├── gnative-gfx-cuda ── gnative-gfx
└── gnative-core
```

`gnative-runtime` tidak di-dep oleh `gnative-cli` — libruntime di-build
terpisah menjadi staticlib, lalu di-link oleh `linker.rs` bersama object `.gn`.

---

## 5. Bahasa `.gn`

### 5.1 Filosofi

`.gn` terlihat seperti TypeScript yang dipangkas untuk kebutuhan GUI. Tidak ada
`DOM`; ada **komponen**, **window**, **tipe**, dan **elemen deklaratif**
(`<Button label={...} />`). `.gn` bukan bahasa pemrograman native — ini **DSL
UI** yang dikompilasi menjadi aplikasi GUI native.

### 5.2 Contoh program

```ts
// src/main.gn
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
  title: "Gnative";
  size: [800, 600];
  root: <vstack gap={16}>
    <text content={status_label(sum())} size={32} />
    <button label="Klik Saya" />
  </vstack>;
}
```

### 5.3 Sintaks yang didukung (subset TS)

| Fitur | Contoh |
|---|---|
| `use` / import | `use "@gnative/ui";` |
| `function` | `function f(a: number): string { ... }` — params + return |
| `let` | `let total: number = 0;` |
| `if`/`else` | `if (count > 0) { ... } else { ... }` |
| `for` | `for (n of [1, 2, 3]) { ... }` — array literal, di-unroll |
| `return` | `return expr;` / `return;` |
| Ekspresi | aritmatika `+ - * /`, perbandingan `< <= > >= == !=`, `!`, `&&`/`||` (short-circuit), concat string literal `"a" + "b"` |
| Assignment | `x = v`, `x += v`, `x -= v` (variabel lokal) |
| `window` | `window App { title: ...; size: [w, h]; root: <...>; }` |
| `component` | `component Badge { render(): Element { return <text content={props.label}/>; } }` — **diekspansi compile-time** |
| `state` | `state { count: number = 0; }` di komponen — per instance, mutasi + re-render via event |
| `props` stateful | `<Counter judul="satu" />` — disubstitusi per instance (seperti stateless) |
| `onClick` | `onClick={this.method}` pada `<button>` di `render()` stateful — klik panggil method + rebuild + redraw |
| Elemen deklaratif | `<vstack gap={8}> <text .../> </vstack>` + komponen user `<Badge label="x"/>` |
| Atribut elemen | `content={expr}`, `size={n}`, `label={expr}`, `gap`, `placeholder` |
| Komentar | `// komentar` |

> Milestone 0.9: subset yang dikompilasi ke native = `window` + elemen
> deklaratif + komponen stateless (ekspansi compile-time, props disubstitusi)
> + komponen stateful (multi-instance, props per instance, `state` + `onClick`
> → dispatch + rebuild + redraw) + `function`/`let`/`if`/`for`/ekspresi.
> Catatan: `<` dipakai membuka elemen deklaratif dan operator perbandingan
> (disambiguasi parser: `<` operator hanya bila left operand bukan elemen).

### 5.4 Sistem tipe

- Primitif: `string`, `number`, `boolean`, `void`, `null`.
- `interface` penuh: field wajib/opsional (`?`), union literal
  (`"a" | "b"`), cek duplikat field. `component B(props: Props)` —
  pemakaian `<B .../>` divalidasi: atribut asing (warning), field wajib
  hilang (error), literal salah tipe (error). Nilai non-literal
  (panggilan, variabel) dilewati — inferensi penuh = roadmap.
- `window` → entry point aplikasi (title, size, root).
- Elemen: `<vstack>`, `<text>`, `<button>`, `<input>` (dari package `@gnative/ui`).

### 5.5 Aliran data

- Satu `window` per aplikasi; `root:` adalah scene UI deklaratif.
- Runtime `.gn` bawaan (`crates/gnative-compiler/runtime/@gnative/ui/ui.gn`)
  mendefinisikan elemen UI yang dikompilasi pipeline bersama source project.

---

## 6. Pipeline Compiler

Kode: `crates/gnative-lang` + `crates/gnative-compiler`.

```
src/*.gn  (+ container/packages/@gnative/ui dari runtime .gn bawaan)
   │  lexer (gnative-lang::lexer)
   ▼
Token stream
   │  parser (gnative-lang::parser)
   ▼
AST (gnative-lang::ast)
   │  checker (gnative-lang::checker)
   ▼
Module ter-validasi
   │  lower (gnative-compiler::lower) — AST → Semantic IR
   ▼
Semantic IR (ir.rs)
   │  mir_lower (gnative-compiler::mir_lower) — IR → MIR
   ▼
Gnative MIR (mir.rs)
   │  codegen (gnative-compiler::codegen)
   │    ├─ cranelift.rs  → dev build → object file (ELF)
   │    └─ llvm.rs       → release build (stub/roadmap) → object file
   ▼
Object File (container/artifacts/<name>.o)
   │  linker (gnative-compiler::linker)
   │    cc <app.o> libgnative_runtime.a -o <binary>
   ▼
Native Binary (container/artifacts/<name>)
```

### 6.1 Tahapan kode

| File | Tanggung jawab |
|---|---|
| `config.rs` | parse `gnative.toml` (nama project, deps) |
| `resolve.rs` | resolusi `use "@gnative/ui"` → package di `container/packages` |
| `fingerprint.rs` | hash deterministik (FNV-1a) per source |
| `cache.rs` | cache incremental (hit/miss per file) |
| `ir.rs` | Semantic IR: `IrModule`, `IrWindow`, `IrFunc`, `IrComponent` (state + method), `ComponentInstance` (nomor + props + render per situs), `instance_key` |
| `lower.rs` | AST → Semantic IR (stateless → ekspansi; stateful → definisi + `ComponentInstance` per situs, props disubstitusi, penjaga siklus) |
| `mir.rs` | MIR: `MirModule`, `MirFunc`, `MirInst` (`Ptr`, `FuncAddr`, `CallIndirect`) |
| `mir_lower.rs` | IR → MIR (window → build `__gn_build_scene` + `main` dinamis; string → pool; state per instance → `gn_state_*`; method tersalin `Comp__i__m`; klik → `gn_scene_add_button_clickable` + `gn_register_method`) |
| `codegen/mod.rs` | dispatcher backend (Cranelift dev / LLVM release) |
| `codegen/cranelift.rs` | MIR → object ELF via cranelift-codegen + cranelift-object (termasuk import `gn_state_*`/`gn_register_method`/`gn_window_run_dynamic`, `FuncAddr` + `CallIndirect`) |
| `codegen/llvm.rs` | MIR → object via inkwell/LLVM 18 (optimasi agresif, multi-target): slot → alloca entry, CFG → basic block |
| `linker.rs` | object `.gn` + libruntime → binary native via `cc` |
| `pipeline.rs` | orkestrasi seluruh build (seed runtime, parse, IR/MIR, codegen, link) |

### 6.2 MIR → intrinsik runtime (C-ABI)

Binary hasil kompiler memanggil intrinsik dari **libruntime**
(`crates/gnative-runtime/src/intrinsics.rs`):

```
main (dari .gn)
  ├─ gn_scene_create() → scene_handle
  ├─ gn_scene_add_text(handle, ptr, size)        (untuk <text>)
  ├─ gn_scene_add_button(handle, ptr)           (untuk <button>)
  ├─ gn_scene_add_input(handle, ptr)            (untuk <input>)
  ├─ gn_scene_begin_vstack(handle, gap) / gn_scene_end_vstack(handle)
  ├─ gn_window_create(title_ptr, title_len, w, h) → handle
  └─ gn_window_run_scene(scene_handle) → i32
```

- `gn_scene_create` … `gn_scene_end_vstack` — rakit `Scene` dari pohon UI
  deklaratif (`IrNode`) yang di-lower di `mir_lower::build_scene`. Runtime
  menumpuk node ke kontainer, lalu `gn_window_run_scene` mengambilnya,
  menghitung layout (`Scene::layout()` di `gnative-gfx`), dan merendernya.
- `gn_window_create` — simpan konfigurasi window (title, ukuran).
- `gn_window_run_scene` — buka window native + event loop + render Scene
  (blocking). `gn_window_run` (lama) tetap ada sebagai fallback placeholder.
- State komponen (`state_store.rs`): `gn_state_set_f64/get_f64`,
  `gn_state_set_bool/get_bool`, `gn_state_set_str/get_str` —
  kunci `(kunci instance, field)` mis. `("Counter#0", "count")`, satu state
  per situs pemakaian. `main` meng-init state (`set_*`), render instance
  yang di-inline membaca via `get_*`, salinan method `Comp__i__m`
  memutasi via `set_*`.
- Event klik (event-driven): `main` mendaftarkan method
  (`gn_register_method(comp, method, fptr)`) lalu
  `gn_window_run_dynamic(build)` — runtime build awal via `__gn_build_scene`,
  klik tombol (`gn_scene_add_button_clickable` → `Node::Action`) memicu
  hit-test (`Scene::action_at`) → panggil method → `rebuild_scene`
  (build ulang + layout) → redraw. Handler harus tanpa parameter.
- Codegen: `MirInst::Call` ke symbol tersebut; `gn_window_create`/`WindowRun`
  lewat `MirInst::WindowCreate`/`WindowRun`; alamat fungsi lewat
  `MirInst::FuncAddr` (diregistrasi / dipanggil tak-langsung).

> Milestone 0.8+: pohon UI `.gn` **benar-benar di-render** (bukan lagi
> `default_scene` hardcode). Teks dirender sebagai quad placeholder —
> glyph nyata menunggu atlas font (§13, 1.1).

### 6.3 `container/` (Gnative Container World)

```
container/
├── packages/      # source .gn dependency (@gnative/ui — di-seed dari runtime bawaan)
├── cache/         # fingerprint incremental
└── artifacts/     # <name>.o (object Cranelift) + <name> (binary native hasil link)
```

- `container/generated/` **tidak ada lagi** — tidak ada generate Rust.
- `container/artifacts/` menggantikan peran `target/` Cargo untuk project `.gn`.

### 6.4 Incremental build

- Fingerprint FNV-1a deterministik per file; `cache.rs` menyimpan hash terakhir.
- File tidak berubah → cache hit; hanya yang berubah yang di-regenerate.
- `cache_key` dipisahkan dari nama file agar path runtime (di luar `src/`) konsisten.

---

## 7. Engine Rendering

Kode: `crates/gnative-gfx` (+ `gnative-gfx-wgpu`, `gnative-gfx-cuda`).

### 7.1 Abstraksi `RenderBackend`

`crates/gnative-gfx/src/backend.rs`:

```rust
pub trait RenderBackend {
    fn kind(&self) -> BackendKind;          // Wgpu | Cuda
    fn name(&self) -> &'static str;
    fn resize(&mut self, size: Size) -> GnResult<()>;
    fn render_scene(&mut self, scene: &Scene) -> GnResult<FrameStats>;
}
```

`gnative-gfx` tidak tahu API GPU — hanya tahu `Scene` dan `FrameStats`.

### 7.2 Pemilihan backend (fallback chain)

`crates/gnative-runtime/src/backend_select.rs`:

```
1. WgpuRenderer::new(window)   ── PRIMARY  (Vulkan / Metal / DX12 / GLES)
        │ gagal (no adapter, no surface)
        ▼
2. CudaRenderer::new()         ── FALLBACK (rasterizer komputasi GPU NVIDIA)
        │ gagal (tidak ada driver CUDA / feature nonaktif)
        ▼
3. Err(BackendUnavailable)     → pesan yang jelas kepada user
```

- wgpu dipilih lebih dulu: akselerasi penuh + komposisi window native.
- CUDA = fallback komputasi jika GPU NVIDIA tanpa stack Vulkan yang layak.

### 7.3 Scene Graph

```rust
pub struct Scene { pub size: Size, pub clear: Color, pub nodes: Vec<Node> }

pub enum Node {
    Quad(Quad),        // persegi + warna + radius sudut
    Text(TextItem),    // teks + ukuran font
    Group(Vec<Node>),  // kumpulan node
    Stack(StackNode),  // susunan flex
    Action { id: usize, inner: Box<Node> }, // pembungkus klik (hit-test → dispatch)
}
```

### 7.4 Backend wgpu (PRIMARY) — `crates/gnative-gfx-wgpu`

| File | Tanggung jawab |
|---|---|
| `device.rs` | `Instance` → `Adapter` → `Device` + `Queue` (via `pollster::block_on`) |
| `surface.rs` | `Surface<'static>` (dari `Arc<Window>` winit 0.30) + config sRGB/Fifo |
| `vertex.rs` | `Vertex { position: [f32;2], color: [f32;4], uv: [f32;2] }` + layout |
| `shader.rs` | shader WGSL inline (vertex + fragment sample atlas, alpha blending) |
| `atlas.rs` | font bitmap 5x7 ASCII + tekstur atlas + UV per glyph |
| `renderer.rs` | `WgpuRenderer`: `&Scene` → glyph/quad buffer → render pass → present |

- API wgpu 30: `request_adapter` async + `Result`; `create_render_pipeline`
  mengembalikan pipeline langsung (bukan `Result`); `Queue::present`.
- Frame: `get_current_texture` → `create_view` → `begin_render_pass` →
  `submit` → `present`.

### 7.5 Backend CUDA (FALLBACK) — `crates/gnative-gfx-cuda`

- Di-gate fitur `cuda`. Libruntime CUDA: `cargo build -p gnative-runtime --features cuda`.
- Konsep: scene → daftar primitive → kernel CUDA rasterisasi (scanline) →
  framebuffer → blit ke jendela.
- Jika fitur mati, `CudaRenderer::new()` mengembalikan `BackendUnavailable`.

---

## 8. Runtime — `crates/gnative-runtime`

| File | Tanggung jawab |
|---|---|
| `event_loop.rs` | winit 0.30 `ApplicationHandler`: `resumed` → window + backend; `window_event` → resize/close/redraw |
| `backend_select.rs` | fallback chain wgpu → cuda |
| `intrinsics.rs` | C-ABI `gn_window_*` + `gn_scene_*` + `gn_register_method`/`gn_window_run_dynamic` (dispatch klik + rebuild) |
| `state_store.rs` | C-ABI `gn_state_set/get_*` (f64/bool/str): penyimpanan state komponen |

- Crate-type `["rlib", "staticlib"]` → `libgnative_runtime.a` di-link oleh `linker.rs`.
- Window dibuat dari event loop (`event_loop.create_window`); winit 0.30 `Window`
  tidak `Clone`, jadi dipakai `Arc<Window>` untuk surface wgpu `'static`.

---

## 9. CLI — `crates/gnative-cli`

| Perintah | Fungsi |
|---|---|
| `new <nama>` | buat project `.gn` baru (gnative.toml + src/main.gn) |
| `build` | pipeline: `.gn` → object → link → binary native |
| `run` | build lalu jalankan binary (buka window native) |
| `check` | validasi project (parse + check semua `.gn`) |
| `clean` | hapus `container/artifacts` + cache |
| `graph` | tampilkan dep graph project |
| `container` | kelola container (list/inspect/clean) |
| `doctor` | cek lingkungan (linker, libruntime, toolchain) |
| `help` | daftar perintah |

Perintah gaya npm (analog):

| Perintah | Analog npm | Fungsi |
|---|---|---|
| `init <nama>` | `npm init` | alias `new` — buat project baru |
| `install` | `npm install` | seed package dari `gnative.toml` ke `container/packages` (bawaan compiler; lainnya roadmap registry) |
| `list` / `ls` | `npm list` | tampilkan dependencies + status terpasang |
| `start` | `npm start` | alias `run` — build lalu jalankan |
| `dev` | `npm run dev` | watch `src/` + `container/packages/`: rebuild + restart binary otomatis tiap perubahan |
| `test` | `npm test` | alias `check` — validasi semua `.gn` |
| `publish` | `npm publish` | publish ke registry (roadmap 1.1 — stub) |

Opsi lintas-target: `build --target <triple>` / `run --target <triple>`
(mis. `aarch64-unknown-linux-gnu`). Cranelift multi-arch murni Rust —
object target dihasilkan tanpa toolkit; linker memakai cross-toolchain
(`<triple>-cc`/`-clang`/`-gcc`); libruntime di-build per-target
(`rustup target add ...` + `cargo build -p gnative-runtime --target ...`).

Perintah `generate` **dihapus** — tidak ada lagi generate kode Rust.

---

## 10. Aturan "1 File = 1 Tanggung Jawab"

Daftar file → tanggung jawab (kondisi saat ini).

```
src/main.rs                       → entry point; hanya memanggil gnative_cli

crates/gnative-core/src/error.rs  → GnError + Result
crates/gnative-core/src/color.rs  → Color (RGBA f32)
crates/gnative-core/src/vec.rs    → Vec2
crates/gnative-core/src/rect.rs   → Rect
crates/gnative-core/src/size.rs   → Size
crates/gnative-core/src/units.rs  → satuan ukuran
crates/gnative-core/src/event.rs  → Event input
crates/gnative-core/src/lib.rs    → re-export tipe dasar

crates/gnative-lang/src/token.rs      → Token + TokenKind
crates/gnative-lang/src/lexer.rs      → teks → token
crates/gnative-lang/src/ast.rs        → struktur AST (.gn)
crates/gnative-lang/src/parser.rs     → token → AST
crates/gnative-lang/src/checker.rs    → validasi semantik + tipe (interface penuh: props per pemakaian)
crates/gnative-lang/src/ty.rs         → representasi tipe
crates/gnative-lang/src/span.rs       → Span + LineColumn
crates/gnative-lang/src/diagnostic.rs → pesan diagnostik

crates/gnative-compiler/src/config.rs     → parse gnative.toml
crates/gnative-compiler/src/resolve.rs    → resolusi use → package
crates/gnative-compiler/src/install.rs    → install package (analog npm install)
crates/gnative-compiler/src/graph.rs      → dep graph + cycle detection
crates/gnative-compiler/src/fingerprint.rs→ hash deterministik
crates/gnative-compiler/src/cache.rs      → cache incremental
crates/gnative-compiler/src/ir.rs         → Semantic IR
crates/gnative-compiler/src/lower.rs      → AST → IR
crates/gnative-compiler/src/mir.rs        → MIR
crates/gnative-compiler/src/mir_lower.rs  → IR → MIR
crates/gnative-compiler/src/codegen/mod.rs      → dispatcher backend (+ target)
crates/gnative-compiler/src/codegen/cranelift.rs→ MIR → object (dev, multi-target via `lookup_by_name`)
crates/gnative-compiler/src/codegen/llvm.rs     → MIR → object (release, stub)
crates/gnative-compiler/src/linker.rs     → object + libruntime → binary
crates/gnative-compiler/src/pipeline.rs   → orkestrasi build (+ `validate` untuk check)
crates/gnative-compiler/runtime/@gnative/ui/ui.gn → runtime UI bawaan (.gn)

crates/gnative-gfx/src/backend.rs → trait RenderBackend + BackendKind
crates/gnative-gfx/src/scene.rs   → Scene + Node + primitive
crates/gnative-gfx/src/frame.rs   → FrameStats

crates/gnative-gfx-wgpu/src/device.rs   → Instance/Adapter/Device/Queue
crates/gnative-gfx-wgpu/src/surface.rs  → Surface + config
crates/gnative-gfx-wgpu/src/vertex.rs   → Vertex + layout (+ UV)
crates/gnative-gfx-wgpu/src/shader.rs   → WGSL source (+ sample atlas)
crates/gnative-gfx-wgpu/src/atlas.rs    → font 5x7 + tekstur atlas + UV
crates/gnative-gfx-wgpu/src/renderer.rs → implementasi RenderBackend

crates/gnative-gfx-cuda/src/lib.rs      → re-export
crates/gnative-gfx-cuda/src/renderer.rs → implementasi RenderBackend (CUDA)

crates/gnative-runtime/src/lib.rs           → init + re-export
crates/gnative-runtime/src/event_loop.rs    → ApplicationHandler (winit 0.30)
crates/gnative-runtime/src/backend_select.rs→ fallback chain wgpu→cuda
crates/gnative-runtime/src/intrinsics.rs    → C-ABI gn_window_* + gn_scene_*
crates/gnative-runtime/src/state_store.rs   → C-ABI gn_state_set/get_* (state komponen)

crates/gnative-cli/src/lib.rs        → run() CLI
crates/gnative-cli/src/cmd/mod.rs    → dispatch + help
crates/gnative-cli/src/cmd/*.rs      → satu perintah per file
  (new, build, run, check, clean, graph, doctor, container,
   init→new, install, list/ls, start→run, dev, test→check, publish)

apps/hello/                        → contoh project .gn (window + vstack/text/button/input + Counter stateful 2 instance + props)
apps/hello/container/artifacts/    → hello.o + binary native hasil build
```

---

## 11. Konvensi Kode

- Nama file `snake_case`, satu tanggung jawab per file.
- Struct `UpperCamelCase`, fungsi `snake_case`, enum `UpperCamelCase`.
- Semua item `pub` diberi doc comment (`///`) — Bahasa Indonesia.
- Error memakai `GnError` dari `gnative-core`; jangan `unwrap()` di jalur publik.
- `gnative-core` tetap **zero-dependency**.
- Dependency GPU (wgpu, winit) hanya di `gnative-gfx-wgpu` dan `gnative-runtime`.
- CUDA di-gate fitur `cuda`; default build tetap sukses tanpa CUDA toolkit.
- Dokumen ditulis dalam Bahasa Indonesia.

---

## 12. Pengujian

- Unit test di setiap crate (modul `#[cfg(test)]`).
- `cargo check --workspace` / `cargo test --workspace` — test ringan tanpa GPU.
- Test end-to-end compiler: `.gn` → object ELF (magic `\x7fELF`) + link.
- Pengujian render butuh GPU/display → manual (`gnative run`).
- `cargo fmt --check` dan `cargo clippy --workspace -- -D warnings`.

---

## 13. Roadmap

| Milestone | Isi |
|---|---|
| **0.1 — Pondasi** | Workspace, core types, bahasa .gn (lexer/parser/checker), CLI, runtime winit, renderer wgpu (quad), backend CUDA (kerangka). |
| **0.2–0.6** | Layout runtime, generated crate (lama: `.gn → .rs`), komponen, event, state. |
| 0.7 — Kompiler native | Pipeline `.gn → IR → MIR → Cranelift → object → linker → binary native`; **hapus generate Rust**; libruntime wgpu/CUDA di-link (intrinsik C-ABI); window native nyata; project 100% `.gn`. |
| **0.8 — Bahasa lengkap (parsial, saat ini)** | `function` + params/return, `let`, `if`/`else`, `for (x of [...])`, ekspresi aritmatika & perbandingan (`< <= > >= == !=`), `!`, `&&`/`||` short-circuit, concat string literal, perbandingan string (`==`) — dikompilasi ke MIR → Cranelift. |
| **0.9 — Komponen + cross-compile (parsial, saat ini)** | `component` stateless diekspansi compile-time (`<Badge label="x"/>` → render body dengan `props` disubstitusi, tanpa overhead runtime); CLI gaya npm (`init`, `install`, `list`/`ls`, `start`, `dev` watch+auto-restart, `test`, `publish` stub); cross-compile `--target <triple>` (Cranelift multi-arch). |
| 0.9 — Stateful (saat ini) | `state` + event klik + multi-instance + props: init + baca (`gn_state_get_*`) + mutasi method tersalin per instance (`Counter__0__inc`, kunci `Counter#0`); `onClick={this.m}` → hit-test → dispatch → rebuild + redraw. Render divalidasi saat definisi (props dummy) + di-lower per situs. |
| 0.10 — Teks (saat ini, parsial) | `interface` penuh (wajib/opsional, union literal, cek props per pemakaian); font atlas bitmap 5x7 ASCII (teks nyata ganti placeholder; non-ASCII dilewati). |
| 0.9 — Bahasa lengkap (saat ini) | Backend LLVM release nyata (inkwell/LLVM 18, O-agresif, cross-target): `gnative build --release`. |
| 1.0 — CUDA penuh | Kernel rasterizer lengkap, blit framebuffer, parity fitur dengan wgpu. |
| 1.1 — Ekosistem | Registry package, hot reload, animasi, layout flex/grid, font vektor (pengganti bitmap 5x7). |

---

## 14. Glosarium

- **`.gn`** — file source UI Gnative (TypeScript-like DSL).
- **Semantic IR** — representasi semantik setelah AST (sebelum MIR).
- **MIR** — intermediate representation mesin Gnative (instruksi low-level).
- **Cranelift** — codegen dev (murni Rust, cepat) → object file.
- **LLVM** — codegen release (inkwell/LLVM 18) → object file teroptimasi.
- **libruntime** — `libgnative_runtime.a` (staticlib Rust): window + render wgpu/CUDA,
  di-link bersama object `.gn`.
- **Scene** — representasi renderable UI (bukan DOM).
- **RenderBackend** — kontrak rendering; wgpu (utama) / CUDA (fallback).
- **container/** — environment kompilasi Gnative (packages, cache, artifacts).
