import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function clamp(n: number, lo: number, hi: number) {
  return Math.min(hi, Math.max(lo, n));
}

export function fmt1(v: number | null | undefined, unit = ""): string {
  if (v == null || Number.isNaN(v)) return "-";
  return `${v.toFixed(1)}${unit}`;
}

export function rgbToHex(r: number, g: number, b: number): string {
  const h = (n: number) => clamp(Math.round(n), 0, 255).toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

export function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "").trim();
  if (h.length !== 6) return [255, 255, 255];
  const n = parseInt(h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Interpolasi duty dari kurva (port `fan::curve_duty`). */
export function curveDuty(points: Array<[number, number]>, tempC: number): number {
  if (points.length === 0) return 40;
  const pts = [...points].sort((a, b) => a[0] - b[0]);
  if (tempC <= pts[0][0]) return pts[0][1];
  for (let i = 1; i < pts.length; i++) {
    if (tempC <= pts[i][0]) {
      const [t0, d0] = pts[i - 1];
      const [t1, d1] = pts[i];
      if (t1 === t0) return d1;
      const f = (tempC - t0) / (t1 - t0);
      return Math.round(d0 + f * (d1 - d0));
    }
  }
  return pts[pts.length - 1][1];
}
