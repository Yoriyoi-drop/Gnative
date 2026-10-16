# AUDIT.md — Gnative v1.0.0 Production Readiness & Release Gate Audit

> Bahasa: Indonesia. Tanggal audit: 14 Okt 2026. Kondisi repo: tanpa tag rilis,
> semua crate versi `0.1.0`, tanpa `LICENSE`/`CHANGELOG`/CI. Audit berbasis bukti:
> baca source, grep, `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`,
> inspeksi artefak. Tidak ada perubahan source selama audit.

## Metode dan perintah verifikasi aktual

- `cargo check --workspace` → PASS (~3 detik).
- `cargo test --workspace` → PASS semua yang berjalan: compiler 49, lang 20,
> core 9, gfx-scene 3, wgpu 8, runtime 10, cli 0, cuda 0. Total berjalan 99.
> `grep #[test]` menemukan 103 di source — selisih 4 adalah test mati di
> `crates/gnative-runtime/src/ui_builder.rs` (file ada, tapi tidak dideklarasikan
> sebagai `mod` di `lib.rs:12-15`, jadi tidak dikompilasi).
- `cargo clippy --workspace -- -D warnings` → PASS. `cargo fmt --check` → PASS.
- `grep unsafe` → 35 titik, hampir semua di `gnative-runtime` (FFI/CStr/transmute)
> plus 1 di `codegen/llvm.rs:600`.
- `.github/` → tidak ada. `ls LICENSE*/CHANGELOG*` → tidak ada.
- `Cargo.lock` ada di disk (75KB, 302 `[[package]]`) tapi di-`.gitignore`, jadi tidak terkunci di git.
- Artefak: `demo`/`hello` binary ~208M vs `.o` 2-7K; `target/debug/gnative` ~244M;
> `target/debug/libgnative_runtime.a` ~498M. `ldd demo` hanya libm/libgcc/libc (wgpu statis).
- `llvm-config` 18.1.3 tersedia di mesin audit — `build --release` bisa jalan di sini,
> tapi tidak dijamin di mesin pengguna.

## A. Executive Summary

Gnative adalah framework GUI native (.gn → IR → MIR → Cranelift/LLVM → object →
link libruntime wgpu/CUDA → binary). Arsitektur tiga dunia (developer/container/
native-build) koheren di dokumen dan kode. Pipeline dev end-to-end terbukti jalan
(object ELF + binary + window wgpu pada demo yang sudah di-build).

Namun ini **bukan kondisi v1.0.0 production**. Ditemukan 9 blocker P0 dan belasan
P1 yang menyentuh kebenaran kompiler, keselamatan memori/FFI, klaim incremental/
distribusi, dan ketiadaan pintu rilis (CI, lisensi, versioning, SBOM/provenance).
Versi masih `0.1.0` di 9 Cargo.toml, dokumen mengklaim milestone 0.8–0.10 secara
campuran, CUDA adalah stub di kedua cfg, dan `publish`/registry roadmap 1.1.
Keputusan: **NO-GO untuk v1.0.0**.

## B. Keputusan Release Gate — NO-GO

Ada P0 yang belum ditutup dan gate inti (Compiler, Memory, Testing, Security,
Build, Distribution, Release) berstatus FAIL. Dilarang memakai GO/CONDITIONAL-GO.
Syarat mutlak sebelum RC ulang: tutup semua P0, perbaiki P1 release-blocker,
aktifkan CI minimal, kunci lockfile + lisensi + changelog, dan buktikan matriks
gate di bawah berstatus PASS terverifikasi.

## Matriks Release Gate

| Gate | Persyaratan | Status | Bukti | Risiko |
|---|---|---|---|---|
| Compiler | Grammar + semantik terverifikasi | FAIL | Parser panic EOF; checker tanpa scope; coerce diam ke F64; unreachable Cranelift | Miscompile/crash |
| Runtime | Lifecycle + state stabil | PARTIAL | Event loop jalan tapi busy-redraw, resize tak relayout, state global tak reset | CPU boros, klik meleset HiDPI |
| Rendering | Jalur GPU + failure handling | PARTIAL | wgpu render teks ASCII + quad; device-lost tanpa recovery; CUDA stub | Headless langsung gagal |
| Memory | Tanpa defect kritis | FAIL | expect di extern C; transmute tanpa validasi; CStr tanpa batas; str_ptr dangling | UB/crash/eksploitasi |
| API | Kontrak publik jelas + stabil | PARTIAL | RenderBackend stabil; radius/clip/diabaikan; hstack/image dikenal tapi ditolak lower | API bohong |
| Testing | Test inti jalan di CI | FAIL | 99 lolos lokal tapi tanpa CI; 0 test CLI; 4 test mati; e2e skip diam bila libruntime absen | Regresi tak tertangkap |
| Security | Risiko kritis ditangani | FAIL | Path traversal; tanpa batas resource; exec cc dari PATH; FFI tanpa validasi | Tulis di luar project, DoS |
| Build | Clean + release build sukses | PARTIAL | check/clippy/fmt hijau; build lapor sukses tanpa binary; cross-link fallback host | CI hijau palsu |
| Distribution | Artefak terpasang dari nol | FAIL | Binary 208M tanpa installer/strip/checksum/provenance/rollback | Distribusi manual raksasa |
| Documentation | Quickstart + contoh tervalidasi | PARTIAL | Contoh terkompilasi tapi quickstart tak akurat PATH; emoji non-ASCII vs atlas ASCII | User tersesat |
| Release | Versi/changelog/artefak konsisten | FAIL | Semua 0.1.0 vs klaim 0.10; tanpa tag/changelog/lisensi | Tak bisa rilis legal |


