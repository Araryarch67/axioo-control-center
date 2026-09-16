import * as React from "react";
import {
  Activity, Battery, Cpu, Fan, Gauge as GaugeIcon, Keyboard, LayoutGrid,
  Minus, Monitor, Power, Settings as SettingsIcon, Square, X, Zap,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, EFFECTS, THEMES, isTauri, type Snapshot, type ThemeName } from "@/lib/api";
import { useStore, applyTheme, type Tab } from "@/lib/store";
import { tickEffect } from "@/lib/effects";
import { cn, curveDuty, fmt1, hexToRgb } from "@/lib/utils";
import { Bar, Card, CardTitle, CButton, Chip, Seg, Switch, Toast } from "@/components/ui";
import { Stat, SpecRow } from "@/components/Gauge";
import { KeyboardVisual } from "@/components/KeyboardVisual";
import { FanCurveEditor } from "@/components/FanCurveEditor";

const TABS: Array<{ id: Tab; label: string; icon: React.ReactNode }> = [
  { id: "dashboard", label: "Dash", icon: <LayoutGrid size={19} /> },
  { id: "performance", label: "Perf", icon: <Zap size={19} /> },
  { id: "fan", label: "Fans", icon: <Fan size={19} /> },
  { id: "keyboard", label: "Keys", icon: <Keyboard size={19} /> },
  { id: "power", label: "Power", icon: <Power size={19} /> },
  { id: "settings", label: "Setup", icon: <SettingsIcon size={19} /> },
];

const TITLES: Record<Tab, { title: string; sub: string }> = {
  dashboard: { title: "Dashboard", sub: "Live hardware overview" },
  performance: { title: "Performance", sub: "Power modes & limits" },
  fan: { title: "Fan Control", sub: "Thermals & curves" },
  keyboard: { title: "Keyboard", sub: "Backlight zones" },
  power: { title: "Power", sub: "Battery & draw" },
  settings: { title: "Settings", sub: "Theme & service" },
};

const PRESETS = ["#f5efe0", "#ff5f56", "#86be78", "#7aa2f7", "#f5a524", "#7dd3e0", "#cba6f7", "#ff79c6"];

function Titlebar({ tab }: { tab: Tab }) {
  const win = React.useMemo(() => (isTauri() ? getCurrentWindow() : null), []);
  const btn = "flex h-8 w-11 items-center justify-center text-faint transition-colors hover:bg-card2 hover:text-ink";
  return (
    <div className="flex h-11 shrink-0 items-stretch border-b-2 border-black bg-bg">
      <div data-tauri-drag-region className="flex-1" />
      <div data-tauri-drag-region className="flex items-center justify-center">
        <span className="text-[12.5px] font-extrabold uppercase italic tracking-[0.14em]">{TITLES[tab].title}</span>
      </div>
      <div className="flex flex-1 items-stretch justify-end">
        <button className={btn} onClick={() => win?.minimize()} aria-label="minimize"><Minus size={14} /></button>
        <button className={btn} onClick={() => win?.toggleMaximize()} aria-label="maximize"><Square size={12} /></button>
        <button className={cn(btn, "hover:bg-bad hover:text-white")} onClick={() => win?.close()} aria-label="close"><X size={15} /></button>
      </div>
    </div>
  );
}

