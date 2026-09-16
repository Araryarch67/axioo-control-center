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

/** Warna dominan matugen (primary) yang dicerahkan untuk LED. */
function rgbToHsv(r: number, g: number, b: number): [number, number, number] {
  r /= 255; g /= 255; b /= 255;
  const mx = Math.max(r, g, b), mn = Math.min(r, g, b);
  const d = mx - mn;
  const h = d === 0 ? 0
    : mx === r ? (((g - b) / d) % 6 + 6) % 6 / 6
    : mx === g ? (b - r) / d / 6 + 1 / 3
    : (r - g) / d / 6 + 2 / 3;
  return [h, mx === 0 ? 0 : d / mx, mx];
}

function hsvToRgb(h: number, s: number, v: number): [number, number, number] {
  const f = (n: number) => {
    const k = (n + h * 6) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [Math.round(f(5) * 255), Math.round(f(3) * 255), Math.round(f(1) * 255)];
}

/** Cerahkan: value penuh; warna → saturasi vivid; abu (tak ber-hue) → putih. */
function brightenLed([r, g, b]: [number, number, number]): [number, number, number] {
  const [h, s] = rgbToHsv(r, g, b);
  return hsvToRgb(h, s < 0.03 ? 0 : Math.max(s, 0.7), 1);
}

export async function matugenDominant(): Promise<[number, number, number] | null> {
  try {
    const pal = await api.matugen();
    for (const k of ["primary", "secondary", "tertiary"]) {
      const h = pal[k];
      if (h && /^#[0-9a-fA-F]{6}$/.test(h)) return brightenLed(hexToRgb(h));
    }
    return null;
  } catch {
    return null;
  }
}
