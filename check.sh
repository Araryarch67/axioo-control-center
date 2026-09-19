#!/usr/bin/env bash
# Gerbang lokal (tanpa CI cloud): fmt + clippy + unit test + tsc.
# Jalankan sebelum klaim "hijau":  ./check.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

echo "==> fmt"
cargo fmt --check --all

echo "==> clippy (-D warnings)"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> unit test"
cargo test -p axioo-lib -p axiood -p axioo-ctl -p axioo-center

echo "==> frontend types"
(cd "$HERE/tauri-app" && bunx tsc --noEmit)

echo "OK: fmt + clippy + test + tsc hijau."