export default function App() {
  const tab = useStore((s) => s.tab);
  const setTab = useStore((s) => s.setTab);
  const theme = useStore((s) => s.theme);
  const setTheme = useStore((s) => s.setTheme);
  const snap = useStore((s) => s.snap);
  const startPolling = useStore((s) => s.startPolling);
  const stopPolling = useStore((s) => s.stopPolling);
  const toast = useStore((s) => s.toast);
  const dismissToast = useStore((s) => s.dismissToast);
  const busy = useStore((s) => s.busy);
  const run = useStore((s) => s.run);
  const curve = useStore((s) => s.fanCurve);
  const setCurve = useStore((s) => s.setFanCurve);
  const manualDuty = useStore((s) => s.fanManualDuty);
  const setManualDuty = useStore((s) => s.setFanManualDuty);
  const kbdZone = useStore((s) => s.kbdZone);
  const setKbdZone = useStore((s) => s.setKbdZone);
  // State keyboard di store (tidak reset tiap pindah tab; hydrate sekali dari hardware).
  const kbdBright = useStore((s) => s.kbdBright);
  const setKbdBright = useStore((s) => s.setKbdBright);
  const kbdHex = useStore((s) => s.kbdHex);
  const setKbdHex = useStore((s) => s.setKbdHex);
  const kbdDraft = useStore((s) => s.kbdDraft);
  const setKbdDraft = useStore((s) => s.setKbdDraft);
  const kbdFx = useStore((s) => s.kbdFx);
  const setKbdFx = useStore((s) => s.setKbdFx);
  const kbdRearFx = useStore((s) => s.kbdRearFx);
  const setKbdRearFx = useStore((s) => s.setKbdRearFx);
  const kbdSpeed = useStore((s) => s.kbdSpeed);
  const setKbdSpeed = useStore((s) => s.setKbdSpeed);
  const [batStart, setBatStart] = React.useState("80");
  const [batEnd, setBatEnd] = React.useState("100");

  React.useEffect(() => {
    applyTheme(useStore.getState().theme);
    startPolling();
    return () => stopPolling();
  }, [startPolling, stopPolling]);

  React.useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  React.useEffect(() => {
    if (!snap) return;
    if (snap.bat_start != null) setBatStart(String(snap.bat_start));
    if (snap.bat_end != null) setBatEnd(String(snap.bat_end));
  }, [snap?.stamp]); // eslint-disable-line react-hooks/exhaustive-deps

  const daemon = snap?.profile.daemon ?? false;
  const liveTemp = snap?.max_temp_c ?? snap?.ec_cpu_temp ?? null;
  const ecAuto = daemon && (snap?.profile.fan_mode ?? "") === "ec_auto";
  const shownDuty = !daemon
    ? (liveTemp != null ? curveDuty(curve, liveTemp) : manualDuty)
    : ecAuto ? null : snap?.profile.fan_duty ?? null;

  return (
    <div className="flex h-full flex-col bg-transparent text-ink">
      <Titlebar tab={tab} />
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-[76px] shrink-0 flex-col items-center border-r border-hair bg-bg py-3">
          <nav className="flex w-full flex-1 flex-col gap-1 px-2">
            {TABS.map((t) => (
              <button key={t.id} data-active={tab === t.id} onClick={() => setTab(t.id)} className="railbtn">
                {t.icon}
                <span>{t.label}</span>
              </button>
            ))}
          </nav>
          <div className="flex flex-col items-center gap-2">
            <span className="cdot" style={{
              background: daemon ? "rgb(var(--ok))" : "rgb(var(--ghost))",
              boxShadow: daemon ? "0 0 8px rgb(var(--ok))" : "none",
            }} />
            <span className="num text-[9px] text-faint">{snap ? `#${snap.stamp % 100}` : "--"}</span>
          </div>
        </aside>

        <main className="bg-grid min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto max-w-[1360px] p-6">
            {!isTauri() && (
              <div className="card mb-4 flex items-center gap-2.5 !border-accent/50 p-3.5 text-[13px] text-accent">
                Buka lewat aplikasi Axioo Control Center (./dev.sh), bukan di browser.
              </div>
            )}
            <div className="mb-5 flex flex-wrap items-end justify-between gap-3">
              <div>
                <h1 className="text-[26px] font-black uppercase italic tracking-tight text-cream">{TITLES[tab].title}</h1>
                <p className="mt-0.5 text-[13px] font-medium text-faint">{TITLES[tab].sub} · {snap?.product ?? "…"}</p>
              </div>
              <div className="flex gap-2">
                <Chip on={daemon} color={daemon ? "ok" : "bad"}>{snap?.profile.profile ?? "-"} {snap?.profile.quiet_fan ? "+quiet" : ""}</Chip>
                <Chip>PPD {snap?.profile.ppd ?? "-"}</Chip>
                {snap?.ec_err && !daemon && <Chip color="bad">EC unreadable — needs root</Chip>}
              </div>
            </div>

            {tab === "dashboard" && <Dashboard snap={snap} shownDuty={shownDuty} ecAuto={ecAuto} liveTemp={liveTemp} busy={busy} run={run} />}
            {tab === "performance" && <Performance snap={snap} busy={busy} run={run} />}
            {tab === "fan" && <FanPanel snap={snap} curve={curve} setCurve={setCurve} liveTemp={liveTemp} manualDuty={manualDuty} setManualDuty={setManualDuty} busy={busy} run={run} ecAuto={ecAuto} />}
            {tab === "keyboard" && <KeyboardPanel snap={snap} zone={kbdZone} setZone={setKbdZone}
              bright={kbdBright} setBright={setKbdBright} hex={kbdHex} setHex={setKbdHex}
              draft={kbdDraft} setDraft={setKbdDraft} fx={kbdFx} setFx={setKbdFx}
              rearFx={kbdRearFx} setRearFx={setKbdRearFx}
              speed={kbdSpeed} setSpeed={setKbdSpeed} />}
            {tab === "power" && <PowerPanel snap={snap} batStart={batStart} setBatStart={setBatStart} batEnd={batEnd} setBatEnd={setBatEnd} busy={busy} run={run} />}
            {tab === "settings" && <SettingsPanel snap={snap} theme={theme} setTheme={setTheme} />}
          </div>
        </main>
      </div>
      {toast && <Toast text={toast.text} error={toast.error} onClose={dismissToast} />}
    </div>
  );
}

/* ---------------- dashboard ---------------- */