## C. Temuan berdasarkan keparahan

### P0 — blocker rilis (9)

**P0-01 Parser panic pada input truncated/EOF.**
Kategori: compiler/parser. File: `crates/gnative-lang/src/parser.rs:839-858`
(`current`/`bump`/`at_eof`/`check` indeks langsung `self.tokens[self.pos]`;
`parse_atom` fallback `bump` walau EOF). Bukti terverifikasi via baca source.
Reproduksi: parse file `.gn` terpotong tanpa token penutup → panic indeks,
bukan `Result`. Dampak: compiler crash (DoS via source rusak). Perbaikan:
ganti semua akses token dengan `get`, kembalikan `GnError::ParseError`.
Regresi: test file kosong, terpotong tiap posisi, fuzz corpus truncated.
Tutup: tidak ada panic pada 10k input adversarial + `cargo test` hijau.

**P0-02 Checker tanpa scope simbol lokal → miscompile diam.**
Kategori: bahasa/semantik. File: `crates/gnative-lang/src/checker.rs:113-125,241-299`
(hanya cek Ident huruf besar + Call huruf kecil; `let`/param/state/props/`this`
tak didaftarkan); `crates/gnative-compiler/src/lower.rs:740` semua Ident jadi
`Var`; `codegen/cranelift.rs:588-594`, `llvm.rs:528-540` `LoadVar` default 0.
Reproduksi: rujuk variabel typo di fungsi → lolos checker → runtime baca 0.
Dampak: logika salah diam. Perbaikan: tabel simbol scope + error unknown-variable.
Regresi: test typo-var, shadowing, param tak dipakai. Tutup: semua rujukan
tak dikenal gagal sebelum codegen.

**P0-03 Ekspansi stateless tanpa guard siklus → stack overflow.**
Kategori: compiler/lowering. File: `crates/gnative-compiler/src/lower.rs:1093-1120`
(`expand_component` → `expr_node` rekursif tanpa cek `expanding`; guard hanya di
`lower_instance:285-290` untuk stateful). Reproduksi: `A render→<B/>`,
`B render→<A/>` stateless → rekursi tak berujung. Dampak: crash compiler via
source valid-parser. Perbaikan: guard siklus untuk stateless + error jelas.
Regresi: test siklus stateless A↔B dan self-recursion. Tutup: kembali error,
tidak overflow.

**P0-04 Coercion tipe diam ke F64.**
Kategori: compiler/type-soundness. File: `lower.rs:40-46,202-204,433-435`
(`unwrap_or(IrTy::F64)` untuk Element/array/union/generic/unknown).
Reproduksi: pakai `string[]`/Element sebagai tipe → jadi angka diam.
Dampak: type unsound, miscompile. Perbaikan: error untuk tipe tak didukung,
daftar explisit di dokumen bahasa. Regresi: test tiap tipe tak didukung gagal
dengan pesan. Tutup: tidak ada coerce diam di jalur sukses.

**P0-05 Backend Cranelift `unreachable!` vs LLVM `Err` (divergen).**
Kategori: compiler/codegen. File: `codegen/cranelift.rs:648-649,660-662,785`
(relasional bool/str → unreachable; `slot_vars[&slot]`, `func_ids[callee]`
indeks langsung) vs `llvm.rs:866-870` kembalikan `GnError`.
`mir_lower.rs:626-634` dapat emit CmpBool/CmpStr relasional.
Reproduksi: MIR dengan `<` pada bool/str → dev panic, release error rapi.
Dampak: backend dev crash. Perbaikan: validasi MIR + error seragam, tanpa
`unreachable` pada input pengguna. Regresi: test matriks op×tipe di kedua
backend. Tutup: perilaku identik (error rapi) di kedua backend.

**P0-06 Klaim incremental salah (cache hanya dihitung, tak dipakai skip).**
Kategori: compiler/cache. File: `pipeline.rs:168-172,196-208` (selalu parse+lower;
`cache_hits/regenerated` hanya hitung). `fingerprint.rs:14-16` hash konten saja
(tanpa versi compiler/flag/runtime). `cache.rs:91-111` sanitasi bisa collision.
Reproduksi: build 2× tanpa perubahan → kerja penuh ulang walau lapor hit.
Dampak: klaim incremental salah; risiko object basi bila skip dipakai kelak.
Perbaikan: pakai cache untuk skip nyata + kunci (hash konten + versi compiler
+ flag + runtime) + invalidasi saat toolchain berubah. Regresi: test build 2×
(deteksi skip), ganti flag → rebuild. Tutup: metrik cache sesuai kerja yang
dilewati + tidak ada artefact basi lintas versi.

