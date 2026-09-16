import { api } from "./api";
import { hexToRgb } from "./utils";

/**
 * Theme "matugen": petakan ~/.cache/ryoku/colors.json ke CSS var app.
 * Kunci matugen adalah hex "#rrggbb"; var app adalah triplet "r g b".
 * Key yang hilang → fallback di dalam kurung (key lain, bukan warna mentah).
 */
const MAP: Array<[varName: string, key: string, fallback?: string]> = [
  ["--bg", "background"],
  ["--bg2", "surfaceContainerLow", "surface"],
  ["--card", "surfaceContainer", "surface"],
  ["--card2", "surfaceContainerHigh", "surfaceContainer"],
  ["--border", "outlineVariant", "surfaceVariant"],
  ["--hair", "outlineVariant", "surfaceVariant"],
  ["--ink", "onSurface"],
  ["--cream", "onSurface"],
  ["--dim", "onSurfaceVariant", "onSurface"],
  ["--faint", "outline"],
  ["--ghost", "surfaceVariant", "outlineVariant"],
  ["--accent", "primary"],
  ["--accent-ink", "onPrimary"],
  ["--lime", "tertiary", "secondary"],
  ["--ok", "tertiary", "primary"],
  ["--bad", "error"],
];

function triplet(hex: string | undefined): string | null {
  if (!hex || !/^#[0-9a-fA-F]{6}$/.test(hex)) return null;
  const [r, g, b] = hexToRgb(hex);
  return `${r} ${g} ${b}`;
}

/** Terapkan palet ke :root. Mengembalikan jumlah var yang kepasang. */
export function applyMatugen(palette: Record<string, string>): number {
  const root = document.documentElement;
  let n = 0;
  for (const [v, key, fb] of MAP) {
    const t = triplet(palette[key]) ?? (fb ? triplet(palette[fb]) : null);
    if (t) {
      root.style.setProperty(v, t);
      n++;
    }
  }
  return n;
}

/** Hapus override inline (kembali ke blok [data-theme] di CSS). */
export function clearMatugen() {
  const root = document.documentElement;
  for (const [v] of MAP) root.style.removeProperty(v);
}

/**
 * Fetch palet via backend lalu terapkan. true bila accent kepasang
 * (kunci minimal agar theme layak); false → pemanggil pakai fallback CSS.
 */
export async function refreshMatugen(): Promise<boolean> {
  try {
    const pal = await api.matugen();
    const n = applyMatugen(pal);
    return triplet(pal["primary"]) != null && n > 0;
  } catch {
    return false;
  }
}

/** Warna paling vivid matugen — untuk LED (warna muted kelihatan mati). */
const VIVID_KEYS = [
  "primary", "secondary", "tertiary",
  "color1", "color2", "color3", "color4", "color5", "color6",
  "color9", "color10", "color11", "color12", "color13", "color14",
];

/** Skor vivid: saturasi × value (0–1). Putih/abu → 0, warna gelap → kecil. */
function vividScore([r, g, b]: [number, number, number]): number {
  const mx = Math.max(r, g, b) / 255;
  const mn = Math.min(r, g, b) / 255;
  if (mx === 0) return 0;
  return ((mx - mn) / mx) * mx;
}

export async function matugenVivid(): Promise<[number, number, number] | null> {
  try {
    const pal = await api.matugen();
    let best: [number, number, number] | null = null;
    let bestScore = 0.05; // di bawah ini = palet abu semua → fallback primary
    for (const k of VIVID_KEYS) {
      const h = pal[k];
      if (!h || !/^#[0-9a-fA-F]{6}$/.test(h)) continue;
      const c = hexToRgb(h);
      const s = vividScore(c);
      if (s > bestScore) {
        bestScore = s;
        best = c;
      }
    }
    if (best) return best;
    const p = pal["primary"];
    if (p && /^#[0-9a-fA-F]{6}$/.test(p)) return hexToRgb(p);
    return null;
  } catch {
    return null;
  }
}