function Dashboard({ snap, shownDuty, ecAuto, liveTemp, busy, run }: {
  snap: Snapshot | null; shownDuty: number | null; ecAuto: boolean; liveTemp: number | null;
  busy: boolean; run: (fn: () => Promise<string>) => void;
}) {
  const cpuTemp = snap?.max_temp_c ?? snap?.ec_cpu_temp ?? null;
  const fan1 = snap?.ec_fan1_rpm ?? snap?.fan_rpms[0] ?? null;
  const fan2 = snap?.ec_fan2_rpm ?? snap?.fan_rpms[1] ?? null;
  const shortCpu = (snap?.cpu_model ?? "-").replace(/\(R\)|\(TM\)/g, "").replace(/\s+/g, " ").trim();
  return (
    <div className="grid grid-cols-12 gap-4">
      <Card className="col-span-12 sm:col-span-6 xl:col-span-3">
        <div className="flex items-baseline justify-between">
          <span className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">CPU load</span>
          <span className="num text-[11px] font-bold text-accent">{cpuTemp ?? "-"}°C</span>
        </div>
        <div className="num mt-1 text-[44px] font-black leading-none tracking-tight text-cream">{fmt1(snap?.cpu_usage_pct, "%")}</div>
        <div className="mt-3"><Bar pct={snap?.cpu_usage_pct} /></div>
        <div className="num mt-2 truncate text-[11px] text-faint" title={`${snap?.governor} · ${snap?.epp} · ${snap?.cpu_freq_line}`}>
          {snap?.governor ?? "?"} · {snap?.epp ?? "?"} · {snap?.cpu_freq_line ?? ""}
        </div>
      </Card>
      <Card className="col-span-12 sm:col-span-6 xl:col-span-3">
        <div className="flex items-baseline justify-between">
          <span className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">Fans</span>
          <span className="num text-[11px] font-bold text-accent">{fan1 ?? "-"} / {fan2 ?? "-"} rpm</span>
        </div>
        <div className="num mt-1 text-[44px] font-black leading-none tracking-tight text-cream">
          {ecAuto ? "AUTO" : shownDuty ?? "-"}
          {!ecAuto && <span className="unit text-[20px] text-faint">%</span>}
        </div>
        <div className="mt-3"><Bar pct={ecAuto ? null : shownDuty} /></div>
        <div className="num mt-2 truncate text-[11px] text-faint">{ecAuto ? "firmware owns fans" : `duty ${shownDuty ?? "-"}% · ${liveTemp ?? "-"}°C`}</div>
      </Card>
      <Card className="col-span-12 sm:col-span-6 xl:col-span-3">
        <div className="flex items-baseline justify-between">
          <span className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">Battery</span>
          <span className="num text-[11px] font-bold text-accent">{snap?.bat_start ?? "?"}→{snap?.bat_end ?? "?"}%</span>
        </div>
        <div className="num mt-1 text-[44px] font-black leading-none tracking-tight text-cream">
          {snap?.bat_pct != null ? Math.round(snap.bat_pct!) : "-"}<span className="unit text-[20px] text-faint">%</span>
        </div>
        <div className="mt-3"><Bar pct={snap?.bat_pct} /></div>
        <div className="num mt-2 truncate text-[11px] text-faint" title={snap?.bat_line ?? ""}>{snap?.bat_line ?? "-"}</div>
      </Card>
      <Card className="col-span-12 sm:col-span-6 xl:col-span-3">
        <div className="flex items-baseline justify-between">
          <span className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">Memory</span>
          <span className="num text-[11px] font-bold text-accent">{fmt1(snap?.pkg_watts, "W")} pkg</span>
        </div>
        <div className="num mt-1 text-[44px] font-black leading-none tracking-tight text-cream">
          {snap?.mem_pct != null ? snap.mem_pct!.toFixed(0) : "-"}<span className="unit text-[20px] text-faint">%</span>
        </div>
        <div className="mt-3"><Bar pct={snap?.mem_pct} /></div>
        <div className="num mt-2 truncate text-[11px] text-faint" title={snap?.mem_line ?? ""}>{snap?.mem_line ?? "-"}</div>
      </Card>

      <Card className="col-span-12 xl:col-span-7">
        <CardTitle icon={<Cpu size={14} />}>System</CardTitle>
        <div className="grid gap-x-8 gap-y-4 sm:grid-cols-2">
          <SpecRow label="Product" value={snap?.product ?? "-"} />
          <SpecRow label="CPU" value={shortCpu} sub={snap?.cpu_freq_line ?? ""} />
          <SpecRow label="GPU" value={snap?.gpus[0]?.name ?? "N/A"} sub={snap?.gpus[0] ? `${fmt1(snap.gpus[0].temp_c, "°C")} · ${fmt1(snap.gpus[0].power_w, "W")}` : "power.limit N/A"} />
          <SpecRow label="Keyboard" value={snap ? `${snap.kbd_nodes} zones` : "-"} sub={snap?.kbd_writable ? "writable" : "read-only"} />
        </div>
      </Card>
      <Card className="col-span-12 xl:col-span-5">
        <CardTitle icon={<Activity size={14} />} right={<Chip on={snap?.profile.daemon} color={snap?.profile.daemon ? "ok" : "bad"}>{snap?.profile.profile ?? "-"}</Chip>}>Profile</CardTitle>
        <div className="flex gap-2">
          {(["Balanced", "Entertainment", "Performance"] as const).map((m) => (
            <button key={m} disabled={busy || !snap?.profile.daemon}
              className={cn("flex-1 rounded-[9px] border px-2 py-2.5 text-[12.5px] font-semibold transition-colors disabled:cursor-not-allowed disabled:opacity-50",
                snap?.profile.profile === m
                  ? "border-accent bg-accent/10 text-accent"
                  : "border-hair text-dim hover:text-ink")}
              onClick={() => run(() => api.setProfile(m))}>
              {m}
            </button>
          ))}
        </div>
        <p className="mt-2.5 text-[12px] text-faint">Full controls in the Performance tab.</p>
      </Card>
    </div>
  );
}

/* ---------------- performance ---------------- */

function Performance({ snap, busy, run }: { snap: Snapshot | null; busy: boolean; run: (fn: () => Promise<string>) => void }) {
  const cur = snap?.profile.profile ?? "Balanced";
  const modes = [
    { v: "Balanced", d: "Daily driver · cool & quiet", icon: <Monitor size={18} /> },
    { v: "Entertainment", d: "Casual · extra headroom", icon: <Activity size={18} /> },
    { v: "Performance", d: "Gaming · max sustained", icon: <Zap size={18} /> },
  ];
  return (
    <div className="grid grid-cols-12 gap-4">
      {modes.map((m) => {
        const active = cur === m.v;
        return (
          <button key={m.v} disabled={busy || !snap?.profile.daemon} onClick={() => run(() => api.setProfile(m.v))}
            className={cn("card col-span-12 cursor-pointer p-5 text-left transition-all sm:col-span-4",
              active ? "!border-black" : "hover:-translate-y-0.5",
              (!snap?.profile.daemon || busy) && "cursor-not-allowed opacity-60")}
            style={active ? { boxShadow: "6px 6px 0 rgb(var(--accent))" } : undefined}>
            <span className={cn("flex h-10 w-10 items-center justify-center rounded",
              active ? "bg-accent" : "bg-card2")}
              style={active ? { color: "rgb(var(--accent-ink))" } : { color: "rgb(var(--faint))" }}>
              {m.icon}
            </span>
            <div className="mt-3 text-[15px] font-black uppercase tracking-wide">{m.v}</div>
            <div className="mt-0.5 text-[12.5px] text-faint">{m.d}</div>
            {active && <div className="num mt-2 text-[11px] font-semibold text-accent">● ACTIVE</div>}
          </button>
        );
      })}
      <Card className="col-span-12 flex items-center justify-between sm:col-span-6">
        <div>
          <div className="text-[14px] font-bold">Quiet fan</div>
          <div className="text-[12.5px] text-faint">Pin fans to minimum duty</div>
        </div>
        <Switch on={snap?.profile.quiet_fan ?? false} disabled={busy || !snap?.profile.daemon}
          onClick={() => run(() => api.setQuietFan(!(snap?.profile.quiet_fan ?? false)))} />
      </Card>
      <Card className="col-span-12 sm:col-span-6">
        <Stat label="Package power" value={fmt1(snap?.pkg_watts, "W")} sub="Balanced 44/120W · Ent/Perf 44/160W" />
      </Card>
      {!snap?.profile.daemon && (
        <p className="col-span-12 font-mono text-[11.5px] text-accent">axiood offline — jalankan ./install-system.sh lalu restart app.</p>
      )}
    </div>
  );
}