**P0-07 `expect` di dalam `extern "C"` (poison → panic lintas FFI = UB).**
Kategori: memory/FFI. File: `intrinsics.rs:54,64,81,116,153,207,226,243,260,271,354,415,424,427`
(14× `.lock().expect`). Bukti via grep. Dampak: abort/UB, langgar aturan
tanpa-unwrap jalur publik. Perbaikan: tangani poison (buka atau error aman,
jangan panic lintas FFI). Regresi: test mutex poisoned → tidak panic/UB.
Tutup: tidak ada `expect/unwrap/panic` di semua `extern "C"`.

**P0-08 `transmute usize→fn()` tanpa validasi + `CStr::from_ptr` tanpa batas.**
Kategori: memory/FFI. File: `intrinsics.rs:226-228,243-248` (transmute alamat
dari binary, hanya cek null di `gn_window_run_dynamic:112`);
`intrinsics.rs:49,145,148,288,308,345,348,351,392` + `state_store.rs:35,38`
(hanya cek null, `title_len` diabaikan, andalkan NUL codegen).
Dampak: alamat liar → crash/eksploitasi; binary korup → OOB read.
Perbaikan: validasi rentang/null-termination dengan batas eksplisit, pakai
panjang yang diteruskan, tolak pointer tak valid dengan error. Regresi: test
pointer null/liar/tanpa-NUL/overlong. Tutup: semua FFI terbatasi + fuzzer FFI
tanpa crash.

**P0-09 Path traversal tulis/baca.**
Kategori: security. File: `compiler/install.rs:42`, `compiler/resolve.rs:45-48`,
`cli/container.rs:46`, `cli/new.rs:13` (`join(name)` tanpa tolak `..`/`/`/`\`;
`Path::new(name)` terima `../x`). Reproduksi konseptual: deps
`"../../evil"` → tulis di luar `container`; `container inspect ../../..`
list direktori arbitrer. Dampak: repo jahat tulis di luar project.
Perbaikan: sanitasi + canonicalize + tolak escape root + error jelas.
Regresi: test `..`, absolut, separator Windows. Tutup: semua path terkungkung
di root project/container.


### P1 — mayor, release-blocker (16)

**P1-01 Span zero-width + offset char-vs-byte.** `lang/lexer.rs:294-304`,
`lang/span.rs:15-16` (doc klaim byte,実 char index; semua token panjang 0).
Error tunjuk kolom akhir; unicode geser offset. Perbaikan: span start..end
byte + test unicode/emoji. Tutup: span presisi per byte pada corpus.

**P1-02 `eat` diam tanpa diagnostik.** `lang/parser.rs:865-871` dipakai ~40x
(`; } : =`). Sintaks hilang lolos diam → AST miring. Perbaikan: error eksplisit
tiap token wajib. Regresi: test tiap delimiter hilang. Tutup: tidak ada `eat`
diam di jalur wajib.

**P1-03 Angka `1.2.3` → `0.0` diam.** `lang/lexer.rs:94-106`
(`parse::<f64>().unwrap_or(0.0)`). Typo jadi nol lolos. Perbaikan: error leksikal.
Regresi: test `1.2.3`, `.`, `1e`. Tutup: angka invalid selalu error.

**P1-04 String tanpa escape/unterminated.** `lang/lexer.rs:108-121` (tanpa
`\n \t \" \\`; EOF dipotong `saturating_sub(1)`). Multiline tak sengaja boleh;
file terpotong hasilkan string palsu. Perbaikan: escape + error unterminated.
Tutup: corpus string/escape/i18n lolos.

**P1-05 Precedence tanpa spec; `%` hilang.** `lang/parser.rs:547-607`
(precedence hardcode; `%` dilex `token.rs:72` tapi tak diparse).
`hstack`/`image` dikenal checker tapi ditolak lower. Perbaikan: spec grammar
+ error untuk token tak didukung. Tutup: grammar terdokumen = perilaku parser.

**P1-06 Duplikat/siklus tak lengkap; multi-window timpa diam.**
`lang/checker.rs:94-139` (tanpa window ganda/const/state/method/param ganda);
`pipeline.rs:203-205` (`ir.window=Some` timpa). Perbaikan: deteksi duplikat
menyeluruh + error multi-window. Tutup: tiap duplikat diuji gagal.

**P1-07 Props hanya literal; lokasi hilang.** `checker.rs:165-208,326-356`
(non-literal diskip; `Span::default()` untuk tipe Named/Generic).
Salah tipe via variabel lolos sampai runtime tanpa lokasi. Perbaikan: simpan span
+ validasi penuh atau tolak eksplisit. Tutup: error tipe selalu berlokasi.

**P1-08 `const`/`interface`/`type-alias` hilang di IR.**
`compiler/lower.rs:191` (`Decl::Interface|TypeAlias|Const=>{}` diam) → rujukan
jadi `LoadVar` 0. Perbaikan: lower konstanta atau error jelas. Tutup: tidak ada
deklarasi yang hilang diam.

**P1-09 ABI string-key + mangling bisa tabrakan.** `compiler/ir.rs:37-39`,
`mir_lower.rs:20-22` (`Comp#inst`, `Comp__i__m` tanpa escaping/namespace/versi).
Default tipe hilang → F64 (`mir_lower.rs:303-318`). Perbaikan: escaping +
namespace + versi + error tipe hilang. Tutup: tidak ada collision pada uji.

