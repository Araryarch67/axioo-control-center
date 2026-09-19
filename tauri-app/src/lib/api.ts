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
  /** Dynamic device DB: stable id, grade string, fan-write gate. */
  device_id: string;
  device_grade: string;
  fan_write_allowed: boolean;
  cpu_model: string;
  is_root: boolean;
  cpu_temp_line: string;
  cpu_freq_line: string;
  cpu_usage_pct: number | null;
  governor: string;
  epp: string;
  gpus: GpuRow[];
  /** dGPU RTD3: "active" | "suspended" | "absent". */
  dgpu_state: string;
  /** (pid, name, MiB) holders — only queried when active. */
  dgpu_procs: Array<[number, string, number | null]>;
  fan_rpms: number[];
  bat_pct: number | null;
  bat_line: string;
  bat_start: number | null;
  bat_end: number | null;
  /** Full/design health % (null bila firmware tak expose kapasitas). */
  bat_health_pct: number | null;
  bat_full_milli: number | null;
  bat_design_milli: number | null;
  bat_capacity_unit: string | null;
  /** Hours until empty/full (null when Full/rate unreadable). */
  bat_time_h: number | null;
  bat_charging: boolean;
  /** AC mains online (null bila firmware tak expose). */
  ac_online: boolean | null;
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
  /** Efek rear exhaust ("follow" = ikut efek utama). */
  kbd_rear_effect: string;
  /** Pengali kecepatan efek (1.0 = normal). */
  kbd_effect_speed: number;
  /** mtime colors.json matugen (0 bila tak ada) — pemicu refresh theme. */
  matugen_rev: number;
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
  kbdEffectStart: (effect: string, r: number, g: number, b: number, brightness: number, speed: number, rear?: string) =>
    invoke<string>("kbd_effect_start", { args: { effect, r, g, b, brightness, speed, rear: rear ?? "follow" } }),
  kbdEffectStop: () => invoke<string>("kbd_effect_stop"),
  batterySet: (start: number | null, end: number | null) =>
    invoke<string>("battery_set", { start, end }),
  /** Palet matugen Ryoku (Err bila ~/.cache/ryoku/colors.json tak ada). */
  matugen: () => invoke<Record<string, string>>("get_matugen"),
  /** Autostart login (entry wrapper, bukan plugin — lihat main.rs). */
  autostartGet: () => invoke<boolean>("autostart_get"),
  autostartSet: (enabled: boolean) => invoke<boolean>("autostart_set", { enabled }),
  /** Perilaku tombol close (Settings). Backend simpan per sesi; default tray. */
  closeBehaviorSet: (behavior: string) => invoke<string>("close_behavior_set", { behavior }),
};

export const THEMES = [
  "matugen",
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
  { id: "static", label: "Static", desc: "still color" },
  { id: "breathing", label: "Breathing", desc: "fade in/out" },
  { id: "wave", label: "Wave", desc: "hue flowing per zone" },
  { id: "rainbow", label: "Rainbow", desc: "all zones in sync" },
  { id: "cycle", label: "Cycle", desc: "6 presets cycling" },
  { id: "aurora", label: "Aurora", desc: "slow pastel" },
  { id: "twinkle", label: "Twinkle", desc: "random sparkle" },
  { id: "pulse", label: "Pulse", desc: "heartbeat" },
  { id: "gradient", label: "Gradient", desc: "fixed rainbow per zone" },
  { id: "music", label: "Music", desc: "beat pulse (simulated)" },
  { id: "spectrum", label: "Spectrum", desc: "band per zone (simulated)" },
  { id: "reactive", label: "Reactive", desc: "flash (placeholder)" },
];

/**
 * Efek rear exhaust — dikurasi untuk 1 zona (EC 0x07).
 * wave/gradient/spectrum disengaja DIBUANG: di 1 zona wave == rainbow,
 * gradient = merah statis, spectrum ≈ music. Deskripsi beda dari keyboard
 * karena perilakunya memang beda di single LED.
 */
export const REAR_EFFECTS: Array<{ id: string; label: string; desc: string }> = [
  { id: "follow", label: "Follow", desc: "follow keyboard effect" },
  { id: "breathing", label: "Breathing", desc: "fade chosen color" },
  { id: "rainbow", label: "Rainbow", desc: "rotating hue" },
  { id: "cycle", label: "Cycle", desc: "6 presets cycling" },
  { id: "aurora", label: "Aurora", desc: "slow pastel" },
  { id: "twinkle", label: "Twinkle", desc: "random color blink" },
  { id: "pulse", label: "Pulse", desc: "heartbeat" },
  { id: "music", label: "Music", desc: "beat pulse (simulated)" },
];