/* ---------------- fan ---------------- */

function FanPanel({ snap, curve, setCurve, liveTemp, manualDuty, setManualDuty, busy, run, ecAuto }: {
  snap: Snapshot | null; curve: Array<[number, number]>; setCurve: (p: Array<[number, number]>) => void;
  liveTemp: number | null; manualDuty: number; setManualDuty: (n: number) => void;
  busy: boolean; run: (fn: () => Promise<string>) => void; ecAuto: boolean;
}) {
  const daemon = snap?.profile.daemon ?? false;
  const fanMode = snap?.profile.fan_mode || "curve";
  const manual = daemon && fanMode === "manual";
  const shown = !daemon
    ? (liveTemp != null ? curveDuty(curve, liveTemp) : manualDuty)
    : ecAuto ? null : snap?.profile.fan_duty ?? null;
  const setFill = (el: HTMLInputElement | null, v: number) => {
    el?.style.setProperty("--fill", `${((v - 40) / 60) * 100}%`);
  };

  return (
    <div className="grid grid-cols-12 gap-4">
      <Card className="col-span-12 xl:col-span-8">
        <CardTitle icon={<GaugeIcon size={14} />}
          right={daemon
            ? <Seg value={fanMode as "curve" | "manual" | "ec_auto"}
              options={[{ value: "curve", label: "Curve" }, { value: "manual", label: "Manual" }, { value: "ec_auto", label: "EC Auto" }]}
              onChange={(m) => {
                if (m === "ec_auto") run(() => api.setFanEcAuto(true));
                else if (m === "manual") run(() => api.setFanManual(manualDuty));
                else run(async () => {
                  if (fanMode === "ec_auto") await api.setFanEcAuto(false);
                  return api.clearFanOverride();
                });
              }} />
            : <Chip>Firmware preview</Chip>}>
          Fan curve
        </CardTitle>
        {ecAuto ? (
          <div className="flex items-center gap-4 rounded-lg border-2 border-black bg-bg p-5">
            <Fan size={26} className="shrink-0 text-accent" />
            <div>
              <div className="text-[15px] font-bold">EC Auto — firmware in control</div>
              <div className="text-[12.5px] text-faint">RAPL + profile still managed by the daemon.</div>
            </div>
            <CButton className="ml-auto" disabled={busy} onClick={() => run(() => api.setFanEcAuto(false))}>Take over</CButton>
          </div>
        ) : (
          <FanCurveEditor points={daemon && snap?.profile.curve.length ? snap.profile.curve : curve}
            liveTemp={liveTemp} disabled={daemon} onChange={setCurve} />
        )}
      </Card>

      <div className="col-span-12 flex flex-col gap-4 xl:col-span-4">
        <Card>
          <div className="flex items-baseline justify-between">
            <span className="text-[11px] font-extrabold uppercase tracking-[0.12em] text-faint">Duty</span>
            <span className="num text-[11px] font-bold text-accent">{liveTemp != null ? `${liveTemp}°C` : ""}</span>
          </div>
          <div className="num mt-1 text-[52px] font-black leading-none tracking-tight text-cream">
            {ecAuto ? "AUTO" : shown ?? "-"}
            {!ecAuto && <span className="unit text-[22px] text-faint">%</span>}
          </div>
          <div className="mt-3"><Bar pct={ecAuto ? null : shown} /></div>
          <div className="num mt-2 text-[11px] text-faint">
            fan1 {snap?.ec_fan1_rpm ?? snap?.fan_rpms[0] ?? "-"} · fan2 {snap?.ec_fan2_rpm ?? snap?.fan_rpms[1] ?? "-"} rpm
          </div>
        </Card>
        <Card>
          <CardTitle>Manual duty</CardTitle>
          <div className="flex items-center gap-3">
            <input type="range" min={40} max={100} value={manualDuty}
              disabled={busy || (daemon && ecAuto)}
              ref={(el) => setFill(el, manualDuty)}
              onChange={(e) => { setManualDuty(Number(e.target.value)); setFill(e.target, Number(e.target.value)); }}
              className="ck flex-1" />
            <span className="num w-14 text-right text-[20px] font-bold text-cream">{manualDuty}<span className="text-xs text-faint">%</span></span>
          </div>
          <div className="mt-3 flex gap-2">
            {daemon ? (
              manual
                ? <>
                  <CButton variant="accent" disabled={busy} className="flex-1" onClick={() => run(() => api.setFanManual(manualDuty))}>Lock {manualDuty}%</CButton>
                  <CButton disabled={busy} onClick={() => run(() => api.clearFanOverride())}>Curve</CButton>
                </>
                : ecAuto
                  ? <CButton disabled className="flex-1">Daemon idle — firmware owns fans</CButton>
                  : <CButton variant="accent" disabled={busy} className="flex-1" onClick={() => run(() => api.setFanManual(manualDuty))}>Lock {manualDuty}%</CButton>
            ) : (
              <>
                <CButton variant="accent" disabled={busy || !snap?.is_root} className="flex-1" onClick={() => run(() => api.fanManual(manualDuty))}>Apply</CButton>
                <CButton disabled={busy || !snap?.is_root} onClick={() => run(() => api.fanAuto())}>EC Auto</CButton>
              </>
            )}
          </div>
          {!daemon && !snap?.is_root && <p className="mt-2 font-mono text-[11px] text-accent">Needs root or axiood.</p>}
        </Card>
      </div>
    </div>
  );
}

/* ---------------- keyboard ---------------- */

