# Kebijakan Keamanan — Gnative

Dokumen ini menjelaskan cara melaporkan kerentanan, komitmen waktu tanggap,
dan kebijakan rollback rilis. Ditambahkan sebagai bagian remediasi audit
kesiapan rilis (lihat `audit.md`, Tahap 5).

## Versi yang didukung

| Versi         | Didukung |
| ------------- | -------- |
| `1.0.0-rc.1`  | Ya       |
| `< 1.0.0-rc.1`| Tidak    |

Hanya rilis kandidat/rilis terbaru pada cabang utama yang menerima perbaikan
keamanan.

## Melaporkan kerentanan

**Jangan** membuka issue publik untuk masalah keamanan. Gunakan salah satu
kanal privat berikut:

1. **GitHub Private Security Advisory** (disarankan) — buka tab **Security** →
   **Report a vulnerability** pada repository
   <https://github.com/Yoriyoi-drop/Gnative>. Ini membuat kanal privat antara
   pelapor dan maintainer tanpa perlu email.
2. Jika kanal di atas tidak tersedia, hubungi maintainer repository secara
   langsung (lihat `Cargo.toml` → `authors` / profil pemilik repository).

Sertakan bila memungkinkan:

- Versi Gnative (`gnative doctor`) dan host triple (`rustc -vV`).
- Langkah reproduksi minimal, atau file `.gn` pemicu.
- Dampak yang diperkirakan (RCE, sandbox escape, DoS, kebocoran data).
- Apakah masalah muncul dari input yang **tidak tepercaya** (mis. project
  `.gn` dari pihak ketiga) atau hanya dari input lokal tepercaya.

## Batas kepercayaan (penting untuk penilaian)

- **Project `.gn` dan `gnative.toml` diperlakukan sebagai input tidak
  tepercaya.** Kompiler wajib menolaknya dengan `Err`, bukan panic. Semua
  temuan audit P0/P1 tentang panic lintas FFI, `unwrap`, `transmute`, dan
  `CStr` liar termasuk kategori ini.
- **Nama path dan argumen CLI diperlakukan sebagai input tidak tepercaya.**
  Path traversal (`..`) dan path absolut yang keluar dari root project
  ditolak.
- **Toolchain sistem** (`rustc`, linker, `llvm-config`, `nvcc`) diperlakukan
  sebagai tepercaya. Kerentanan yang hanya dapat dipicu dengan mengganti
  toolchain tidak termasuk cakupan.
- Backend CUDA berada di luar cakupan: `CudaRenderer::new()` selalu
  mengembalikan `BackendUnavailable`.

## Komitmen waktu tanggap (best effort)

Proyek ini dikelola tanpa SLA komersial. Target terbaik:

- **Akusisi** laporan: ≤ 3 hari kerja.
- **Penilaian awal** (valid/tidak + tingkat keparahan): ≤ 7 hari kerja.
- **Perbaikan** untuk isu kritikal/tinggi: ≤ 30 hari kerja, lalu rilis patch.
- Pelapor akan dikreditkan di `CHANGELOG.md` bila menginginkan.

## Kebijakan rollback rilis

Jika sebuah rilis ditemukan rusak atau berbahaya setelah dipublikasikan:

1. **Tandai** tag rilis terkait sebagai *pre-release* dan tambahkan catatan
   peringatan pada halaman rilis.
2. **Jangan hapus/ubah tag** yang sudah dipublikasikan (agar checksum dan
   provenance tetap dapat diverifikasi). Kekekalan riwayat lebih penting
   daripada tampilan.
3. **Terbitkan rilis baru** (patch) yang membalikkan atau memperbaiki
   perubahan bermasalah, dengan catatan rollback eksplisit di `CHANGELOG.md`.
4. Jika diperlukan, sediakan **checksum** rilis terakhir yang diketahui baik
   (lihat `ci/provenance.sh`) agar pengguna dapat memverifikasi artefak yang
   aman.
5. Perubahan yang dibalik (revert) wajib memiliki entri `CHANGELOG.md` yang
   menyebutkan: apa yang dibalik, mengapa, dan tindakan yang harus diambil
   pengguna.

## Verifikasi artefak

Setiap rilis menyertakan checksum SHA-256 yang dihasilkan lokal oleh
`ci/provenance.sh`. Verifikasi:

```bash
sha256sum -c SHA256SUMS.txt
```

Provenance (commit, versi rustc, host triple, versi LLVM/CUDA) tercatat di
`PROVENANCE.txt` pada direktori keluaran script tersebut. **Selama audit
kesiapan rilis, tidak ada artefak yang dipublikasikan**; script hanya
menghasilkan berkas lokal.