**P1-10 Tanpa debug info; diagnostik string tanpa span.**
IR/MIR tanpa Span; `lower.rs` error String saja; `pipeline.rs:199-202`
kehilangan file:line. Perbaikan: bawa Span IR→MIR→diagnostik + snippet.
Tutup: tiap error compiler petakan ke file:line:col.

**P1-11 DepGraph mati; resolver dibuang; versi tak dicek.**
`graph.rs` tak dipanggil pipeline; `pipeline.rs:187-193` buang hasil resolve,
kumpul file via walk+sort; `config.rs:53-59` versi dekorasi. Siklus tak
terdeteksi. Perbaikan: pakai graph sungguhan + cek versi. Tutup: siklus dan
versi salah gagalkan build.

**P1-12 Config default diam + build sukses semu tanpa binary.**
`compiler/config.rs:35-44` (nama→app tanpa warning);
`pipeline.rs:94-113` + `cli/cmd/build.rs:17-40` (runtime hilang → diagnostik
tanpa `has_errors`, exit 0, cetak sukses). Fresh checkout hijau palsu.
Perbaikan: gagalkan build bila binary None + warning config. Tutup: fresh
checkout tanpa libruntime = exit non-0.

**P1-13 Cross-link fallback host + runtime-dir tak cari `target/<triple>`.**
`compiler/linker.rs:91-130` (append host cc; hanya cek target/debug|release).
`--target aarch64` pakai host cc → error samar; libruntime per-target tak
ditemukan. Perbaikan: tanpa fallback diam + cari `target/<triple>/…`.
Tutup: cross tanpa toolchain = error jelas, bukan link host.

**P1-14 Tanpa CI/integration harness; 0 test CLI; e2e skip diam.**
Glob `.github/Dockerfile/*.yml/Makefile/scripts/tests/` nihil (terverifikasi).
103 test unit, 0 di `gnative-cli`; `pipeline.rs:664-669,794-799,893-898`
skip bila libruntime absen. `graph` non-rekursif + cycle exit 0
(`cli/cmd/graph.rs:13-50`); `check` abaikan args. Perbaikan: CI minimal
(check+clippy+fmt+test), test CLI, fail bila prasyarat absen. Tutup: CI hijau
artinya semua gerbang jalan, bukan skip.

**P1-15 Tanpa batas resource untuk input tak tepercaya.**
Parser rekursif tanpa depth; `mir_lower.rs:411-430` for-unroll penuh;
walk rekursif + `read_to_string` tanpa batas; FNV-1a bukan anti-collision.
`.gn` jahat → stack overflow/OOM. Perbaikan: depth limit, batas unroll/ukuran,
timeout/isolasi. Tutup: corpus adversarial + fuzz tanpa hang/OOM.

**P1-16 `ui_builder.rs` modul mati (4 test tak pernah jalan).**
File ada dengan 4 `#[test]`, tapi `runtime/lib.rs:12-15` tidak deklarasikan
`pub mod ui_builder` (terverifikasi). Perbaikan: daftarkan atau hapus +
pindahkan test. Tutup: `cargo test` hitung = grep test.


### P2 — moderat (14, ringkas)