/* ---------------- keyboard (live-apply, tanpa tombol) ---------------- */

function KeyboardPanel({ snap, zone, setZone, bright, setBright, hex, setHex, draft, setDraft, fx, setFx, rearFx, setRearFx, speed, setSpeed }: {
  snap: Snapshot | null; zone: number | null; setZone: (z: number | null) => void;
  bright: number; setBright: (n: number) => void; hex: string; setHex: (h: string) => void;
  draft: string; setDraft: (d: string) => void; fx: string; setFx: (f: string) => void;
  rearFx: string; setRearFx: (f: string) => void;
  speed: number; setSpeed: (s: number) => void;
}) {
  const max = snap?.kbd_max || 255;
  const nZoneTotal = snap?.kbd_zones.length ?? 0;
  /** Indeks 4 dari 5+ zona = lightbar exhaust belakang (EC 0x07). */
  const zoneName = (i: number | null) =>
    i == null ? "semua" : nZoneTotal >= 5 && i === 4 ? "Rear" : `Z${i + 1}`;
  const [status, setStatus] = React.useState("…");
  const [animT, setAnimT] = React.useState(0);
  const lastFxChange = React.useRef(0);
  const timer = React.useRef<number | undefined>(undefined);
  const canWrite = (snap?.kbd_writable ?? false) && (snap?.kbd_nodes ?? 0) > 0;
  const validHex = /^#[0-9a-fA-F]{6}$/.test(hex);
  const nZones = Math.max(snap?.kbd_zones.length ?? 0, 4);
  // Rear independen hanya bila backend melihat ≥5 node (indeks terakhir).
  const rearIdx = (snap?.kbd_zones.length ?? 0) >= 5 ? 4 : -1;
  const rearActive = rearFx !== "follow" && rearIdx >= 0;

  // Rekonsiliasi efek dari backend (bila user >1.5 dtk tak mengubah).
  // State warna/brightness milik App (tidak reset); init sekali di App.
  React.useEffect(() => {
    if (!snap) return;
    if (Date.now() - lastFxChange.current > 1500 && snap.kbd_effect !== fx) setFx(snap.kbd_effect);
    if (Date.now() - lastFxChange.current > 1500 && snap.kbd_rear_effect !== rearFx) setRearFx(snap.kbd_rear_effect);
  }, [snap, fx, rearFx]);

  const sameRgb = (a: [number, number, number], b: [number, number, number]) =>
    Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]) + Math.abs(a[2] - b[2]) <= 6;

  const applyNow = React.useCallback(async (f: string, b: number, h: string, z: number | null, sp: number, rear: string) => {
    const [r, g, bl] = hexToRgb(h);
    try {
      if (f === "static" && rear === "follow") {
        await api.kbdSet(z, b, r, g, bl);
      } else {
        await api.kbdEffectStart(f, r, g, bl, b, sp, rear);
      }
      setStatus(`live · ${new Date().toLocaleTimeString("en-GB")}`);
    } catch (e) {
      setStatus(`gagal: ${String(e).slice(0, 110)}`);
    }
  }, []);

  // Jam animasi GUI: reset tiap ganti efek/speed (backend thread juga
  // mulai dari t=0 saat spawn, jadi fase awal selaras). Jalan juga bila
  // hanya rear yang beranimasi (keyboard static + rear wave).
  React.useEffect(() => {
    if (fx === "static" && !rearActive) return;
    let raf = 0;
    let last = 0;
    const t0 = performance.now();
    const loop = (now: number) => {
      if (now - last > 80) {
        last = now;
        setAnimT((now - t0) / 1000);
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, [fx, speed, rearActive, rearFx]);

  // Zona tampil: snapshot saat static, hasil tick() saat efek jalan.
  // Rear independen selalu di-preview dari tick()-nya sendiri.
  const [rr, gg, bb] = hexToRgb(validHex ? hex : "#000000");
  const shownZones: Array<[number, [number, number, number]]> = React.useMemo(() => {
    if (fx === "static" && !rearActive) return snap?.kbd_zones ?? [];
    const b = Math.min(bright, max);
    const cols = (fx === "static" && snap?.kbd_zones.length
      ? snap.kbd_zones.map((z) => z[1])
      : tickEffect(fx, [rr, gg, bb], animT * speed, nZones)).map(
      (c) => [b, c] as [number, [number, number, number]],
    );
    if (rearActive) {
      const rc = tickEffect(rearFx, [rr, gg, bb], animT * speed, 1)[0];
      if (rearIdx >= 0 && rearIdx < cols.length) cols[rearIdx] = [b, rc];
    }
    return cols;
  }, [fx, rearActive, rearFx, rearIdx, snap, rr, gg, bb, bright, max, animT, speed, nZones]);

  // Apakah hardware sudah sinkron? Hanya bermakna untuk static
  // (efek animasi selalu berubah, tak pernah "sinkron"). Zona rear
  // dikeluarkan bila ia beranimasi independen.
  const inSync = React.useMemo(() => {
    if (fx !== "static") return false;
    const zones = snap?.kbd_zones ?? [];
    if (!zones.length || !validHex) return false;
    const [r, g, b] = hexToRgb(hex);
    const all = zone == null
      ? zones.map((_: [number, [number, number, number]], i: number) => i)
      : [zone];
    const targets = rearActive ? all.filter((i: number) => i !== rearIdx) : all;
    if (!targets.length) return false;
    return targets.every((i: number) => {
      const z = zones[i];
      return z && z[0] === Math.min(bright, max) && sameRgb(z[1], [r, g, b]);
    });
  }, [snap, validHex, hex, zone, bright, max, fx, rearActive, rearIdx]);

  // Tulis hanya setelah interaksi user (bukan saat mount/init),
  // agar kembali dari tab lain tidak menimpa hardware.
  const dirtyRef = React.useRef(false);
  const markDirty = () => { dirtyRef.current = true; };

  const pickEffect = (f: string) => {
    setFx(f);
    lastFxChange.current = Date.now();
    markDirty();
    if (!canWrite || !validHex) return;
    window.clearTimeout(timer.current);
    applyNow(f, bright, hex, zone, speed, rearFx);
  };

  const pickRear = (r: string) => {
    setRearFx(r);
    lastFxChange.current = Date.now();
    if (!canWrite || !validHex) return;
    window.clearTimeout(timer.current);
    // Langsung apply (tanpa markDirty agar debounce tak double-spawn).
    applyNow(fx, bright, hex, zone, speed, r);
  };

  // Live-apply debounce; lewati bila invalid atau sudah sinkron.
  React.useEffect(() => {
    if (!dirtyRef.current || !canWrite || !validHex || inSync) {
      if (inSync) setStatus("sinkron");
      return;
    }
    setStatus("menulis…");
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => applyNow(fx, bright, hex, zone, speed, rearFx), 280);
    return () => window.clearTimeout(timer.current);
  }, [bright, hex, zone, canWrite, validHex, inSync, fx, rearFx, speed, applyNow]);

  const pickPreset = (p: string) => {
    setHex(p);
    setDraft(p);
    markDirty();
    if (canWrite) {
      window.clearTimeout(timer.current);
      applyNow(fx, bright, p, zone, speed, rearFx);
    }
  };

  const commitDraft = () => {
    if (/^#[0-9a-fA-F]{6}$/.test(draft.trim())) {
      const h = draft.trim().toLowerCase();
      markDirty();
      setHex(h);
      setDraft(h);
    } else {
      setDraft(hex); // kembalikan bila invalid
    }
  };

  const [r, g, b] = [rr, gg, bb];
  return (
    <div className="grid grid-cols-12 gap-4">
      <Card className="col-span-12">
        <CardTitle icon={<Keyboard size={14} />}
          right={<Chip on={canWrite} color={canWrite ? "ok" : undefined}>
            {`live · ${zoneName(zone)}`} · {fx !== "static" ? `${fx} · ` : ""}{status}
          </Chip>}>
          Live map — klik untuk pilih zona{fx !== "static" ? " · beranimasi" : ""}
        </CardTitle>
        <KeyboardVisual zones={shownZones} maxBright={snap?.kbd_max || 255}
          selected={zone} onSelect={(z) => { markDirty(); setZone(z); }} />
        <div className="num mt-2 flex flex-wrap gap-x-5 gap-y-1 text-[11px] text-faint">
          {shownZones.map((z, i) => (
            <span key={i}>
              {zoneName(i)} · {z[0]} · <span className="font-bold" style={{ color: `rgb(${z[1][0]},${z[1][1]},${z[1][2]})` }}>■</span> {z[1].join(" ")}
            </span>
          ))}
        </div>
      </Card>
      <Card className="col-span-12">
        <CardTitle icon={<Zap size={14} />}
          right={<Chip on={fx !== "static"} color={fx !== "static" ? "ok" : undefined}>
            {EFFECTS.find((e) => e.id === fx)?.label ?? fx}
          </Chip>}>
          Effect — animasi semua zona
        </CardTitle>
        <div className="grid grid-cols-3 gap-2 sm:grid-cols-4 lg:grid-cols-6">
          {EFFECTS.map((e) => (
            <button key={e.id} title={e.desc} onClick={() => pickEffect(e.id)}
              className={cn("rounded border-2 p-2.5 text-left transition-all",
                fx === e.id
                  ? "border-black bg-accent"
                  : "border-hair hover:border-dim")}
              style={fx === e.id
                ? { color: "rgb(var(--accent-ink))", boxShadow: "3px 3px 0 #000" }
                : undefined}>
              <div className="text-[12.5px] font-black uppercase italic">{e.label}</div>
              <div className={cn("mt-0.5 text-[10.5px]", fx === e.id ? "opacity-70" : "text-faint")}>{e.desc}</div>
            </button>
          ))}
        </div>
        {fx !== "static" && (
          <>
            <div className="mt-4 flex items-center gap-4">
              <span className="text-[12px] font-extrabold uppercase text-dim">Speed</span>
              <input type="range" min={0.25} max={3} step={0.25} value={speed}
                ref={(el) => el?.style.setProperty("--fill", `${((speed - 0.25) / 2.75) * 100}%`)}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  markDirty();
                  setSpeed(v);
                  e.target.style.setProperty("--fill", `${((v - 0.25) / 2.75) * 100}%`);
                }} className="ck flex-1" />
              <span className="num w-14 text-right text-[20px] font-black text-cream">{speed.toFixed(2).replace(/0$/, "")}<span className="unit text-xs text-faint">x</span></span>
            </div>
            <p className="num mt-2 text-[11px] text-faint">efek memakai warna + brightness di bawah untuk semua zona · pilih Static untuk kembali ke warna diam per-zona</p>
          </>
        )}
        {rearIdx >= 0 && (
          <div className="mt-4 flex flex-wrap items-center gap-3 border-t border-hair pt-4">
            <span className="text-[12px] font-extrabold uppercase text-dim">Rear exhaust</span>
            <select value={rearFx} onChange={(e) => pickRear(e.target.value)}
              className="field !w-auto cursor-pointer" title="Animasi independen lightbar belakang (EC 0x07)">
              <option value="follow">Follow keyboard</option>
              {EFFECTS.filter((e) => e.id !== "static").map((e) => (
                <option key={e.id} value={e.id}>{e.label} — {e.desc}</option>
              ))}
            </select>
            {rearActive && <Chip on color="ok">rear {rearFx} · keyboard tetap</Chip>}
          </div>
        )}
      </Card>
      <Card className="col-span-12 xl:col-span-5">
        <CardTitle icon={<Keyboard size={14} />} right={<Chip on={(snap?.kbd_nodes ?? 0) > 0}>{snap?.kbd_nodes ?? 0} LEDs</Chip>}>Zones</CardTitle>
        <div className="flex flex-wrap gap-2">
          {[{ label: "All", z: null }, ...Array.from({ length: snap?.kbd_nodes ?? 0 }, (_, i) => ({ label: zoneName(i), z: i as number | null }))].map((o) => (
            <button key={o.label} onClick={() => { markDirty(); setZone(o.z); }}
              className={cn("rounded px-3.5 py-2 text-[13px] font-extrabold uppercase transition-all",
                zone === o.z
                  ? "border-2 border-black bg-accent"
                  : "border-2 border-hair text-dim hover:text-ink")}
              style={zone === o.z
                ? { color: "rgb(var(--accent-ink))", boxShadow: "3px 3px 0 #000" }
                : undefined}>
              {o.label}
              {o.z != null && <span className="num unit text-[11px] opacity-70">{snap?.kbd_zones[o.z]?.[0] ?? "-"}</span>}
            </button>
          ))}
        </div>
        {(snap?.kbd_nodes ?? 0) === 0 && <p className="mt-2 font-mono text-[11px] text-accent">No LEDs — check tuxedo_keyboard driver.</p>}
        <div className="mt-5">
          <div className="mb-1 flex justify-between text-[12px] font-extrabold uppercase text-dim">
            <span>Brightness</span>
            <span className="num text-cream">{Math.min(bright, max)}<span className="unit text-faint">/{max}</span></span>
          </div>
          <input type="range" min={0} max={max} value={Math.min(bright, max)}
            ref={(el) => el?.style.setProperty("--fill", `${(Math.min(bright, max) / max) * 100}%`)}
            onChange={(e) => {
              markDirty();
              setBright(Number(e.target.value));
              e.target.style.setProperty("--fill", `${(Number(e.target.value) / max) * 100}%`);
            }} className="ck" />
        </div>
        <div className="mt-4 flex h-16 items-center justify-center gap-1.5 overflow-hidden rounded-lg border-2 border-black bg-bg">
          {Array.from({ length: 14 }, (_, i) => (
            <span key={i} className="inline-block h-6 w-3.5 rounded-[3px] transition-colors"
              style={{ background: `rgb(${r},${g},${b})`, opacity: (bright / Math.max(max, 1)) * (0.35 + (0.65 * i) / 13) }} />
          ))}
        </div>
        <div className="num mt-1.5 text-center text-[11px] text-faint">preview · {zoneName(zone) === "semua" ? "all zones" : zoneName(zone)}</div>
      </Card>
      <Card className="col-span-12 xl:col-span-7">
        <CardTitle right={<span className="h-6 w-12 rounded-md border-2 border-black" style={{ background: hex }} />}>Color</CardTitle>
        <div className="grid grid-cols-8 gap-2">
          {PRESETS.map((p) => (
            <button key={p} title={p} onClick={() => pickPreset(p)}
              className={cn("h-10 rounded border-2 transition-transform hover:scale-105 hover:-rotate-3",
                hex === p ? "border-cream" : "border-black")}
              style={{ background: p, boxShadow: hex === p ? "3px 3px 0 #000" : "2px 2px 0 #000" }} />
          ))}
        </div>
        <div className="mt-4 flex items-center gap-3">
          <input type="color" value={validHex ? hex : "#000000"} onChange={(e) => { markDirty(); setHex(e.target.value); setDraft(e.target.value); }}
            className="h-10 w-16 cursor-pointer rounded border-2 border-black bg-transparent p-1" />
          <input value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commitDraft}
            onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
            spellCheck={false} className="field !w-28 num" />
          <span className="num text-[12px] text-faint">rgb({r}, {g}, {b})</span>
        </div>
        <p className="mt-3 font-mono text-[11px] text-faint">perubahan langsung ditulis ke LED - tanpa tombol apply.</p>
        {snap && !snap.kbd_writable && snap.kbd_nodes > 0 && (
          <p className="mt-1 font-mono text-[11px] text-accent">sysfs read-only - jalankan ./setup.sh lalu reboot.</p>
        )}
      </Card>
    </div>
  );
}

