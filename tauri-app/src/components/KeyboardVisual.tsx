import * as React from "react";

type ZoneColor = { rgb: [number, number, number]; bright: number };

const U = 46;          // 1u key size (px in viewBox)
const GAP = 6;
const H = 46;
const MAIN_W = 15;     // main block = 15u wide
const NUM_X = MAIN_W * (U + GAP) + 28; // numpad offset

type Key = { label: string; w?: number; h?: number; x?: number; y?: number; zone?: number };

const MAIN: Key[][] = [
  [{ label: "Esc" }, { label: "F1" }, { label: "F2" }, { label: "F3" }, { label: "F4" },
   { label: "F5" }, { label: "F6" }, { label: "F7" }, { label: "F8" },
   { label: "F9" }, { label: "F10" }, { label: "F11" }, { label: "F12" },
   { label: "Prt" }, { label: "Del" }],
  [{ label: "`" }, { label: "1" }, { label: "2" }, { label: "3" }, { label: "4" },
   { label: "5" }, { label: "6" }, { label: "7" }, { label: "8" }, { label: "9" },
   { label: "0" }, { label: "-" }, { label: "=" }, { label: "⌫", w: 2 }],
  [{ label: "Tab", w: 1.5 }, { label: "Q" }, { label: "W" }, { label: "E" }, { label: "R" },
   { label: "T" }, { label: "Y" }, { label: "U" }, { label: "I" }, { label: "O" },
   { label: "P" }, { label: "[" }, { label: "]" }, { label: "\\", w: 1.5 }],
  [{ label: "Caps", w: 1.75 }, { label: "A" }, { label: "S" }, { label: "D" }, { label: "F" },
   { label: "G" }, { label: "H" }, { label: "J" }, { label: "K" }, { label: "L" },
   { label: ";" }, { label: "'" }, { label: "Enter", w: 2.25 }],
  [{ label: "Shift", w: 2.25 }, { label: "Z" }, { label: "X" }, { label: "C" }, { label: "V" },
   { label: "B" }, { label: "N" }, { label: "M" }, { label: "," }, { label: "." },
   { label: "/" }, { label: "Shift", w: 2.75 }],
  [{ label: "Ctrl", w: 1.25 }, { label: "Win", w: 1.25 }, { label: "Alt", w: 1.25 },
   { label: "", w: 6.25 }, { label: "Alt", w: 1.25 }, { label: "Fn", w: 1.25 },
   { label: "←" }, { label: "↑↓" }, { label: "→" }],
];

const NUMPAD: Key[][] = [
  [{ label: "Num" }, { label: "/" }, { label: "*" }, { label: "-" }],
  [{ label: "7" }, { label: "8" }, { label: "9" }, { label: "+", h: 2 }],
  [{ label: "4" }, { label: "5" }, { label: "6" }],
  [{ label: "1" }, { label: "2" }, { label: "3" }, { label: "Ent", h: 2 }],
  [{ label: "0", w: 2 }, { label: "." }],
];

/** Peta kolom → zona (sepertiga kiri/tengah/kanan). */
function colZone(frac: number, zones: number): number {
  if (zones <= 1) return 0;
  if (zones === 2) return frac < 0.5 ? 0 : 1;
  return frac < 0.34 ? 0 : frac < 0.67 ? 1 : 2;
}

