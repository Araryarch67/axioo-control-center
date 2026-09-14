/** Big stat: label + value + sub-line. Value truncates with full text on hover. */
export function Stat({ label, value, unit, sub }: {
  label: string; value: string; unit?: string; sub?: string;
}) {
  return (
    <div className="min-w-0">
      <div className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">{label}</div>
      <div className="num mt-0.5 truncate text-[32px] font-extrabold leading-none tracking-tight text-cream" title={value}>
        {value}
        {unit && <span className="unit text-[15px] text-faint">{unit}</span>}
      </div>
      {sub && <div className="num mt-1 truncate text-[11.5px] text-faint" title={sub}>{sub}</div>}
    </div>
  );
}

/** Compact spec row for system cards: small label + truncated value. */
export function SpecRow({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="min-w-0">
      <div className="text-[10.5px] font-extrabold uppercase tracking-[0.12em] text-faint">{label}</div>
      <div className="truncate text-[17px] font-bold leading-snug text-cream" title={value}>{value}</div>
      {sub && <div className="num truncate text-[11px] text-faint" title={sub}>{sub}</div>}
    </div>
  );
}