/* ---------------- power ---------------- */

function PowerPanel({ snap, batStart, setBatStart, batEnd, setBatEnd, busy, run }: {
  snap: Snapshot | null; batStart: string; setBatStart: (s: string) => void;
  batEnd: string; setBatEnd: (s: string) => void; busy: boolean; run: (fn: () => Promise<string>) => void;
}) {
  return (
    <div className="grid grid-cols-12 gap-4">
      <Card className="col-span-12 sm:col-span-6">
        <CardTitle icon={<Battery size={14} />}>Battery</CardTitle>
        <div className="num text-[52px] font-black leading-none tracking-tight text-cream">
          {snap?.bat_pct != null ? Math.round(snap.bat_pct!) : "-"}<span className="unit text-[22px] text-faint">%</span>
        </div>
        <div className="mt-3"><Bar pct={snap?.bat_pct} /></div>
        <div className="num mt-2 truncate text-[12px] text-dim" title={snap?.bat_line ?? ""}>{snap?.bat_line ?? "-"}</div>
        <div className="num mt-1 text-[12px] text-faint">FlexiCharger {snap?.bat_start ?? "?"}% → {snap?.bat_end ?? "?"}%</div>
      </Card>
      <Card className="col-span-12 sm:col-span-6">
        <CardTitle>Charge limits</CardTitle>
        <div className="grid grid-cols-2 gap-3">
          <label className="text-[12px] font-semibold text-dim">Start · 40–95
            <input value={batStart} onChange={(e) => setBatStart(e.target.value)} inputMode="numeric" className="field num mt-1.5" />
          </label>
          <label className="text-[12px] font-semibold text-dim">End · 60–100
            <input value={batEnd} onChange={(e) => setBatEnd(e.target.value)} inputMode="numeric" className="field num mt-1.5" />
          </label>
        </div>
        <CButton variant="accent" disabled={busy || !snap?.bat_writable} className="mt-3 w-full"
          onClick={() => run(() => api.batterySet(batStart ? Number(batStart) : null, batEnd ? Number(batEnd) : null))}>
          Save thresholds
        </CButton>
        {snap && !snap.bat_writable && (
          <p className="mt-2 font-mono text-[11px] text-accent">sysfs read-only - jalankan ./setup.sh lalu reboot.</p>
        )}
      </Card>
      <Card className="col-span-12">
        <CardTitle>Power draw</CardTitle>
        <div className="grid gap-6 sm:grid-cols-2">
          <div>
            {snap?.pkg_watts != null ? (
              <>
                <div className="num text-[44px] font-black leading-none tracking-tight text-cream">
                  {snap.pkg_watts!.toFixed(1)}<span className="unit text-[20px] text-faint">W live</span>
                </div>
                <div className="mt-3"><Bar pct={(snap.pkg_watts! / 160) * 100} /></div>
                <p className="num mt-2 text-[11.5px] text-faint">live package power via RAPL</p>
              </>
            ) : (
              <>
                <div className="num text-[44px] font-black leading-none tracking-tight text-cream">
                  44<span className="unit text-[20px] text-faint">W / {snap?.profile.profile === "Balanced" ? "120" : "160"}W caps</span>
                </div>
                <div className="mt-3"><Bar pct={null} /></div>
                <p className="num mt-2 text-[11.5px] text-faint">PL1/PL2 untuk {snap?.profile.profile ?? "-"} — live RAPL tak terbaca (butuh akses powercap)</p>
              </>
            )}
          </div>
          <div className="space-y-2.5">
            {[
              ["Profile caps", snap?.profile.profile === "Balanced" ? "PL1 44W · PL2 120W" : "PL1 44W · PL2 160W"],
              ["PPD", snap?.profile.ppd ?? "-"],
              ["Governor", snap?.governor ?? "?"],
              ["EPP", snap?.epp ?? "?"],
            ].map(([k, v]) => (
              <div key={k} className="flex items-center justify-between gap-3 text-[13px]">
                <span className="font-bold uppercase tracking-wide text-faint text-[11px]">{k}</span>
                <span className="num font-bold text-cream">{v}</span>
              </div>
            ))}
            <p className="pt-1 text-[12px] font-medium leading-relaxed text-faint">
              PL1/PL2 ditegakkan axiood per profil. EPP + governor milik power-profiles-daemon.
            </p>
          </div>
        </div>
      </Card>
    </div>
  );
}

