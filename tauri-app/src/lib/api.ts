import { invoke } from "@tauri-apps/api/core";

export interface GpuRow {
  name: string;
  usage_pct: number | null;
  temp_c: number | null;
  power_w: number | null;
  clock_mhz: number | null;
}

export interface ProfileState {
  daemon: boolean;
  profile: string;
  quiet_fan: boolean;
  ppd: string;
  curve: Array<[number, number]>;
  fan_duty: number;
  /** "curve" | "manual" | "ec_auto" — "" bila daemon mati. */
  fan_mode: string;
}

export interface Snapshot {
  product: string;
  cpu_model: string;
  is_root: boolean;
  cpu_temp_line: string;
  cpu_freq_line: string;
  cpu_usage_pct: number | null;
  governor: string;
  epp: string;
  gpus: GpuRow[];
  fan_rpms: number[];
  bat_pct: number | null;
  bat_line: string;
  bat_start: number | null;
  bat_end: number | null;
  mem_pct: number | null;
  mem_line: string;
  pkg_watts: number | null;
  ec_cpu_temp: number | null;
  ec_fan1_rpm: number | null;
  ec_fan2_rpm: number | null;
  ec_duty: number | null;
  ec_err: string | null;
  max_temp_c: number | null;
  kbd_nodes: number;
  kbd_max: number;
  kbd_brightness: number | null;
  kbd_rgb: [number, number, number] | null;
  kbd_zones: Array<[number, [number, number, number]]>;
  kbd_writable: boolean;
  bat_writable: boolean;
  /** Efek RGB aktif ("static" = warna diam). */
  kbd_effect: string;
  /** Pengali kecepatan efek (1.0 = normal). */
  kbd_effect_speed: number;
  profile: ProfileState;
  stamp: number;
}

/** `false` bila UI dibuka di browser biasa (tanpa runtime Tauri) — invoke pasti gagal. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const api = {
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  setProfile: (name: string) => invoke<string>("set_profile", { name }),
  setQuietFan: (quiet: boolean) => invoke<string>("set_quiet_fan", { quiet }),
  fanManual: (duty: number) => invoke<string>("fan_set_manual", { duty }),
  fanAuto: () => invoke<string>("fan_set_auto"),
  setFanEcAuto: (auto: boolean) => invoke<string>("set_fan_ec_auto", { auto }),
  setFanManual: (duty: number) => invoke<string>("set_fan_manual", { duty }),
  clearFanOverride: () => invoke<string>("clear_fan_override"),
  kbdSet: (zone: number | null, brightness: number, r: number, g: number, b: number) =>
    invoke<string>("kbd_set", { args: { zone, brightness, r, g, b } }),
  kbdEffectStart: (effect: string, r: number, g: number, b: number, brightness: number, speed: number) =>
    invoke<string>("kbd_effect_start", { args: { effect, r, g, b, brightness, speed } }),
  kbdEffectStop: () => invoke<string>("kbd_effect_stop"),
  batterySet: (start: number | null, end: number | null) =>
    invoke<string>("battery_set", { start, end }),
};

export const THEMES = [
  "ryoku",
  "gruvbox",
  "dracula",
  "nord",
  "tokyo",
  "catppuccin",
] as const;
export type ThemeName = (typeof THEMES)[number];

/** Katalog efek RGB (cermin `kbd_effect::KbdEffect`). */
export const EFFECTS: Array<{ id: string; label: string; desc: string }> = [
  { id: "static", label: "Static", desc: "warna diam" },
  { id: "breathing", label: "Breathing", desc: "fade in/out" },
  { id: "wave", label: "Wave", desc: "hue mengalir per zona" },
  { id: "rainbow", label: "Rainbow", desc: "semua zona sinkron" },
  { id: "cycle", label: "Cycle", desc: "6 preset bergantian" },
  { id: "aurora", label: "Aurora", desc: "pastel lambat" },
  { id: "twinkle", label: "Twinkle", desc: "kilau acak" },
  { id: "pulse", label: "Pulse", desc: "detak jantung" },
  { id: "gradient", label: "Gradient", desc: "pelangi tetap per zona" },
  { id: "music", label: "Music", desc: "denyut beat (simulasi)" },
  { id: "spectrum", label: "Spectrum", desc: "band per zona (simulasi)" },
  { id: "reactive", label: "Reactive", desc: "flash (placeholder)" },
];
