#!/usr/bin/env bash
# install.sh — alias pendek untuk setup.sh (installer end-to-end).
#   ./install.sh [args...]   ≡   ./setup.sh [args...]
# Pasangannya: ./uninstall.sh (bersih total).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
exec "$HERE/setup.sh" "$@"
