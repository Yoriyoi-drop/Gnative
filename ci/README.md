# CI

`ci.yml` adalah definisi GitHub Actions untuk repo ini: fmt + clippy
(`-D warnings`) + `cargo check` + build libruntime + `cargo test --workspace`
(dengan `GNATIVE_REQUIRE_RUNTIME=1` agar test e2e tidak skip diam).

## Mengaktifkan

GitHub hanya menjalankan workflow dari `.github/workflows/`. Token push saat
ini tidak memiliki scope `workflow`, sehingga file ini disimpan di `ci/` agar
tetap terversion-control. Untuk mengaktifkan, salin ke path yang benar:

```bash
mkdir -p .github/workflows
cp ci/ci.yml .github/workflows/ci.yml
```

lalu commit dengan token/akun yang punya scope `workflow`.

## Prasyarat runner

- LLVM 18 (`llvm-18-dev`, `libclang-18-dev`, `clang-18`) — backend release
  (inkwell) butuh ini untuk kompilasi.
- `build-essential` — linker `cc` untuk `gnative build`.
