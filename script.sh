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

echo "==> frontend (vite build)"
if [ ! -d "$HERE/tauri-app/node_modules" ]; then
    (cd "$HERE/tauri-app" && npm install --no-audit --no-fund)
fi
(cd "$HERE/tauri-app" && npm run build)

echo "==> cargo build --release (ctl + daemon + tauri)"
cargo build --release -p axioo-ctl -p axiood -p axioo-center
echo "binaries: $HERE/target/release/axioo-ctl $HERE/target/release/axiood $HERE/target/release/axioo-center"

if [ "${1:-}" = "--no-appimage" ]; then
    exit 0
fi

echo "==> AppImage (tauri bundle)"
mkdir -p "$OUT"
(cd "$HERE/tauri-app" && npx tauri build --bundles appimage)
ls -la "$HERE/tauri-app/src-tauri/target/release/bundle/appimage/"*.AppImage 2>/dev/null || true
echo "OK (appimage di atas; VERSION=$VERSION)"