**P2-01 Template `${}` jadi literal diam.** `lexer.rs:123-137`, `lower.rs:738`.
Perbaikan: interpolasi nyata atau error. Tutup: test template eksplisit.
**P2-02 Ident unicode inkonsisten; hanya komentar `//`.** `lexer.rs:40-57,248-265`
(`$#~^` jadi Ident; tanpa `/* */`). Perbaikan: spec ident + block-comment
atau error jelas. Tutup: sesuai spec.
**P2-03 `else-if` hack `pos-=1`; `for` hanya `of`.** `parser.rs:491-499`.
Perbaikan: parse else-if aman + dokumentasi batas for. Tutup: tanpa underflow.
**P2-04 `null→0.0`; `size/gap` wajib literal.** `lower.rs:498,739,861-900`.
Perbaikan: null-typing + error dinamis yang jelas. Tutup: matriks literal/ekspresi.
**P2-05 Label/terminator divergen antar backend.** `cranelift.rs:482-499` Err vs
`llvm.rs:393-403` bridge diam. Perbaikan: normalisasi MIR + satu aturan.
Tutup: backend identik.
**P2-06 Intrinsik selalu diimport; `len=0` diabaikan; intern O(n).**
`cranelift.rs:76-226,722`, `llvm.rs:224-277`, `mir_lower.rs:340`. Perbaikan:
import selektif + pakai len + interner hash. Tutup: object fungsi murni bersih.
**P2-07 Busy redraw loop.** `runtime/event_loop.rs:111-133` render lalu
`request_redraw` tanpa syarat vs `ControlFlow::Wait:215`. Idle 100% CPU/GPU.
Perbaikan: redraw on-demand/dirty-flag. Tutup: idle tanpa submit berulang.
**P2-08 Resize tak relayout; campur fisik/logis.** `event_loop.rs:100-110,134-177`
(`scene` tak `layout()` ulang; fisik vs logical). Klik meleset HiDPI.
Perbaikan: relayout + unit konsisten + test HiDPI. Tutup: resize/klik benar di 1.5x/2x.
**P2-09 `action_at` z-order terbalik; Group/Action offset bukan layout.**
`gfx/scene.rs:226-298` (LIFO timpa → bawah menang; Stack dalam Group tak
`layout_nodes`). Perbaikan: iterasi front-to-back + layout rekursif + test overlap.
**P2-10 Adapter tanpa `compatible_surface`; format/alpha indeks langsung.**
`gfx-wgpu/renderer.rs:29-33`, `device.rs:28-57`, `surface.rs:30-44`
(`formats[0]`/`alpha_modes[0]` bisa panic). Perbaikan: pilih adapter compatible +
cek kosong → GnError. Tutup: driver kosong = error rapi.
**P2-11 `Vertex::quad` div0; radius disimpan tapi tak dirender; clear alpha
dipaksa 1.0; buffer per-frame tanpa cap; tanpa clip/scissor; geometri tak
divalidasi.** `vertex.rs:28-31`, `renderer.rs:100-184`, `scene.rs:82-161`.
Perbaikan: guard + SDF/radius nyata atau hapus API + cap + scissor + validasi
NaN/Inf. Tutup: scene ekstrem tak hasilkan NaN/UB.
**P2-12 Windowing/input lubang besar.** `event_loop.rs:92-152` hanya
Close/Resized/Redraw/CursorMoved/MouseInput-kiri; keyboard/wheel/focus/scale/
IME/clipboard/touch nihil; `core/event.rs` dead abstraction; `_window_id`
diabaikan; minimize 0x0 truncasi. Perbaikan: tangani/nyatakan-tak-dukung +
test routing. Tutup: matriks input vs klaim.
**P2-13 `str_ptr` dangling; HashMap tanpa batas; lock poison inkonsisten;
re-entrancy callback; float ekstrem ke GPU.** `state_store.rs`, `intrinsics.rs`,
`core/color.rs,vec.rs,units.rs,rect.rs`. Perbaikan: lifetime/ownership jelas,
bound, guard re-entrancy, validasi numerik. Tutup: uji terkait hijau.
**P2-14 Observabilitas minim.** Hanya `println!/eprintln!` (~30 titik), tanpa
level/tracing; pesan campur Inggris/Indonesia (`core/error.rs:30-36`); `doctor`
tak cek llvm/cuda/target, tak konsisten dengan linker; info versi minim.
Perbaikan: level log + pesan seragam + doctor jujur. Tutup: `--verbose` + doctor
menampilkan versi/target/toolchain.

### P3 — minor/ergonomi (7, ringkas)

**P3-01** Doc rujuk `scene_bridge.rs` hilang (`ui_builder.rs:5`). Rapikan doc.
**P3-02** `--target` tanpa nilai jadi host diam (`codegen/mod.rs:37-41`).
Tolak arg kosong dengan error.
**P3-03** `container verify` tautologi (`contains("gnative")`) + mismatch dokumen
(`container.rs:73-80`). Kriteria nyata + selaraskan DESIGN/container.md/help.
**P3-04** `new`/`clean` telan error (`let _ = write/remove`). Propagasi error.
**P3-05** `dev` polling 300ms tanpa debounce; komentar SIGKILL menyesatkan;
Windows taskkill. Debounce + shutdown bersih.
**P3-06** `run`/`dev` spawn tanpa verifikasi (TOCTOU); `status.code().unwrap_or(1)`
tutup sinyal. Verifikasi path + laporkan sinyal.
**P3-07** Fallback CUDA semu: `CudaRenderer::new()` selalu
`BackendUnavailable` di kedua cfg (`gfx-cuda/renderer.rs:20-37`) + `render_scene`
selalu Err. Nyatakan CUDA belum didukung di semua dokumen atau implementasikan.


## D. Matriks komponen

| Crate/subsistem | Implementasi | Test aktual | Dokumentasi | Risiko | Gate |
|---|---|---|---|---|---|
| gnative-core | PASS (geometri/warna/error) | 9 lolos | Baik | Rendah (validasi numerik longgar) | PASS dgn catatan |
| gnative-lang | PARTIAL (grammar implisit, checker lemah) | 20 lolos, tanpa adversarial/unicode | Klaim TS melebihi realita | Tinggi (P0-01/02, P1-01..06) | FAIL |
| gnative-compiler IR/lower/MIR | PARTIAL (coerce diam, siklus, tanpa span) | 24 lower+mir_lower lolos happy-path | DESIGN akurat struktur, optimis incremental | Tinggi (P0-03/04/06) | FAIL |
| codegen Cranelift/LLVM | PARTIAL (divergen error, tanpa debug info) | 7 lolos (ELF magic, cross-aarch64) | Klaim cross tanpa-toolkit setengah benar | Tinggi (P0-05) | PARTIAL |
| linker/pipeline | PARTIAL (sukses semu, fallback host) | 7 pipeline + 1 linker (lemah) | container.md akurat struktur | Tinggi (P1-12/13) | FAIL |
| gnative-gfx scene | PARTIAL (z-order, Group layout salah) | 3 lolos | Baik | Sedang | PARTIAL |
| gnative-gfx-wgpu | PARTIAL (ASCII-only, tanpa recovery) | 8 lolos (atlas+raster, tanpa GPU) | Klaim teks melebihi realita | Sedang-Tinggi | PARTIAL |
| gnative-gfx-cuda | STUB (selalu Err kedua cfg) | 0 | Tercatat roadmap tapi klaim fallback menyesatkan | Sedang | FAIL sbg fallback |
| gnative-runtime | PARTIAL (busy-loop, FFI tak validasi) | 10 lolos + 4 mati | Cukup | Kritikal (P0-07/08) | FAIL |
| gnative-cli | Fungsional happy-path, tanpa test | 0 | Matriks perintah tak konsisten | Sedang (P1-14, P3) | FAIL |
| apps/hello + demo | Contoh terkompilasi (ELF 208M) | Manual (artefak ada) | Emoji contoh rusak parsial di atlas ASCII | Rendah | PARTIAL |
| Build/distribusi | Check/clippy/fmt hijau; distribusi nihil | Tanpa CI | Quickstart tak akurat PATH | Tinggi | FAIL |
| Release eng. | Tanpa tag/changelog/lisensi/SBOM | — | Versi 0.1.0 vs klaim 0.10 | Blocker legal | FAIL |

