#!/usr/bin/env bash
# Build release + paketkan AppImage Axioo Control Center (Tauri).
#
#   ./script.sh                 # progress bar diam (default)
#   ./script.sh --verbose     # tampilkan output mentah
#   ./script.sh --no-appimage   # cuma build (tanpa bundle)
#
# Butuh: node/npm, Rust, webkit2gtk (lihat tauri-app/dev.sh).
# sudo TIDAK perlu untuk build.
set -euo pipefail

QUIET=1
NO_APPIMAGE=0
for a in "$@"; do
    case "$a" in
        -v|--verbose) QUIET=0 ;;
        -q|--quiet|--silent) QUIET=1 ;; # kompat: kini default
        --no-appimage) NO_APPIMAGE=1 ;;
        -h|--help) echo "pakai: ./script.sh [--verbose] [--no-appimage]"; exit 0 ;;
    esac
done
log() { [ "$QUIET" = 1 ] || echo "$@"; }

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

log "==> frontend (vite build)"
if [ ! -d "$HERE/tauri-app/node_modules" ]; then
    if [ "$QUIET" = 1 ]; then
        (cd "$HERE/tauri-app" && npm install --no-audit --no-fund >/dev/null 2>&1)
    else
        (cd "$HERE/tauri-app" && npm install --no-audit --no-fund)
    fi
fi
if [ "$QUIET" = 1 ]; then
    (cd "$HERE/tauri-app" && npm run build >/dev/null 2>&1)
else
    (cd "$HERE/tauri-app" && npm run build)
fi

log "==> ikon hicolor (dist/icon.png dari icons/)"
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

log "==> cargo build --release (ctl + daemon + tauri)"
if [ "$QUIET" = 1 ]; then
    cargo build --release -p axioo-ctl -p axiood -p axioo-center >/dev/null 2>&1
else
    cargo build --release -p axioo-ctl -p axiood -p axioo-center
fi
log "binaries: $HERE/target/release/axioo-ctl $HERE/target/release/axiood $HERE/target/release/axioo-center"

if [ "$NO_APPIMAGE" = 1 ]; then
    exit 0
fi

log "==> AppImage (tauri bundle)"
mkdir -p "$OUT"
# NO_STRIP=1 (wajib di Arch modern): strip kuno bawaan linuxdeploy gagal
# pada library dengan section `.relr.dyn` (RELR relocs, default toolchain
# Arch) → "Strip call failed ... Unable to recognise the format" untuk
# SEMUA lib → bundle gagal. Tanpa strip AppImage sedikit lebih besar,
# fungsi identik. (Diverifikasi 2026-09-16.)
export NO_STRIP=1
if [ "$QUIET" = 1 ]; then
    (cd "$HERE/tauri-app" && npx tauri build --bundles appimage >/dev/null 2>&1)
else
    (cd "$HERE/tauri-app" && npx tauri build --bundles appimage)
fi
# Bundle dir = cargo target dir workspace (bukan src-tauri/target!):
# `cargo metadata` → target_directory = $HERE/target (workspace root).
if [ "$QUIET" = 1 ]; then
    ls "$HERE/target/release/bundle/appimage/"*.AppImage >/dev/null 2>&1 || true
else
    ls -la "$HERE/target/release/bundle/appimage/"*.AppImage 2>/dev/null || true
fi
if [ "$QUIET" = 1 ]; then
    echo "OK (appimage; VERSION=$VERSION)"
else
    echo "OK (appimage di atas; VERSION=$VERSION)"
fi