/* ---------------- settings ---------------- */

function SettingsPanel({ snap, theme, setTheme }: {
  snap: Snapshot | null; theme: ThemeName; setTheme: (t: ThemeName) => void;
}) {
  const [autoStart, setAutoStart] = React.useState<boolean | null>(null);
  React.useEffect(() => {
    if (!isTauri()) return;
    let alive = true;
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const m = await import("@tauri-apps/plugin-autostart");
        if (alive) setAutoStart(await m.isEnabled());
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<boolean>("autostart-changed", (e) => {
          if (alive) setAutoStart(e.payload);
        });
      } catch { if (alive) setAutoStart(null); }
    })();
    return () => { alive = false; unlisten?.(); };
  }, []);
  const flipAutoStart = async () => {
    if (autoStart == null) return;
    try {
      const m = await import("@tauri-apps/plugin-autostart");
      if (autoStart) await m.disable(); else await m.enable();
      setAutoStart(!autoStart);
    } catch { /* toast di level App bila perlu */ }
  };
  return (
    <div className="grid grid-cols-12 gap-4">
      <Card className="col-span-12 xl:col-span-6">
        <CardTitle>Startup</CardTitle>
        <div className="flex items-center justify-between gap-3">
          <div>
            <div className="text-[14px] font-bold">Start saat login (tray)</div>
            <div className="text-[12.5px] text-faint">App sembunyi ke tray · butuh tray host (mis. waybar) di Hyprland</div>
          </div>
          <Switch on={autoStart ?? false} disabled={!isTauri() || autoStart == null} onClick={flipAutoStart} />
        </div>
        <p className="mt-2 text-[12px] text-faint">Tutup jendela (×/Alt+F4) = sembunyi ke tray · keluar via menu tray → Keluar.</p>
      </Card>
      <Card className="col-span-12 xl:col-span-6">
        <CardTitle>Theme</CardTitle>
        <div className="grid grid-cols-2 gap-2">
          {THEMES.map((t) => (
            <button key={t} onClick={() => setTheme(t)}
              className={cn("rounded-lg border-2 p-3 text-left capitalize transition-all",
                theme === t ? "border-black bg-accent/15" : "border-hair hover:border-dim")}
              style={theme === t ? { boxShadow: "4px 4px 0 #000" } : undefined}>
              <div className="text-[13.5px] font-black uppercase tracking-wide">{t}</div>
              <div className="mt-1.5 flex gap-1">
                <span className="h-3.5 w-8 rounded-full bg-cream" />
                <span className="h-3.5 w-8 rounded-full bg-accent" />
                <span className="h-3.5 w-8 rounded-full bg-ghost" />
              </div>
            </button>
          ))}
        </div>
      </Card>
      <Card className="col-span-12 xl:col-span-6">
        <CardTitle>Service status</CardTitle>
        <div className="space-y-2.5">
          {[
            ["axiood daemon", snap?.profile.daemon ? `running · ${snap.profile.profile}` : "offline", !!snap?.profile.daemon],
            ["Fan mode", snap?.profile.daemon ? snap.profile.fan_mode : "-", !!snap?.profile.daemon],
            ["Keyboard sysfs", snap?.kbd_writable ? `writable · ${snap?.kbd_nodes} zones` : "read-only", snap?.kbd_writable ?? false],
            ["Battery sysfs", snap?.bat_writable ? "writable" : "read-only", snap?.bat_writable ?? false],
            ["App privileges", snap?.is_root ? "root (direct EC)" : "user (via daemon)", true],
          ].map(([k, v, ok]) => (
            <div key={k as string} className="flex items-center justify-between text-[13px]">
              <span className="text-faint">{k}</span>
              <Chip on={ok as boolean} color={(ok as boolean) ? "ok" : undefined}>{v as string}</Chip>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