Catatan jujur: "lolos lokal" bukan "terbukti production". Batas tool: tanpa
Miri/sanitizer/fuzzing terukur, tanpa uji GPU/headless/m Newark, tanpa
`cargo audit/deny`, tanpa benchmark baseline — semua dicatat UNVERIFIED.

## E. Gap analysis (dipisah tegas)

1. Defect wajib sebelum rilis: semua P0-01..09.
2. Fitur wajib diselesaikan/dinyatakan: guard siklus stateless, tabel simbol +
error unknown-var, coerce→error, validasi MIR seragam, cache nyata +
invalidasi versi, FFI aman (tanpa panic/transmute/CStr liar), sanitasi path,
build gagal-jujur (tanpa binary = non-0), CUDA dinyatakan-belum-dukung
(keluarkan dari cakupan v1.0).
3. Kekurangan test/bukti: CI minimal; test CLI; hapus skip-diam e2e;
aktifkan 4 test mati; corpus adversarial + fuzz lexer/parser/lower/IR;
test span/unicode/escape; test resize/HiDPI/device-lost/OOM; test FFI
poison/liar; test traversal; test cross-target + doctor-vs-linker.
4. Optimasi yang boleh tunda (bukan blocker): intern O(n)→hash, import
intrinsik selektif, buffer reuse/cap, debounce dev-watch, pesan bilingual
(pilih satu + konsisten), radius/SDF, low-latency present mode.
5. Dikeluarkan dari cakupan v1.0 (dokumentasikan jujur): CUDA penuh, registry/
`publish`, hot reload, animasi, flex/grid penuh, font vektor/shaping/IME/
clipboard, multi-window, `%`/template-`${}`/block-comment/generic-penuh.
Tidak merusak inti bila dinyatakan tak didukung + error jelas saat dipakai.


## F. Remediation plan (urut dependency + risiko)

**Tahap 1 — P0/P1 fondasi (compiler + memory + security).**
P0-01 (token aman), P0-02 (scope), P0-03 (guard stateless), P0-04 (coerce),
P0-05 (validasi MIR seragam), P1-01/02/03/04 (span/eat/angka/string),
P1-06/07/08 (duplikat/props/const). Tiap item: perbaikan + test regresi di
seksinya. Acceptance: `cargo test` + corpus adversarial tanpa panic/miscompile.

**Tahap 2 — integritas runtime + rendering.**
P0-07/08 (FFI aman), P2-13 (ownership/bound/guard), P2-07/08/09 (redraw,
relayout, z-order), P2-10/11 (adapter/surface/vertex/radius/clip/validasi),
P2-12 (matriks input jujur). Acceptance: uji resize/HiDPI/overlap/ekstrem +
fuzzer FFI bersih.

**Tahap 3 — regression, fuzz, platform.**
P1-15 (batas resource) + fuzz lexer/parser/lower/IR/event/state; P1-14 (hapus
skip-diam, test CLI, aktifkan ui_builder); P1-11 (graph+versi); P1-10/P2-14
(span→diagnostik, log level, doctor jujur). Acceptance: CI jalankan semua +
fuzz 1 jam tanpa crash/hang tak terbatas.

**Tahap 4 — artefak, instalasi, dokumen.**
P0-06 (cache nyata), P0-09 (sanitasi path), P1-12/13 (build jujur + cross),
P2-06/P3 (import selektif, dev debounce, pesan seragam), P1-16 selesai,
strip/split-debug + ukuran/startup terdokumen, quickstart dari env bersih,
selaraskan README/DESIGN/container.md/AGENTS/help. Acceptance: fresh-checkout
→ build gagal-jujur tanpa libruntime; dengan libruntime → binary + run manual.

**Tahap 5 — final RC verification.**
Kunci `Cargo.lock` di git, tambah lisensi + `rust-version` + changelog, tag RC,
checksum + provenance, uji instalasi dari artefak (bukan source tree),
rollback/security-disclosure policy, matriks gate PASS. Tanpa publish artefak
selama audit (sesuai larangan). Acceptance: checklist G di bawah semua centang
dengan bukti.

## G. Final RC checklist (dapat dijalankan maintainer)

