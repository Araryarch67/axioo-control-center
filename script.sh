#!/usr/bin/env bash
# Build release + paketkan AppImage axioo-control-center.
#
#   ./script.sh                 # build + AppImage -> dist/
#   VERSION=1.0 ./script.sh     # nama file pakai versi sendiri
#   ./script.sh --no-appimage   # cuma cargo build --release
#
# Butuh: curl, python3 + Pillow (ikon), sudo TIDAK perlu untuk build.
# linuxdeploy/appimagetool diunduh sekali ke .tools/ (butuh internet).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="$HERE/dist"
TOOLS="$HERE/.tools"
APPID="axioo-control-center"
VERSION="${VERSION:-$(date +%Y.%m.%d)}"

mkdir -p "$OUT" "$TOOLS"

echo "==> cargo build --release"
cargo build --release -p axioo-gui -p axioo-ctl

if [ "${1:-}" = "--no-appimage" ]; then
    echo "binaries: $HERE/target/release/axioo-control-center $HERE/target/release/axioo-ctl"
    exit 0
fi

echo "==> icon"
python3 "$HERE/packaging/appimage/make_icon.py" "$OUT/icon.png"

fetch() { # url, dest
    if [ ! -x "$2" ]; then
        echo "download: $1"
        curl -fSL --retry 3 -o "$2" "$1"
        chmod +x "$2"
    fi
}
fetch "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage" \
    "$TOOLS/linuxdeploy-x86_64.AppImage"
fetch "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage" \
    "$TOOLS/appimagetool-x86_64.AppImage"
export PATH="$TOOLS:$PATH"
export APPIMAGE_EXTRACT_AND_RUN=1   # jalan tanpa FUSE

echo "==> AppDir"
APPDIR="$OUT/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" \
    "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps"
cp "$HERE/target/release/axioo-control-center" "$HERE/target/release/axioo-ctl" "$APPDIR/usr/bin/"
cp "$HERE/packaging/appimage/$APPID.desktop" "$APPDIR/usr/share/applications/"
cp "$HERE/packaging/appimage/$APPID.desktop" "$APPDIR/"
cp "$OUT/icon.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APPID.png"
cp "$OUT/icon.png" "$APPDIR/$APPID.png"
cp "$HERE/packaging/appimage/icon.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APPID.svg"

echo "==> AppImage"
export OUTPUT="$OUT/Axioo-Control-Center-$VERSION-x86_64.AppImage"
rm -f "$OUTPUT"
"$TOOLS/linuxdeploy-x86_64.AppImage" --appimage-extract-and-run \
    --appdir "$APPDIR" \
    -e "$APPDIR/usr/bin/axioo-control-center" \
    -d "$APPDIR/usr/share/applications/$APPID.desktop" \
    -i "$APPDIR/usr/share/icons/hicolor/256x256/apps/$APPID.png" \
    --output appimage
chmod +x "$OUTPUT"
echo "OK: $OUTPUT"
ls -la "$OUTPUT"
