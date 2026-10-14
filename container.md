# container.md — Gnative Container World

Dokumen ini menjelaskan **Container World** Gnative: lingkungan kompilasi project
`.gn` — apa isinya, apa alurnya, dan mengapa `container/generated/` **sudah tidak ada**.

---

## 1. Alur Utama

```
                     Gnative Project
                           │
                    ┌──────▼──────┐
                    │    src/     │
                    │   *.gn      │
                    └──────┬──────┘
                           │
                           ▼
                  ┌─────────────────┐
                  │ Gnative Compiler│
                  └────────┬────────┘
                           │
             ┌─────────────┼─────────────┐
             │             │             │
             ▼             ▼             ▼
        Resolution       Graph         Cache
             │             │             │
             └─────────────┼─────────────┘
                           ▼
                  container/packages/
                           │
                           ▼
              .gn → AST → Semantic IR → MIR
                           │
                           ▼
                 Codegen (Cranelift/LLVM)
                           │
                           ▼
                 container/artifacts/*.o
                           │
                           ▼
              Linker (cc + libgnative_runtime.a)
                           │
                           ▼
                 container/artifacts/<binary>
```

Tidak ada langkah **generate Rust** dan tidak ada **Cargo/rustc** dalam pipeline project.
Compiler menghasilkan **object file** langsung, lalu **linker** menggabungkannya dengan
**libruntime** (window + render wgpu/CUDA) menjadi **binary native**.

## 2. Filosofi Utama

Ada 3 dunia yang harus dipisahkan:

```
┌──────────────────────────────────────┐
│ 1. DEVELOPER WORLD                   │
│    src/*.gn                          │
│    gnative.toml                      │
└──────────────────┬───────────────────┘
                   │ gnative build
                   ▼
┌──────────────────────────────────────┐
│ 2. GNATIVE CONTAINER WORLD           │
│    packages/   — source .gn dependency│
│    cache/      — fingerprint incremental│
│    artifacts/  — object + binary      │
└──────────────────┬───────────────────┘
                   │ object (.gn) + libruntime via linker
                   ▼
┌──────────────────────────────────────┐
│ 3. NATIVE BUILD WORLD                │
│    cc/ld + libgnative_runtime.a      │
│    binary native                     │
└──────────────────────────────────────┘
```

Developer **tidak pernah menyentuh** Container World dan tidak pernah menulis C/Rust.
Semua ditangani kompiler.

## 3. Struktur Project

```
my-app/
├── gnative.toml
├── src/
│   ├── main.gn
│   └── components/        # (milestone 0.8: component/interface)
│
└── container/
    ├── packages/          # source .gn dependency (hasil resolve)
    │   └── @gnative/
    │       └── ui/        # runtime .gn bawaan (di-seed kompiler)
    ├── cache/             # cache incremental kompiler
    │   └── fingerprint/   # hash per source file
    └── artifacts/         # output build
        ├── app.o          # object file (Cranelift dev / LLVM release)
        └── app            # binary native (ter-link dengan libruntime)
```

> `container/generated/`, `container/graph/`, `container/metadata/` dari desain lama
> (pipeline `.gn → .rs`) **tidak dipakai lagi** — tidak ada kode yang di-generate.

## 4. gnative.toml

Deklarasi project (nama, dependency):

```toml
[project]
name = "my-app"
version = "0.1.0"

[gnative]
version = "0.7"

[dependencies]
"@gnative/ui" = "0.7.0"
```

Developer menyatakan kebutuhan secara deklaratif — bukan path file.

## 5. container/packages/

Mirip `node_modules/`, tetapi berisi **source `.gn`**, bukan kode hasil generate.

```
container/packages/
└── @gnative/
    └── ui/
        └── ui.gn          # runtime UI bawaan (.gn)
```

- Package memiliki source `.gn`.
- `@gnative/ui` di-**seed** otomatis oleh kompiler dari
  `crates/gnative-compiler/runtime/@gnative/ui/ui.gn` saat `gnative build`.
- Namespace (`@gnative/`, `@company/`, `@community/`) mencegah bentrok nama
  — ini tetap berlaku dari desain lama.

## 6. container/cache/

Cache incremental — kompiler tidak mengerjakan pekerjaan yang sama berkali-kali.