- [ ] `cargo check --workspace` hijau.
- [ ] `cargo test --workspace` hijau tanpa skip-diam; hitung = grep `#[test]`.
- [ ] `cargo clippy --workspace -- -D warnings` hijau; `cargo fmt --check` hijau.
- [ ] Corpus adversarial parser (kosong/besar/terpotong/nesting/unicode) tanpa panic.
- [ ] Typo-var/duplikat/siklus (stateless+stateful)/tipe-tak-dukung selalu error jelas.
- [ ] Matriks op×tipe identik di Cranelift dan LLVM (tanpa `unreachable`).
- [ ] Build 2× tanpa ubah → cache jujur; ganti flag/versi → rebuild.
- [ ] Tanpa `expect/unwrap/panic` di `extern "C"`; fuzzer FFI bersih.
- [ ] Traversal (`..`, absolut) ditolak; path terkungkung root.
- [ ] Tanpa libruntime → `build` exit non-0 dengan pesan; dengan libruntime → binary.
- [ ] Cross `--target` tanpa toolchain = error jelas (tanpa fallback host diam).
- [ ] Resize/relayout + klik HiDPI + overlap z-order + scene ekstrem benar.
- [ ] `doctor` konsisten dengan linker; tampilkan versi/target/llvm/cuda/libruntime.
- [ ] CI minimal aktif (check+clippy+fmt+test) + test CLI + 4 test mati aktif.
- [ ] `Cargo.lock` ter-commit; lisensi + MSRV + changelog + tag RC + checksum.
- [ ] Quickstart dari env bersih sampai `run` terdokumen + contoh terkompilasi.
- [ ] Fitur belum-siap dinyatakan tak didukung + error jelas (CUDA/registry/animasi/
font-vektor/multi-window/`%`/`${}`/block-comment).
- [ ] Tidak ada rewrite besar; DESIGN.md diperbarui bila arsitektur berubah.

## Batasan dan kesimpulan (3 jawaban)

1. Terbukti siap: pipeline dev `.gn`→ELF→binary→window wgpu untuk contoh kecil;
check/clippy/fmt hijau; 99 test lolos lokal; struktur crate + batas dependensi
disiplin (core/gfx ringan, GPU hanya di wgpu/runtime).
2. Penyebab gagal production: 9 P0 (parser panic, scope hilang, siklus stateless,
coerce diam, divergen backend, cache semu, panic lintas FFI, transmute/CStr
liar, traversal) + P1 (sukses-semu build, cross fallback, tanpa CI/lisensi/
lock, tanpa batas resource, CUDA stub, input/windowing minim, observabilitas
minim).
3. Bukti + perubahan minimum sebelum v1.0.0: tutup Tahap 1–2 + CI + lisensi/lock/
changelog + checklist G. Tanpa itu, rilis apa pun harus bukan v1.0.0.

*Akhir laporan. Dibuat sebagai `audit.md` sesuai permintaan; source tidak diubah.*

## Log Remediasi (loop bertahap)

### Iterasi 1 — P0-01 parser anti-panic (14 Okt 2026)
- File: `crates/gnative-lang/src/parser.rs` (constructor sentinel Eof di akhir,
  `current`/`prev`/`bump`/`at_eof`/`check` tanpa indeks langsung,
  `else-if` saturating_sub, field sentinel `eof` tanpa `expect`).
- Test: `cargo test -p gnative-lang` 22 lolos (2 test regresi baru:
  input kosong/terpotong ×10, token kosong). `clippy -D warnings` hijau.
- Review subagent: 10 cela P2/P3 ditemukan, semua diperbaiki sebelum commit
  (fallback sentinel milik parser, cek `last()` bukan `any()`, saturating_add,
  cabang empty).
- Status P0-01: CLOSED (sisa: audit span zero-width P1-01 terpisah).

### Iterasi 2 — P0-03 guard siklus stateless (loop tick)
- File: `crates/gnative-compiler/src/lower.rs` (`expand_component` guard
  `expanding` cermin `lower_instance`, push/pop aman di semua jalur + komentar).
- Test: 2 regresi baru (`siklus_stateless_ditolak` A↔B, `siklus_stateless_self_ditolak`);
  `cargo test -p gnative-compiler siklus` 3/3 lolos.
- Review subagent: approve; 2 low/info ditindaklanjuti sebagian (komentar guard,
  saran test 3-node/sibling dicatat backlog).
- Status P0-03: CLOSED.

### Iterasi 3 — P0-02 scope simbol lokal di checker (16 Okt 2026)
- File: `crates/gnative-lang/src/checker.rs` (tambah `scopes: Vec<HashSet<String>>`
  + `const_names`; helper `scope_push`/`scope_pop`/`declare_local`/`is_known_local`;
  `check_function`/`check_component`/`check_stmt`/`check_expr` jadi `&mut self`;
  daftarkan param/state/this/props/let/loop-var; `Expr::Ident`/`Assign`/`Arrow`
  validasi rujukan tak dikenal → `variabel X tidak dikenal`).
- Test: 6 regresi baru (typo-var fungsi/method, props/this dikenal, let/for bersih,
  const infra). `cargo test -p gnative-lang` 27 lolos; `cargo test -p gnative-compiler`
  51 lolos (tanpa regresi). `cargo fmt --check` + `cargo clippy -D warnings` hijau.
