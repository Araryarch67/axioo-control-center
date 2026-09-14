/**
 * Cermin JS dari `axioo_lib::kbd_effect::tick` — dipakai agar visual GUI
 * beranimasi dengan matematika yang SAMA seperti thread hardware
 * (fase boleh beda, bentuk gelombang identik).
 *
 * JANGAN ubah konstanta di sini tanpa mengubah padanannya di Rust.
 */

export type Rgb = [number, number, number];

export function hsvToRgb(h: number, s: number, v: number): Rgb {
  const hh = ((h % 360) + 360) % 360;
  const ss = Math.min(1, Math.max(0, s));
  const vv = Math.min(1, Math.max(0, v));
  const c = vv * ss;
  const x = c * (1 - Math.abs(((hh / 60) % 2) - 1));
  const m = vv - c;
  const seg = Math.floor(hh / 60);
  let r1 = 0, g1 = 0, b1 = 0;
  if (seg === 0) { r1 = c; g1 = x; }
  else if (seg === 1) { r1 = x; g1 = c; }
  else if (seg === 2) { g1 = c; b1 = x; }
  else if (seg === 3) { g1 = x; b1 = c; }
  else if (seg === 4) { r1 = x; b1 = c; }
  else { r1 = c; b1 = x; }
  const q = (n: number) => Math.min(255, Math.max(0, Math.round((n + m) * 255)));
  return [q(r1), q(g1), q(b1)];
}

const lerp = (a: number, b: number, k: number) => Math.min(255, Math.max(0, Math.round(a * (1 - k) + b * k)));

const CYCLE: Rgb[] = [[255, 0, 0], [255, 255, 0], [0, 255, 0], [0, 255, 255], [0, 0, 255], [255, 0, 255]];

/** Satu tick → warna per zona. `t` detik (sudah dikali speed oleh pemanggil). */
export function tickEffect(effect: string, base: Rgb, t: number, nzones: number): Rgb[] {
  const n = Math.max(1, nzones);
  const scale = (f: number): Rgb => [
    Math.round(base[0] * f), Math.round(base[1] * f), Math.round(base[2] * f),
  ];
  switch (effect) {
    case "breathing": {
      const f = (Math.sin(t * 3.8) + 1) / 2;
      return Array(n).fill(scale(0.2 + 0.8 * f));
    }
    case "wave":
      return Array.from({ length: n }, (_, i) => {
        const h = (((t * 40 + i * 70) % 360) + 360) % 360;
        return hsvToRgb(h, 1, 1);
      });
    case "rainbow": {
      const h = (((t * 45) % 360) + 360) % 360;
      return Array(n).fill(hsvToRgb(h, 1, 1));
    }
    case "cycle": {
      const period = 2;
      const idx = Math.floor(t / period) % CYCLE.length;
      const frac = (t % period) / period;
      if (frac < 0.85) return Array(n).fill([...CYCLE[idx]] as Rgb);
      const nxt = CYCLE[(idx + 1) % CYCLE.length];
      const k = (frac - 0.85) / 0.15;
      const c = CYCLE[idx];
      return Array(n).fill([lerp(c[0], nxt[0], k), lerp(c[1], nxt[1], k), lerp(c[2], nxt[2], k)]);
    }
    case "aurora":
      return Array.from({ length: n }, (_, i) => {
        const h = (((t * 15 + i * 35 + Math.sin(t * 0.7) * 20) % 360) + 360) % 360;
        return hsvToRgb(h, 0.65, 1);
      });
    case "twinkle":
      return Array.from({ length: n }, (_, i) => {
        const bucket = BigInt(Math.floor(t * 2.5));
        const h = Number((bucket * 6364136223846793005n + BigInt(i) * 1442695040888963407n) % 360n);
        const on = (Math.sin(t * 6 + i * 1.3) * 0.5 + 0.5) > 0.72;
        return on ? hsvToRgb(h, 0.9, 1) : scale(0.18);
      });
    case "pulse": {
      const phase = ((t * 1.8) % (Math.PI * 2) + Math.PI * 2) % (Math.PI * 2);
      const beat = Math.min(1, Math.max(0,
        Math.pow(Math.sin(phase), 2) * 0.6 + Math.pow(Math.sin(phase * 2), 8) * 0.4));
      return Array(n).fill(scale(0.15 + 0.85 * beat));
    }
    case "gradient":
      return Array.from({ length: n }, (_, i) => hsvToRgb((i / n) * 300, 1, 1));
    case "music": {
      const beat = Math.min(1, Math.max(0, Math.abs(
        Math.sin(t * 6) * 0.5 + Math.sin(t * 11.3) * 0.3 + Math.sin(t * 2.1) * 0.2)));
      const k = 0.35 + 0.65 * Math.pow(beat, 1.6);
      const h = (((t * 28 + beat * 60) % 360) + 360) % 360;
      const c = hsvToRgb(h, 0.9, 1);
      return Array(n).fill([Math.round(c[0] * k), Math.round(c[1] * k), Math.round(c[2] * k)]);
    }
    case "spectrum":
      return Array.from({ length: n }, (_, i) => {
        const freq = 4 + i * 5.5;
        const band = Math.pow(Math.sin(t * freq) * 0.5 + 0.5, 1.2);
        const h = (((i * 65 + t * 18) % 360) + 360) % 360;
        return hsvToRgb(h, 0.95, 0.4 + 0.6 * band);
      });
    case "reactive":
      return Math.sin(t * 3.5) > 0.92
        ? Array(n).fill([255, 255, 255] as Rgb)
        : Array(n).fill([...base] as Rgb);
    default:
      return Array(n).fill([...base] as Rgb);
  }
}
