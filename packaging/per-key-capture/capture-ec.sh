#!/usr/bin/env bash
# capture-ec.sh — snapshot read-only EC + keyboard (diff sebelum/sesudah).
#
# Hanya MEMBACA: axioo-ctl probe / fan dump / kbd status+get + hexdump ec_sys.
# Tidak pernah menulis EC (aturan safety: tulis EC one-shot hanya via fan_ctrl,
# loop kontinu hanya milik axiood).
#
# Dibuat untuk riset per-key RGB — riset itu DITUTUP 2026-09-16 (Control
# Center Windows mesin ini tak punya mode per-key, lihat docs/per-key-rgb.md).
# Script tetap berguna sebagai alat snapshot umum, mis. bandingkan peta EC
# idle vs full-load, atau sebelum/sesudah ganti mode Control Center:
#   1. ./capture-ec.sh before
#   2. ...lakukan perubahan (profil, efek keyboard, beban, dsb)...
#   3. ./capture-ec.sh after
#   4. ./capture-ec.sh diff
#
# Output: ./captures/<label>-<timestamp>/
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
CTL="$ROOT/target/debug/axioo-ctl"
OUT_BASE="$HERE/captures"

usage() {
  echo "usage: $0 before|after|diff [label]" >&2
  echo "  before|after  simpan snapshot read-only ke captures/" >&2
  echo "  diff          bandingkan 2 snapshot terbaru (before vs after)" >&2
  exit 1
}

[ $# -ge 1 ] || usage
MODE="$1"
LABEL="${2:-manual}"

snap() {
  local label="$1"
  local dir="$OUT_BASE/${label}-$(date +%Y%m%d-%H%M%S)"
  mkdir -p "$dir"
  echo "capturing -> $dir"

  if [ -x "$CTL" ]; then
    "$CTL" probe > "$dir/probe.txt" 2>&1 || echo "probe gagal (rc=$?)" > "$dir/probe.txt"
    if [ "$(id -u)" -eq 0 ]; then
      "$CTL" fan dump > "$dir/fan-dump.txt" 2>&1 || true
    else
      { echo "# butuh root: sudo $CTL fan dump"; sudo "$CTL" fan dump 2>&1 || echo "fan dump gagal"; } > "$dir/fan-dump.txt" || true
    fi
    "$CTL" kbd status > "$dir/kbd-status.txt" 2>&1 || true
    "$CTL" kbd get > "$dir/kbd-get.txt" 2>&1 || true
  else
    echo "axioo-ctl tidak ada di $CTL — cargo build -p axioo-ctl dulu" | tee "$dir/ERROR.txt"
  fi

  # Raw 256-byte EC map bila terbaca (read-only xxd).
  if [ -r /sys/kernel/debug/ec/ec0/io ]; then
    xxd /sys/kernel/debug/ec/ec0/io > "$dir/ec-raw.hex" 2>&1 || true
  else
    echo "# /sys/kernel/debug/ec/ec0/io tak terbaca (butuh: sudo modprobe ec_sys + root)" > "$dir/ec-raw.hex"
  fi

  dmidecode -t baseboard 2>/dev/null | head -n 12 > "$dir/dmi-board.txt" || echo "# dmidecode butuh root" > "$dir/dmi-board.txt"
  ls /sys/class/leds/ > "$dir/leds.txt" 2>&1 || true
  date -u > "$dir/timestamp-utc.txt"
  echo "OK: $dir"
}

dodiff() {
  local a b
  a="$(ls -dt "$OUT_BASE"/*/ 2>/dev/null | head -1)"
  b="$(ls -dt "$OUT_BASE"/*/ 2>/dev/null | head -2 | tail -1)"
  [ -n "${a:-}" ] && [ -n "${b:-}" ] || { echo "butuh >=2 snapshot di $OUT_BASE" >&2; exit 1; }
  echo "=== diff $b -> $a ==="
  for f in probe.txt fan-dump.txt kbd-status.txt kbd-get.txt ec-raw.hex leds.txt; do
    echo "--- $f ---"
    diff -u "$b/$f" "$a/$f" 2>&1 | head -n 100 || true
  done
}

case "$MODE" in
  before|after) snap "$MODE-${LABEL}" ;;
  diff) dodiff ;;
  *) usage ;;
esac