- Status P0-02: CLOSED (sisa: const belum ter-parse di parser → P1-08).

### Iterasi 4 — P0-04 coercion tipe diam ke F64 (16 Okt 2026)
- File: `crates/gnative-compiler/src/lower.rs` (ganti `gn_ty_to_ir` yang
  `unwrap_or(IrTy::F64)` dengan `lower_ty(ty: &Type) -> Result<Option<IrTy>, String>`;
  tipe didukung number/boolean/string/void → `Ok`, tipe lain `Element`/`string[]`/
  union/generic/fn/unknown → `Err` eksplisit "tidak didukung oleh codegen native".
  Terapkan di 8 titik: param fungsi + method, return fungsi + method, field state,
  dan `let` (dengan/tanpa state). `void` sebagai param/field → error "tidak valid
  untuk parameter/field state"; return tanpa anotasi atau `void` → `None` (void).
  Diagnostic dari `lower` sudah diubah jadi `has_errors` di `pipeline.rs:196-203`,
  sehingga tipe tak didukung gagal build, bukan miscompile diam).
- Test: 5 regresi baru (`tipe_param_array_ditolak`, `tipe_param_union_ditolak`,
  `tipe_return_element_ditolak`, `tipe_let_element_ditolak`,
  `tipe_didukung_tetap_berhasil`). `cargo test -p gnative-compiler` 56 lolos
  (tanpa regresi). `cargo fmt --check` + `cargo clippy -D warnings` hijau.
- Status P0-04: CLOSED (tidak ada coerce diam di jalur sukses; tipe tak didukung
  selalu error; perluasan daftar tipe didukung → P1-05/roadmap bahasa).

### Iterasi 5 — P0-05 validasi MIR seragam antar backend (16 Okt 2026)
- File: `crates/gnative-compiler/src/codegen/cranelift.rs` (ganti 3 `unreachable!`
  dengan `return Err(GnError::Other("relasional non-f64 belum didukung (invalid
  MIR)"))` di `CmpBool`/`CmpStr` relasional dan `Label`; pesan disamakan dengan
  `llvm.rs:866-870` agar kedua backend berperilaku identik — error rapi, bukan
  panic/crash pada input pengguna). Tidak ada `unreachable!` tersisa di
  cranelift.rs.
- Test: 2 regresi baru (`cmp_bool_relasional_ditolak_tanpa_panic`,
  `cmp_str_relasional_ditolak_tanpa_panic`) — MIR dengan `<` pada bool/str
  mengembalikan `Err`, bukan panic. `cargo test -p gnative-compiler` 58 lolos
  (tanpa regresi). `cargo fmt --check` + `cargo clippy -D warnings` hijau.
- Status P0-05: CLOSED (matriks op×tipe identik kedua backend; tanpa `unreachable`
  pada input pengguna; sisa: `slot_vars[&slot]`/`func_ids[callee]` indeks langsung
  masih Index-panic bila MIR cacat — dicatat backlog P2/validasi MIR).

### Iterasi 6 — P1-01..04 lexer/parser hardening (16 Okt 2026)
- File: `crates/gnative-lang/src/lexer.rs` + `parser.rs` + `lib.rs`.
  - **P1-01**: `Lexer` kini lacak `byte_pos` (offset byte asli, bukan indeks char);
    `span()` pakai `start_byte..byte_pos` dengan lebar > 0 (sebelumnya `self.pos..self.pos`
    → semua token lebar 0). Kolom menunjuk posisi akhir; unicode (😀=4 byte) menggeser
    byte dengan benar.
  - **P1-03**: `lex_number` tidak lagi `unwrap_or(0.0)`; angka gagal parse
    (`1.2.3`, `1.2.3.4`) → `Diagnostic::error` "angka tidak valid" (token fallback
    0.0 agar parser tak panic).
  - **P1-04**: `lex_string` tangani escape (`\n \t \r \" \' \\ \` \0`) dan deteksi
    string tak terterminasi (EOF sebelum quote tutup) → error.
  - **P1-02**: tambah `Parser::expect(kind, what)` (error eksplisit bila delimiter
    wajib hilang); ubah semua `eat` delimiter struktural (`; { } ( ) [ ] > </ => of`,
    `:` tipe, `=` bind) jadi `expect`. Sisa `eat` hanya yang opsional
    (`is_some()`/`while`/`Comma`/`Dot`/`Question`/`Pipe`). `tokenize` kini
    kembalikan `(Vec<Token>, Vec<Diagnostic>)`; `parse_source` gabungkan ke diagnostic.
- Test: 8 regresi baru (`span_byte_dan_lebar_nonzero`, `span_byte_unicode_tergeser`,
  `angka_invalid_melaporkan_error`, `angka_invalid_multi_titik_error`,
  `string_escape_ditangani`, `string_tak_terterminasi_error`, `expect_hilang_delimiter_error`,
  +前期 `string_content_kept_intact` tetap). `cargo test -p gnative-lang` 34 lolos;
  `cargo test -p gnative-compiler` 58 lolos (tanpa regresi). `cargo fmt --check` +
  `cargo clippy -D warnings` hijau.
- Status P1-01/02/03/04: CLOSED (lexer/parser tidak lagi diam pada input cacat;
  span presisi byte; angka/string cacat → error).
