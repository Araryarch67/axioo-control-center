#!/usr/bin/env bash
# Build release + paketkan AppImage Axioo Control Center (Tauri).
#
#   ./script.sh                 # frontend + binari release + AppImage -> dist/
#   ./script.sh --no-appimage   # cuma build (tanpa bundle)
#
# Butuh: node/npm, Rust, webkit2gtk (lihat tauri-app/dev.sh).
# sudo TIDAK perlu untuk build.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/dist"
VERSION="${VERSION:-$(date +%Y.%m.%d)}"

need() { command -v "$1" >/dev/null 2>&1 || { echo "butuh '$1' — jalankan ./setup.sh dulu (install deps)"; exit 1; }; }
need node
need npm
need cargo
if command -v pkg-config >/dev/null 2>&1; then
    for p in webkit2gtk-4.1 gtk+-3.0; do
        pkg-config --exists "$p" || { echo "butuh lib $p — jalankan ./setup.sh dulu (install deps)"; exit 1; }
    done
fi

echo "==> frontend (vite build)"
if [ ! -d "$HERE/tauri-app/node_modules" ]; then
    (cd "$HERE/tauri-app" && npm install --no-audit --no-fund)
fi
(cd "$HERE/tauri-app" && npm run build)

echo "==> ikon hicolor (dist/icon.png dari icons/)"
# dist/icon.png dipakai setup.sh untuk hicolor launcher — generate di sini
# agar selalu ada habis build bersih (uninstall menghapusnya).
if [ -f "$HERE/icons/android-chrome-512x512.png" ]; then
    if python3 -c "import PIL.Image" 2>/dev/null; then
        python3 - "$HERE/icons/android-chrome-512x512.png" "$OUT/icon.png" <<'EOF'
import sys
from PIL import Image
Image.open(sys.argv[1]).convert("RGBA").resize((256, 256), Image.LANCZOS).save(sys.argv[2])
EOF
    else
        cp "$HERE/icons/android-chrome-512x512.png" "$OUT/icon.png"
    fi
fi

echo "==> cargo build --release (ctl + daemon + tauri)"
cargo build --release -p axioo-ctl -p axiood -p axioo-center
echo "binaries: $HERE/target/release/axioo-ctl $HERE/target/release/axiood $HERE/target/release/axioo-center"

if [ "${1:-}" = "--no-appimage" ]; then
    exit 0
fi

echo "==> AppImage (tauri bundle)"
mkdir -p "$OUT"
# NO_STRIP=1 (wajib di Arch modern): strip kuno bawaan linuxdeploy gagal
# pada library dengan section `.relr.dyn` (RELR relocs, default toolchain
# Arch) → "Strip call failed ... Unable to recognise the format" untuk
# SEMUA lib → bundle gagal. Tanpa strip AppImage sedikit lebih besar,
# fungsi identik. (Diverifikasi 2026-09-16.)
export NO_STRIP=1
(cd "$HERE/tauri-app" && npx tauri build --bundles appimage)
# Bundle dir = cargo target dir workspace (bukan src-tauri/target!):
# `cargo metadata` → target_directory = $HERE/target (workspace root).
ls -la "$HERE/target/release/bundle/appimage/"*.AppImage 2>/dev/null || true
echo "OK (appimage di atas; VERSION=$VERSION)"
