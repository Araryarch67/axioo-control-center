import * as React from "react";
import { curveDuty } from "@/lib/utils";

const W = 560;
const H = 230;
const PAD = 30;
const T_MAX = 100;

/** Cockpit fan-curve editor. Disabled (display only) when daemon owns it. */
export function FanCurveEditor({ points, liveTemp, disabled, onChange }: {
  points: Array<[number, number]>;
  liveTemp: number | null;
  disabled?: boolean;
  onChange: (pts: Array<[number, number]>) => void;
}) {
  const ref = React.useRef<SVGSVGElement>(null);
  const [drag, setDrag] = React.useState<number | null>(null);
  const sorted = React.useMemo(() => [...points].sort((a, b) => a[0] - b[0]), [points]);
  const x = (t: number) => PAD + (t / T_MAX) * (W - 2 * PAD);
  const y = (d: number) => H - PAD - (d / 100) * (H - 2 * PAD);
  const toVal = (px: number, py: number): [number, number] => {
    const t = Math.round(((px - PAD) / (W - 2 * PAD)) * T_MAX);
    const d = Math.round(((H - PAD - py) / (H - 2 * PAD)) * 100);
    return [Math.min(100, Math.max(20, t)), Math.min(100, Math.max(40, d))];
  };
  const pos = (e: React.PointerEvent) => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return [0, 0] as const;
    return [(e.clientX - r.left) * (W / r.width), (e.clientY - r.top) * (H / r.height)] as const;
  };
  const path = sorted.map((p, i) => `${i === 0 ? "M" : "L"}${x(p[0])},${y(p[1])}`).join(" ");
  const liveDuty = liveTemp != null ? curveDuty(sorted, liveTemp) : null;
  const grid = [];
  for (let t = 20; t <= 100; t += 20) {
    grid.push(<line key={`v${t}`} x1={x(t)} y1={PAD} x2={x(t)} y2={H - PAD} stroke="rgb(var(--hair))" strokeWidth={1} />);
    grid.push(<text key={`vt${t}`} x={x(t)} y={H - 8} textAnchor="middle" fontSize={9} fill="rgb(var(--faint))" fontFamily="monospace">{t}°</text>);
  }
  for (let d = 40; d <= 100; d += 20) {
    grid.push(<line key={`h${d}`} x1={PAD} y1={y(d)} x2={W - PAD} y2={y(d)} stroke="rgb(var(--hair))" strokeWidth={1} />);
    grid.push(<text key={`ht${d}`} x={10} y={y(d) + 3} fontSize={9} fill="rgb(var(--faint))" fontFamily="monospace">{d}%</text>);
  }
  return (
    <div>
      <svg ref={ref} viewBox={`0 0 ${W} ${H}`}
        className={`w-full touch-none select-none rounded-lg border-2 border-black bg-bg ${disabled ? "opacity-80" : "cursor-crosshair"}`}
        onPointerMove={(e) => {
          if (drag == null || disabled) return;
          const [px, py] = pos(e);
          const [t, d] = toVal(px, py);
          onChange(sorted.map((p, i) => (i === drag ? ([t, d] as [number, number]) : p)));
        }}
        onPointerUp={() => setDrag(null)}
        onPointerLeave={() => setDrag(null)}>
        {grid}
        <path d={`${path} L${x(sorted[sorted.length - 1]?.[0] ?? 100)},${H - PAD} L${x(sorted[0]?.[0] ?? 20)},${H - PAD} Z`}
          fill="rgb(var(--accent))" opacity={0.08} />
        <path d={path} fill="none" stroke="rgb(var(--accent))" strokeWidth={2.5} strokeLinejoin="round" />
        {liveTemp != null && (
          <line x1={x(liveTemp)} y1={PAD} x2={x(liveTemp)} y2={H - PAD}
            stroke="rgb(var(--cream))" strokeWidth={1} strokeDasharray="3 3" opacity={0.6} />
        )}
        {sorted.map((p, i) => (
          <g key={i}>
            <circle cx={x(p[0])} cy={y(p[1])} r={13} fill="transparent" />
            <circle cx={x(p[0])} cy={y(p[1])} r={6}
              fill={drag === i ? "rgb(var(--accent))" : "rgb(var(--bg2))"}
              stroke="rgb(var(--accent))" strokeWidth={2}
              className={disabled ? "" : "cursor-grab"}
              onPointerDown={(e) => {
                if (disabled) return;
                (e.target as Element).setPointerCapture?.(e.pointerId);
                setDrag(i);
              }} />
          </g>
        ))}
      </svg>
      <div className="num mt-1.5 flex justify-between text-[10.5px] text-faint">
        <span>drag points · 20–100°C / 40–100%</span>
        {liveTemp != null && liveDuty != null && <span className="text-accent">live {liveTemp}°C → {liveDuty}%</span>}
      </div>
    </div>
  );
}
