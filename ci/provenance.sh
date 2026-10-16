#!/usr/bin/env bash
# provenance.sh — hasilkan checksum SHA-256 + provenance artefak rilis Gnative.
#
# Skrip ini HANYA menulis berkas lokal (default: ./dist). Tidak mengunggah,
# tidak mempublikasikan, tidak menyentuh jaringan. Ini disengaja: selama audit
# kesiapan rilis, artefak tidak boleh dipublikasikan (lihat SECURITY.md dan
# audit.md Tahap 5).
#
# Pemakaian:
#   ci/provenance.sh                       # checksum target/release/gnative + libruntime
#   ci/provenance.sh path/ke/artefak ...   # checksum berkas/direktori yang diberikan
#   OUT_DIR=dist ci/provenance.sh          # ubah direktori keluaran
#
# Keluaran:
#   <OUT_DIR>/SHA256SUMS.txt   — daftar "hash  nama" (verifikasi: sha256sum -c)
#   <OUT_DIR>/PROVENANCE.txt   — commit, versi, toolchain, host triple, LLVM, CUDA

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

OUT_DIR="${OUT_DIR:-dist}"
mkdir -p "$OUT_DIR"

# --- Versi dari [workspace.package] (satu sumber kebenaran) -------------------
VERSION="$(awk '
  /^\[workspace\.package\]/ { in_pkg = 1; next }
  /^\[/ { in_pkg = 0 }
  in_pkg && /^version[[:space:]]*=/ {
    gsub(/.*=[[:space:]]*"/, ""); gsub(/".*/, ""); print; exit
  }
' Cargo.toml)"

if [ -z "$VERSION" ]; then
  echo "provenance: gagal membaca versi dari Cargo.toml ([workspace.package])" >&2
  exit 1
fi

# --- Artefak yang di-checksum -----------------------------------------------
if [ "$#" -gt 0 ]; then
  ARTIFACTS=("$@")
else
  ARTIFACTS=("target/release/gnative" "target/release/libgnative_runtime.a")
fi

# Kumpulkan berkas yang benar-benar ada; laporkan yang hilang sebagai
# peringatan (bukan error fatal) agar skrip tetap berguna pada build parsial.
FILES=()
MISSING=()
for a in "${ARTIFACTS[@]}"; do
  if [ -d "$a" ]; then
    while IFS= read -r f; do
      FILES+=("$f")
    done < <(find "$a" -type f | sort)
  elif [ -f "$a" ]; then
    FILES+=("$a")
  else
    MISSING+=("$a")
  fi
done

# --- Checksum ----------------------------------------------------------------
SUMS="$OUT_DIR/SHA256SUMS.txt"
: > "$SUMS"
for f in "${FILES[@]:-}"; do
  [ -n "$f" ] || continue
  # Format "hash  nama" yang dimengerti `sha256sum -c`.
  sha256sum "$f" >> "$SUMS"
done

# --- Provenance --------------------------------------------------------------
PROV="$OUT_DIR/PROVENANCE.txt"
{
  echo "gnative provenance"
  echo "=================="
  echo "version      : $VERSION"
  if git rev-parse --git-dir >/dev/null 2>&1; then
    echo "commit       : $(git rev-parse HEAD)"
    echo "commit_date  : $(git show -s --format=%cI HEAD 2>/dev/null || echo '-')"
    echo "dirty        : $(if [ -n "$(git status --porcelain 2>/dev/null)" ]; then echo 'ya (working tree tidak bersih)'; else echo 'tidak'; fi)"
    echo "tag          : $(git describe --tags --always 2>/dev/null || echo '-')"
  else
    echo "commit       : (bukan git repository)"
  fi
  echo "generated_at : $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  echo "--- toolchain ---"
  if command -v rustc >/dev/null 2>&1; then
    rustc -vV | sed 's/^/rustc        : /'
  else
    echo "rustc        : tidak ditemukan"
  fi
  if command -v llvm-config >/dev/null 2>&1; then
    echo "llvm-config  : $(llvm-config --version)"
  elif command -v llvm-config-18 >/dev/null 2>&1; then
    echo "llvm-config  : $(llvm-config-18 --version) (llvm-config-18)"
  else
    echo "llvm-config  : tidak ditemukan"
  fi
  if command -v nvcc >/dev/null 2>&1; then
    echo "cuda (nvcc)  : $(nvcc --version | sed -n 's/.*release \([0-9.]*\).*/\1/p' | head -1)"
  else
    echo "cuda (nvcc)  : tidak ditemukan (backend CUDA di luar cakupan)"
  fi
  echo
  echo "--- artefak ---"
  if [ "${#FILES[@]}" -gt 0 ]; then
    printf '%s\n' "${FILES[@]}" | sed 's/^/artifact     : /'
  else
    echo "artifact     : (tidak ada; build release dulu)"
  fi
  if [ "${#MISSING[@]}" -gt 0 ]; then
    echo
    echo "--- artefak hilang (peringatan) ---"
    printf '%s\n' "${MISSING[@]}" | sed 's/^/missing      : /'
  fi
} > "$PROV"

echo "provenance: ditulis $SUMS ($(wc -l < "$SUMS" | tr -d ' ') berkas) dan $PROV"
if [ "${#MISSING[@]}" -gt 0 ]; then
  echo "provenance: ${#MISSING[@]} artefak tidak ditemukan — jalankan 'cargo build --release' bila perlu" >&2
fi
