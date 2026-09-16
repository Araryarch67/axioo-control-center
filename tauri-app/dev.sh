#!/usr/bin/env bash
# dev.sh — runner Frontend (Vite :1420) + Backend (Tauri axioo-center).
#
#   ./dev.sh          # FE (background) + BE (foreground), Ctrl-C matikan semua
#   ./dev.sh --fe     # hanya Vite dev server
#   ./dev.sh --be     # hanya backend (butuh Vite sudah jalan di :1420)
#   ./dev.sh --build  # build prod FE + BE, tanpa jalan
#
# Catatan: app jalan sebagai user (bukan root). Tulis EC langsung butuh
# root/axiood — lihat toast di UI kalau tombol manual disabled.

set -euo pipefail
SCRIPT="$(readlink -f "$0")"
cd "$(dirname "$SCRIPT")"

VITE_PORT=1420
VITE_PID=""

log() { printf '[dev.sh] %s\n' "$*"; }

cleanup() {
  if [[ -n "$VITE_PID" ]] && kill -0 "$VITE_PID" 2>/dev/null; then
    log "mematikan vite (pid $VITE_PID)"
    kill "$VITE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

ensure_deps() {
  if [[ ! -d node_modules ]]; then
    log "node_modules belum ada — npm install…"
    npm install --no-audit --no-fund
  fi
  if [[ ! -x node_modules/.bin/vite ]]; then
    log "vite tidak ditemukan — npm install…"
    npm install --no-audit --no-fund
  fi
}

port_open() {
  if command -v curl >/dev/null 2>&1; then
    curl -sf -o /dev/null -m 1 "http://127.0.0.1:$VITE_PORT/" 2>/dev/null
  else
    (echo > "/dev/tcp/127.0.0.1/$VITE_PORT") 2>/dev/null
  fi
}

start_vite_bg() {
  if port_open; then
    log "port $VITE_PORT sudah dipakai — pakai server yang ada."
    return 0
  fi
  log "menyalakan vite di 127.0.0.1:$VITE_PORT…"
  # --host 127.0.0.1: paksa IPv4 loopback (default vite bisa jatuh ke ::1 saja).
  ./node_modules/.bin/vite --host 127.0.0.1 --port "$VITE_PORT" --strictPort > /tmp/axioo-vite.log 2>&1 &
  VITE_PID="$!"
  for _ in $(seq 1 50); do
    port_open && { log "vite siap (pid $VITE_PID)."; return 0; }
    kill -0 "$VITE_PID" 2>/dev/null || { log "vite gagal start — lihat /tmp/axioo-vite.log"; return 1; }
    sleep 0.2
  done
  log "vite timeout — lihat /tmp/axioo-vite.log"
  return 1
}

run_fe() {
  ensure_deps
  log "vite foreground di 127.0.0.1:$VITE_PORT (Ctrl-C untuk berhenti)"
  exec ./node_modules/.bin/vite --host 127.0.0.1 --port "$VITE_PORT" --strictPort
}

run_be() {
  if ! port_open; then
    log "ERROR: vite belum jalan di :$VITE_PORT — jalankan './dev.sh' atau './dev.sh --fe' dulu."
    return 1
  fi
  log "menjalankan backend (cargo run -p axioo-center)…"
  exec cargo run -p axioo-center --manifest-path ../Cargo.toml
}

case "${1:---all}" in
  --fe) run_fe ;;
  --be) run_be ;;
  --build)
    ensure_deps
    log "build frontend…"
    npm run build
    log "build backend…"
    cargo build -p axioo-center --manifest-path ../Cargo.toml
    log "selesai: ../target/debug/axioo-center + ./dist"
    ;;
  --all | "")
    ensure_deps
    start_vite_bg
    log "menjalankan backend di foreground (Ctrl-C untuk berhenti)…"
    cargo run -p axioo-center --manifest-path ../Cargo.toml
    ;;
  -h | --help)
    sed -n '2,12p' "$SCRIPT"
    ;;
  *)
    log "flag tak dikenal: $1 (lihat --help)"
    exit 1
    ;;
esac
