# Changelog

Semua perubahan penting pada proyek ini didokumentasikan di file ini.

Format mengikuti [Keep a Changelog](https://keepachangelog.com/id/1.1.0/),
dan versi mengikuti [Semantic Versioning](https://semver.org/lang/id/).

## [Unreleased]

Belum ada perubahan setelah `1.0.0-rc.1`.

## [1.0.0-rc.1] — 2026-10-16

Rilis kandidat pertama. Seluruh temuan audit kesiapan rilis
(`audit.md`: P0-01..09, P1-01..16, P2-01..14, P3-01..07) telah ditutup dan
diverifikasi ulang di lingkungan lokal. Detail bukti per temuan ada di
`audit.md` bagian "Log Remediasi".

### Ditambahkan

- `LICENSE-MIT` + `LICENSE-APACHE` (dual-license `MIT OR Apache-2.0`).
- `CHANGELOG.md` (file ini) dan `SECURITY.md` (kebijakan pelaporan kerentanan
  + kebijakan rollback rilis).
- `ci/provenance.sh` — menghasilkan checksum SHA-256 + provenance artefak
  rilis secara lokal (tanpa publikasi otomatis).
- Field `license` dan `rust-version` (MSRV **1.80**) pada seluruh crate,
  diwariskan dari `[workspace.package]`.
- `Cargo.lock` kini di-commit untuk build rilis yang reproducible.
- Operator `%` (modulo) end-to-end: lexer → parser (presedensi 6, setara `*`)
  → checker → IR → MIR → Cranelift & LLVM.
- Diagnostik ber-span pada deklarasi (window/function/const/interface/type).
- `gnative doctor` melaporkan versi, `rustc` + host triple, linker, libruntime,
  LLVM, dan CUDA; keluar non-0 bila komponen wajib hilang.
- Flag global `--verbose` / `-v` (log debug ke stderr).

### Diubah

- Backend CUDA dinyatakan **kerangka / belum didukung**:
  `CudaRenderer::new()` selalu mengembalikan `BackendUnavailable` dengan pesan
  jelas (bukan stub yang tampak berhasil).
- Pesan error `GnError` diberi prefiks Bahasa Indonesia yang konsisten.
- Tag elemen yang tidak didukung kompiler (`hstack`, `image`, dll.) kini
  menjadi **error** di checker (sebelumnya peringatan), selaras dengan backend.
- Intrinsik `gn_*` hanya dideklarasikan bila benar-benar dipakai
  (`used_intrinsics`), sehingga objek fungsi murni tidak punya referensi FFI.
- `intern` pada lowering string MIR: O(n) → O(1) (HashMap).
- Input runtime dipetakan lewat matriks eksplisit (`input.rs`); posisi kursor
  disimpan dalam piksel fisik agar konsisten di HiDPI.
- Cache build: kunci versi + epoch; mengganti flag/versi memicu rebuild nyata.

### Diperbaiki

- **P0** — parser anti-panic (sentinel EOF, tanpa indeks mentah); scope simbol
  lokal di checker; guard siklus stateless; coercion tipe diam → error;
  validasi MIR seragam (tanpa `unreachable` divergen); cache build nyata;
  FFI aman (tanpa `unwrap`/`panic`/`transmute`/`CStr` liar); sanitasi path
  (traversal `..` dan path absolut ditolak).
- **P1** — span/`eat`/angka/string lexer; duplikat & props & `const`;
  mangling ABI injektif (`_` → `_5f`); `build` gagal-jujur (exit non-0 tanpa
  binary); cross-`--target` tanpa toolchain = error (tanpa fallback host);
  batas resource (kedalaman parser, panjang C-string, jumlah vertex);
  CI + test CLI + 4 test mati diaktifkan; `ui_builder` aktif.
- **P2** — template literal & komentar blok & simbol tak dikenal dilaporkan
  sebagai error; `else if` tanpa hack `pos -= 1`; `null` sebagai nilai
  ditolak; `Label` tanpa terminator = error di LLVM; redraw on-demand (tanpa
  busy loop); relayout saat resize/HiDPI; urutan-z `action_at`; radius SDF
  quad; pemilihan surface format & alpha mode tervalidasi; `state_store`
  menyalin string (bukan menyimpan pointer mentah) + batas entri.
- **P3** — argumen `--target` kosong ditolak; `container verify` memakai
  kriteria nyata; propagasi error `clean`/`new`; debounce `dev`; spawn binary
  terverifikasi; dokumentasi jujur soal CUDA.

### Keamanan

- Semua jalur `extern "C"` bebas dari `unwrap`/`expect`/`panic!`; mutex
  poison dipulihkan lewat helper `locked`; pembacaan C-string dibatasi
  panjangnya.
- Path project dikungkung ke root (penolakan `..` dan absolut, termasuk
  `new` dan `container inspect`).

### Diketahui belum didukung (dinyatakan eksplisit)

- CUDA penuh, registry/`publish`, hot reload, animasi, flex/grid penuh,
  font vektor/shaping/IME/clipboard, multi-window, interpolasi `${}` pada
  template literal, dan generics penuh. Pemakaian fitur ini menghasilkan
  error jelas, bukan kegagalan diam.

[Unreleased]: https://github.com/Yoriyoi-drop/Gnative/compare/v1.0.0-rc.1...HEAD
[1.0.0-rc.1]: https://github.com/Yoriyoi-drop/Gnative/releases/tag/v1.0.0-rc.1