```
container/cache/fingerprint/
├── main.gn.hash
└── ._container_packages__gnative_ui_ui.gn.hash
```

- Setiap source `.gn` (termasuk runtime bawaan) punya fingerprint deterministik (FNV-1a).
- File tidak berubah → cache hit → tidak dikompilasi ulang.
- `cache_key` internal dipisahkan dari nama file agar path runtime (di luar `src/`)
  tetap konsisten.

## 7. container/artifacts/

Output build native — menggantikan peran `target/` Cargo untuk project `.gn`.

```
container/artifacts/
├── app.o          # object file ELF hasil Cranelift (dev) / LLVM (release)
└── app            # binary native = app.o + libgnative_runtime.a (linker cc)
```

### 7.1 Object file

- Dev: **Cranelift** (`codegen/cranelift.rs`) — codegen murni Rust, cepat.
- Release: **LLVM** (`codegen/llvm.rs`) — roadmap 0.8, saat ini stub.

### 7.2 Binary native

- `linker.rs` memanggil `cc`/`ld`: `cc app.o libgnative_runtime.a -o app`.
- **libruntime** (`libgnative_runtime.a`) adalah staticlib dari
  `crates/gnative-runtime` — berisi window native (winit) + render **wgpu** (primary)
  / **CUDA** (fallback).
- Binary `.gn` memanggil intrinsik C-ABI dari libruntime:
  `gn_window_create(title, len, w, h) → handle` dan `gn_window_run(handle)`.
- Linker mencari libruntime ke atas dari project hingga repo root (`target/debug`);
  jika belum di-build, link dilewati dengan peringatan yang jelas.

## 8. Alur gnative build

```
gnative build
      │
      ▼
Read gnative.toml
      │
      ▼
Seed runtime .gn → container/packages/@gnative/ui
      │
      ▼
Dependency Resolution
      │
      ▼
Check Cache (fingerprint)
      ├──────────────┐
      │              │
   cache hit      cache miss
      │              │
      │              ▼
      │        Parse → AST → Semantic IR → MIR
      │              │
      │              ▼
      │        Codegen Cranelift → object
      │              │
      └───────┬──────┘
              ▼
        Link (cc + libgnative_runtime.a)
              │
              ▼
     container/artifacts/<binary>
```

## 9. Contoh Konkret

`src/main.gn`:

```ts
use "@gnative/ui";

window App {
  title: "Hello Gnative";
  size: [800, 600];
  root: <text content="Halo 👋" size={32} />;
}
```

Hasil build:

```
container/artifacts/
├── App.o            # symbol main() hasil Cranelift
└── App              # binary native ELF (window wgpu)
```

Menjalankan `./container/artifacts/App` membuka **window native** yang dirender wgpu.

## 10. Incremental Build

- Fingerprint FNV-1a deterministik per source file (termasuk runtime `.gn` bawaan).
- Perubahan satu file hanya mengompilasi ulang file itu.
- Dependency invalidation (file berubah → dependen ikut di-regenerate) —
  roadmap milestone 0.8 seiring graph dependency penuh.

## 11. Perintah Terkait Container

- `gnative build` — kompilasi + link (menulis `container/artifacts/`).
- `gnative clean` — hapus `container/artifacts` + `container/cache`.
- `gnative container list` — daftar package ter-resolve.
- `gnative container inspect <pkg>` — rincian package.
- `gnative container verify` — cek konsistensi container.
- `gnative doctor` — cek environment (linker, libruntime, toolchain).

## 12. Git & container/

Yang sebaiknya di-commit:

```
src/               ✅
gnative.toml       ✅
```

Yang tidak perlu di-commit (artifact kompiler):

```
container/packages/    ⚠️ tergantung kebijakan (bisa di-seed ulang)
container/cache/       ❌
container/artifacts/   ❌
```

## 13. Pembagian Tanggung Jawab

| Bagian | Fungsi |
|---|---|
| `src/` | Source `.gn` milik developer |
| `gnative.toml` | Konfigurasi project deklaratif |
| `container/packages/` | Source `.gn` dependency (hasil resolve + seed runtime) |
| `container/cache/` | Cache incremental kompiler (fingerprint) |
| `container/artifacts/` | Object file + binary native hasil build |
| `crates/gnative-runtime` | Libruntime: window native + render wgpu/CUDA (staticlib) |