export function KeyboardVisual({ zones, maxBright, selected, onSelect }: {
  /** Live dari snapshot: [(brightness, [r,g,b])] sesuai urutan discover. */
  zones: Array<[number, [number, number, number]]>;
  maxBright: number;
  selected: number | null;
  onSelect: (z: number | null) => void;
}) {
  const n = Math.max(zones.length, 1);
  const colors: ZoneColor[] = Array.from({ length: Math.max(n, 4) }, (_, i) => {
    const z = zones[i % n];
    return { rgb: z ? z[1] : [60, 55, 45], bright: z ? z[0] : 0 };
  });

  const render = (rows: Key[][], ox: number, forceZone?: number) => {
    const els: React.ReactNode[] = [];
    rows.forEach((row, ri) => {
      let cx = ox;
      // baris numpad dengan key tinggi (h:2) butuh skip sel di baris berikut
      row.forEach((k, ki) => {
        const w = (k.w ?? 1) * U + ((k.w ?? 1) - 1) * GAP;
        const h = (k.h ?? 1) * H + ((k.h ?? 1) - 1) * GAP;
        const y = ri * (H + GAP);
        const frac = (cx - ox + w / 2) / (MAIN_W * (U + GAP));
        const zi = forceZone ?? colZone(frac, Math.min(n, 3));
        const c = colors[zi % colors.length];
        const s = maxBright > 0 ? c.bright / maxBright : 0;
        const [r, g, b] = c.rgb.map((v) => Math.round(10 + (v - 10) * Math.max(s, 0.06)));
        const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
        const sel = selected === null || selected === zi;
        els.push(
          <g key={`${ri}-${ki}-${k.label}`} opacity={sel ? 1 : 0.35}
            onClick={() => onSelect(selected === zi ? null : zi)}
            className="cursor-pointer">
            <rect x={cx} y={y} width={w} height={h} rx={7}
              fill={`rgb(${r},${g},${b})`}
              stroke={selected === zi ? "rgb(var(--accent))" : "#000"}
              strokeWidth={selected === zi ? 3 : 1.5}
              style={selected === zi
                ? { filter: "drop-shadow(0 0 8px rgb(var(--accent) / 0.7))" }
                : { filter: `drop-shadow(0 0 ${4 + s * 10}px rgb(${r},${g},${b})` + ` / ${0.25 + s * 0.5}))` }} />
            <text x={cx + w / 2} y={y + h / 2 + 4} textAnchor="middle" fontSize={13}
              fontWeight={700} fill={lum > 0.55 ? "#14110b" : "#f5efe0"}
              fontFamily="'Iosevka Nerd Font', monospace" fontStyle="italic">
              {k.label}
            </text>
          </g>,
        );
        cx += w + GAP;
      });
    });
    return els;
  };

  // Numpad: key h:2 digambar setinggi 2 baris; baris di bawahnya
  // menyisakan kolom itu kosong sehingga tidak overlap.
  const renderNumpad = () => {
    const els: React.ReactNode[] = [];
    NUMPAD.forEach((row, ri) => {
      let cx = NUM_X;
      const y = ri * (H + GAP);
      row.forEach((k, ki) => {
        const w = (k.w ?? 1) * U + ((k.w ?? 1) - 1) * GAP;
        const h = (k.h ?? 1) * H + ((k.h ?? 1) - 1) * GAP;
        const zi = 3;
        const c = colors[zi % colors.length];
        const s = maxBright > 0 ? c.bright / maxBright : 0;
        const [r, g, b] = c.rgb.map((v) => Math.round(10 + (v - 10) * Math.max(s, 0.06)));
        const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
        const sel = selected === null || selected === zi;
        els.push(
          <g key={`n${ri}-${ki}`} opacity={sel ? 1 : 0.35}
            onClick={() => onSelect(selected === zi ? null : zi)} className="cursor-pointer">
            <rect x={cx} y={y} width={w} height={h} rx={7}
              fill={`rgb(${r},${g},${b})`} stroke={selected === zi ? "rgb(var(--accent))" : "#000"}
              strokeWidth={selected === zi ? 3 : 1.5} />
            <text x={cx + w / 2} y={y + Math.min(h, H) / 2 + 4} textAnchor="middle" fontSize={13}
              fontWeight={700} fill={lum > 0.55 ? "#14110b" : "#f5efe0"}
              fontFamily="'Iosevka Nerd Font', monospace" fontStyle="italic">
              {k.label}
            </text>
          </g>,
        );
        cx += w + GAP;
      });
    });
    return els;
  };

  const totalW = NUM_X + 4 * U + 3 * GAP;
  const totalH = 6 * H + 5 * GAP;
  return (
    <svg viewBox={`-8 -8 ${totalW + 16} ${totalH + 16}`} className="w-full select-none">
      {render(MAIN, 0)}
      {renderNumpad()}
    </svg>
  );
}
