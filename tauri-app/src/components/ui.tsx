import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, children }: { className?: string; children: React.ReactNode }) {
  return <div className={cn("card p-5", className)}>{children}</div>;
}

export function CardTitle({ icon, children, right }: {
  icon?: React.ReactNode; children: React.ReactNode; right?: React.ReactNode;
}) {
  return (
    <div className="mb-4 flex items-center justify-between gap-2">
      <div className="card-title">{icon}{children}</div>
      {right}
    </div>
  );
}

type BtnVariant = "accent" | "ghost" | "danger";

export function CButton({ variant = "ghost", className, ...props }:
  React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: BtnVariant }) {
  return (
    <button
      className={cn(
        "cbtn",
        variant === "accent" && "cbtn-accent",
        variant === "danger" && "cbtn-danger",
        className,
      )}
      {...props}
    />
  );
}

export function Chip({ on, color, children, className }: {
  on?: boolean; color?: "ok" | "bad" | "accent"; children: React.ReactNode; className?: string;
}) {
  const c = on
    ? color === "ok" ? "rgb(var(--ok))" : color === "bad" ? "rgb(var(--bad))" : "rgb(var(--accent))"
    : "rgb(var(--ghost))";
  return (
    <span className={cn("chip", className)}>
      <span className="cdot" style={{ background: c, boxShadow: on ? `0 0 7px ${c}` : "none" }} />
      {children}
    </span>
  );
}

/** Chunky progress bar. */
export function Bar({ pct, className }: { pct: number | null | undefined; className?: string }) {
  const p = Math.min(100, Math.max(0, pct ?? 0));
  return (
    <div className={cn("h-[12px] w-full overflow-hidden rounded border-2 border-black bg-bg", className)}
      style={{ borderColor: "rgb(var(--line))" }}>
      <div className="h-full bg-accent transition-all duration-300" style={{ width: `${p}%` }} />
    </div>
  );
}

export function Seg<T extends string>({ options, value, onChange, className }: {
  options: Array<{ value: T; label: string }>;
  value: T;
  onChange: (v: T) => void;
  className?: string;
}) {
  return (
    <div className={cn("inline-flex gap-1.5 rounded-lg border-2 bg-bg p-1.5", className)}
      style={{ borderColor: "rgb(var(--line))", boxShadow: "4px 4px 0 rgb(var(--line))" }}>
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          className={cn(
            "rounded px-4 py-2 text-[12.5px] font-extrabold uppercase italic tracking-wide transition-all",
            value === o.value
              ? "border-2 bg-accent"
              : "border-2 border-transparent text-dim hover:text-ink",
          )}
          style={value === o.value
            ? { borderColor: "rgb(var(--line))", color: "rgb(var(--accent-ink))" }
            : undefined}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Switch({ on, onClick, disabled }: { on: boolean; onClick: () => void; disabled?: boolean }) {
  return (
    <button onClick={onClick} disabled={disabled}
      className="relative h-[28px] w-[52px] shrink-0 cursor-pointer rounded-md border-2 transition-colors disabled:cursor-not-allowed disabled:opacity-40"
      style={{
        borderColor: "rgb(var(--line))",
        background: on ? "rgb(var(--accent))" : "rgb(var(--bg))",
        boxShadow: "3px 3px 0 rgb(var(--line))",
      }}>
      <span className="absolute top-[2px] h-[20px] w-[20px] rounded border-2 bg-cream transition-all"
        style={{ left: on ? 26 : 3, borderColor: "rgb(var(--line))" }} />
    </button>
  );
}

export function Toast({ text, error, onClose }: { text: string; error?: boolean; onClose: () => void }) {
  React.useEffect(() => {
    const t = setTimeout(onClose, 5000);
    return () => clearTimeout(t);
  }, [text, onClose]);
  const c = error ? "rgb(var(--bad))" : "rgb(var(--ok))";
  return (
    <button onClick={onClose}
      className="fixed bottom-6 right-5 z-50 flex max-w-sm cursor-pointer items-center gap-2.5 rounded-lg border-2 bg-bg2 px-4 py-3 text-left text-[13px] font-semibold"
      style={{ borderColor: "rgb(var(--line))", boxShadow: "5px 5px 0 rgb(var(--line))" }}>
      <span className="cdot" style={{ background: c, boxShadow: `0 0 8px ${c}` }} />
      {text}
    </button>
  );
}
