import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, EFFECTS, type Snapshot, type ThemeName } from "./api";
import { clearMatugen, matugenDominant, refreshMatugen } from "./matugen";
import { hexToRgb, rgbToHex } from "./utils";

export type Tab = "dashboard" | "performance" | "fan" | "keyboard" | "power" | "settings";

export const REFERENCE_CURVE: Array<[number, number]> = [
  [40, 40], [50, 45], [60, 55], [70, 70], [80, 85], [90, 95], [100, 100],
];

interface ToastMsg {
  text: string;
  error?: boolean;
}

interface AppStore {
  // navigasi + tema (persist)
  tab: Tab;
  setTab: (t: Tab) => void;
  theme: ThemeName;
  setTheme: (t: ThemeName) => void;
  // snapshot hardware (polling 1 dtk, session saja)
  snap: Snapshot | null;
  startPolling: () => void;
  stopPolling: () => void;
  // toast + busy + runner aksi (session saja)
  toast: ToastMsg | null;
  notice: (text: string, error?: boolean) => void;
  dismissToast: () => void;
  busy: boolean;
  run: (fn: () => Promise<string>) => Promise<void>;
  // keyboard: zona/fx/speed persist; bright/hex selalu dari hardware saat start
  kbdZone: number | null;
  setKbdZone: (z: number | null) => void;
  kbdBright: number;
  setKbdBright: (n: number) => void;
  kbdHex: string;
  setKbdHex: (h: string) => void;
  kbdDraft: string;
  setKbdDraft: (d: string) => void;
  kbdFx: string;
  setKbdFx: (f: string) => void;
  /** Efek rear independen ("follow" = ikut efek utama). Persist. */
  kbdRearFx: string;
  setKbdRearFx: (f: string) => void;
  kbdSpeed: number;
  setKbdSpeed: (s: number) => void;
  /** Ikuti wallpaper: keyboard + rear = primary matugen tiap wallpaper ganti. Persist. */
  kbdFollowWp: boolean;
  setKbdFollowWp: (b: boolean) => void;
  kbdDirty: boolean;
  markKbdDirty: () => void;
  kbdHydrated: boolean;
  /** Init sekali dari snapshot, dari ZONA YANG DIPILIH (anti bug reset). */
  hydrateKbd: (snap: Snapshot) => void;
  /** `true` bila user pernah mengubah LED (persist) — syarat boot-restore. */
  kbdTouched: boolean;
  /** Sekali per sesi: state LED tersimpan sudah ditulis balik ke hardware. */
  kbdRestored: boolean;
  /** Revisi palet terakhir yang sudah diterapkan ke LED (sesi, anti double). */
  kbdFollowRev: number;
  /**
   * Jaga LED tiap tick, SEBELUM `hydrateKbd` (hardware selalu reset ke
   * putih tiap reboot, jadi adopsi-dari-hardware saja menghapus warna
   * terakhir user). Follow → warna dominan matugen; manual → tulis balik
   * state tersimpan sekali per sesi. No-op bila sysfs tak writable.
   */
  maintainKbd: (snap: Snapshot) => void;
  /** Terapkan warna dominan matugen ke LED (dipakai tick + toggle manual). */
  applyKbdFollow: (bright: number, rev: number) => Promise<boolean>;
  // fan: kurva preview lokal + manual duty (persist)
  fanCurve: Array<[number, number]>;
  setFanCurve: (c: Array<[number, number]>) => void;
  fanManualDuty: number;
  setFanManualDuty: (n: number) => void;
  /** Tampilkan tombol -/kotak/X di titlebar (persist, default tampil). */
  showWinBtns: boolean;
  setShowWinBtns: (b: boolean) => void;
  /** Perilaku tombol close: "tray" (hancurkan ke tray) atau "quit" (keluar). */
  closeBehavior: "tray" | "quit";
  setCloseBehavior: (b: "tray" | "quit") => void;
}

let pollTimer: number | undefined;
/** Interval aktif (tampil 1 dtk; hidden 5/30 dtk sesuai status restore). */
let pollMs = 1000;
/** Tick sekali jalan (dipicu juga oleh event "matugen-changed"). */
let pollNowFn: (() => void) | undefined;
let visHooked = false;
const POLL_VISIBLE_MS = 1000;
/** Hidden + restore LED masih tertunda (udev telat): kejar cepat.
 *  Setelah restored → turun ke SAFETY. */
const POLL_HIDDEN_RESTORE_MS = 5000;
/** Hidden steady-state: jaring pengaman saja (0.4 IPC/menit, -99% vs 1 dtk).
 *  Follow-wallpaper real-time via event backend, bukan interval. */
const POLL_HIDDEN_SAFETY_MS = 30000;

function isHiddenNow(): boolean {
  try {
    if (typeof document !== "undefined" && document.hidden) return true;
  } catch { /* abaikan */ }
  return false;
}

/** Interval hidden sesuai status restore LED (butuh store → lazy). */
function hiddenMs(): number {
  try {
    const s = useStore.getState();
    if (s.kbdTouched && !s.kbdRestored) return POLL_HIDDEN_RESTORE_MS;
  } catch { /* abaikan */ }
  return POLL_HIDDEN_SAFETY_MS;
}

const LEGACY_TAB = "axioo.tab";
const LEGACY_THEME = "axioo.theme";

/* -------- session sensor log (in-memory ring, never persisted) --------
 * Diisi tiap poll tick dari snapshot yang memang sudah diambil.
 * Nol IPC/hardware tambahan — murni mencatat. Cap 3600 baris. */
export interface HistRow {
  t: number; temp: number | null; fanMode: string; duty: number;
  rpm: number | null; watts: number | null; bat: number | null;
}
const HIST_MAX = 3600;
const hist: HistRow[] = [];

export function pushHist(s: Snapshot) {
  hist.push({
    t: Date.now(),
    temp: s.max_temp_c ?? s.ec_cpu_temp ?? null,
    fanMode: s.profile.fan_mode || "-",
    duty: s.profile.fan_duty,
    rpm: s.ec_fan1_rpm ?? s.fan_rpms[0] ?? null,
    watts: s.pkg_watts,
    bat: s.bat_pct,
  });
  if (hist.length > HIST_MAX) hist.splice(0, hist.length - HIST_MAX);
}

export function histCount(): number {
  return hist.length;
}

export function histCsv(): string {
  const q = (v: number | string | null) => (v == null ? "" : String(v));
  const lines = ["timestamp_iso,temp_c,fan_mode,fan_duty_pct,fan1_rpm,pkg_w,batt_pct"];
  for (const r of hist) {
    lines.push(
      [new Date(r.t).toISOString(), q(r.temp), r.fanMode, r.duty, q(r.rpm), q(r.watts), q(r.bat)].join(",")
    );
  }
  return lines.join("\n") + "\n";
}

export function downloadCsv() {
  try {
    const blob = new Blob([histCsv()], { type: "text/csv" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "axioo-sensors.csv";
    a.click();
    window.setTimeout(() => URL.revokeObjectURL(a.href), 5000);
  } catch { /* abaikan (SSR/tests) */ }
}

function legacyTab(): Tab | null {
  try {
    const v = localStorage.getItem(LEGACY_TAB);
    return v === "dashboard" || v === "performance" || v === "fan" ||
      v === "keyboard" || v === "power" || v === "settings" ? v : null;
  } catch { return null; }
}

function legacyTheme(): ThemeName | null {
  try {
    const v = localStorage.getItem(LEGACY_THEME);
    return (["matugen", "ryoku", "gruvbox", "dracula", "nord", "tokyo", "catppuccin"] as const).includes(v as ThemeName)
      ? (v as ThemeName) : null;
  } catch { return null; }
}

/** Terapkan tema ke <html data-theme> + bersihkan key lama bila sudah termigrasi. */
export function applyTheme(theme: ThemeName) {
  document.documentElement.dataset.theme = theme;
  if (theme === "matugen") {
    // Palet live dari ~/.cache/ryoku/colors.json (async, tak diblokir).
    void refreshMatugen();
  } else {
    clearMatugen();
  }
  try {
    localStorage.removeItem(LEGACY_THEME);
    localStorage.removeItem(LEGACY_TAB);
  } catch { /* abaikan */ }
}

export const useStore = create<AppStore>()(
  persist(
    (set, get) => ({
      tab: legacyTab() ?? "dashboard",
      setTab: (tab) => set({ tab }),
      theme: legacyTheme() ?? "ryoku",
      setTheme: (theme) => {
        applyTheme(theme);
        set({ theme });
      },
      snap: null,
      startPolling: () => {
        if (pollTimer !== undefined) return;
        let fails = 0;
        const arm = (ms: number) => {
          pollMs = ms;
          if (pollTimer !== undefined) window.clearInterval(pollTimer);
          pollTimer = window.setInterval(() => void tick(), ms);
        };
        const tick = async () => {
          try {
            const s = await api.snapshot();
            fails = 0;
            // maintainKbd DULU (tulis-balik state tersimpan), baru hydrate
            // (adopsi hardware) — hardware selalu reset tiap reboot.
            get().maintainKbd(s);
            get().hydrateKbd(s);
            // Kurva tampil mengikuti daemon (sumber kebenaran GetCurve).
            if (s.profile.daemon && s.profile.curve.length > 0) {
              set({ fanCurve: s.profile.curve });
            }
            set({ snap: s });
            pushHist(s);
            // Restore baru sukses saat hidden → turun ke safety net.
            if (isHiddenNow() && pollMs !== hiddenMs()) arm(hiddenMs());
          } catch {
            fails++;
            if (fails === 2) {
              get().notice("Backend unreachable — run via ./dev.sh.", true);
            }
          }
        };
        pollNowFn = () => void tick();
        const rearmForVisibility = (hidden: boolean, immediate: boolean) => {
          if (pollTimer === undefined) return;
          // stopPolling sudah dipanggil (unmount) → jangan hidupkan lagi.
          arm(hidden ? hiddenMs() : POLL_VISIBLE_MS);
          if (immediate && !hidden) void tick();
        };
        if (!visHooked) {
          visHooked = true;
          try {
            document.addEventListener("visibilitychange", () => {
              rearmForVisibility(isHiddenNow(), true);
            });
            // Fallback: hide() Tauri di sebagian WebKit tak memicu
            // visibilitychange — blur/focus menutup celahnya.
            window.addEventListener("blur", () => rearmForVisibility(true, false));
            window.addEventListener("focus", () => rearmForVisibility(false, true));
          } catch { /* abaikan (SSR/tests) */ }
          // Sinyal paling akurat di Tauri: fokus window (hide tray = unfocused).
          void (async () => {
            try {
              const { getCurrentWindow } = await import("@tauri-apps/api/window");
              await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
                rearmForVisibility(!focused, focused);
              });
            } catch { /* mode browser — cukup listener DOM */ }
          })();
          // Event-driven backend (watch colors.json): wallpaper ganti saat
          // hidden → tick segera (follow real-time). Saat tampil, poll 1 dtk
          // sudah mencakupnya — lewati agar tak dobel IPC.
          void (async () => {
            try {
              const { listen } = await import("@tauri-apps/api/event");
              await listen<number>("matugen-changed", () => {
                if (pollTimer !== undefined && isHiddenNow()) pollNowFn?.();
              });
            } catch { /* backend lama/browser — safety poll tetap jalan */ }
          })();
        }
        tick();
        arm(isHiddenNow() ? hiddenMs() : POLL_VISIBLE_MS);
      },
      stopPolling: () => {
        if (pollTimer !== undefined) {
          window.clearInterval(pollTimer);
          pollTimer = undefined;
        }
        pollNowFn = undefined;
      },
      toast: null,
      notice: (text, error) => set({ toast: { text, error } }),
      dismissToast: () => set({ toast: null }),
      busy: false,
      run: async (fn) => {
        set({ busy: true });
        try {
          get().notice(await fn());
        } catch (e) {
          get().notice(String(e), true);
        } finally {
          set({ busy: false });
        }
      },
      kbdZone: null,
      setKbdZone: (kbdZone) => set({ kbdZone }),
      kbdBright: 255,
      setKbdBright: (kbdBright) => set({ kbdBright, kbdTouched: true }),
      kbdHex: "#f5efe0",
      setKbdHex: (kbdHex) => set({ kbdHex, kbdTouched: true }),
      kbdDraft: "#f5efe0",
      setKbdDraft: (kbdDraft) => set({ kbdDraft }),
      kbdFx: "static",
      setKbdFx: (kbdFx) => set({ kbdFx, kbdTouched: true }),
      kbdRearFx: "follow",
      setKbdRearFx: (kbdRearFx) => set({ kbdRearFx, kbdTouched: true }),
      kbdSpeed: 1,
      setKbdSpeed: (kbdSpeed) => set({ kbdSpeed, kbdTouched: true }),
      kbdFollowWp: false,
      setKbdFollowWp: (kbdFollowWp) => set({ kbdFollowWp }),
      kbdDirty: false,
      markKbdDirty: () => set({ kbdDirty: true }),
      kbdHydrated: false,
      kbdTouched: false,
      kbdRestored: false,
      kbdFollowRev: 0,
      applyKbdFollow: async (bright, rev) => {
        const c = await matugenDominant();
        if (!c) return false;
        const h = rgbToHex(c[0], c[1], c[2]);
        const max = get().snap?.kbd_max || 255;
        const b = Math.min(bright, max);
        try {
          await api.kbdEffectStop();
          await api.kbdSet(null, b, c[0], c[1], c[2]);
        } catch {
          return false;
        }
        set({
          kbdHex: h, kbdDraft: h, kbdFx: "static", kbdRearFx: "follow",
          kbdTouched: true, kbdFollowRev: rev, kbdRestored: true,
        });
        return true;
      },
      maintainKbd: (snap) => {
        const st = get();
        if (snap.kbd_nodes === 0 || !snap.kbd_writable) return;
        const rev = snap.matugen_rev ?? 0;
        // Mode follow: terapkan tiap revisi palet berubah (termasuk boot).
        if (st.kbdFollowWp) {
          if (rev !== 0 && rev !== st.kbdFollowRev) {
            void st.applyKbdFollow(st.kbdBright, rev);
          }
          return;
        }
        // Mode manual: tulis balik warna terakhir sekali per sesi.
        if (st.kbdRestored || !st.kbdTouched) return;
        const validFx = (f: string) => EFFECTS.some((e) => e.id === f);
        const fx = validFx(st.kbdFx) ? st.kbdFx : "static";
        const rear = st.kbdRearFx === "follow" || validFx(st.kbdRearFx) ? st.kbdRearFx : "follow";
        if (!/^#[0-9a-fA-F]{6}$/.test(st.kbdHex)) {
          set({ kbdRestored: true });
          return;
        }
        const [r, g, bl] = hexToRgb(st.kbdHex);
        const bright = Math.min(st.kbdBright, snap.kbd_max || 255);
        const hex = st.kbdHex;
        void (async () => {
          try {
            if (fx === "static" && rear === "follow") {
              await api.kbdSet(null, bright, r, g, bl);
            } else {
              await api.kbdEffectStart(fx, r, g, bl, bright, st.kbdSpeed, rear);
            }
            set({ kbdDraft: hex, kbdRestored: true });
          } catch {
            /* sysfs gagal (mis. udev belum) — coba lagi tick berikutnya */
          }
        })();
      },
      fanCurve: REFERENCE_CURVE,
      setFanCurve: (fanCurve) => set({ fanCurve }),
      fanManualDuty: 70,
      setFanManualDuty: (fanManualDuty) => set({ fanManualDuty }),
      showWinBtns: true,
      setShowWinBtns: (showWinBtns) => set({ showWinBtns }),
      closeBehavior: "tray",
      setCloseBehavior: (closeBehavior) => {
        set({ closeBehavior });
        void api.closeBehaviorSet(closeBehavior).catch(() => {});
      },
      hydrateKbd: (snap) => {
        const st = get();
        if (st.kbdHydrated || snap.kbd_nodes === 0) return;
        // Restore tertunda (user pernah ubah warna tapi tulis-balik sesi
        // ini belum sukses): JANGAN adopsi hardware putih hasil reset
        // firmware — itu menghapus warna terakhir tersimpan. Tandai
        // hydrated saja; maintainKbd yang akan menulis balik.
        if (st.kbdTouched && !st.kbdRestored) {
          set({ kbdHydrated: true });
          return;
        }
        const z = st.kbdZone != null ? snap.kbd_zones[st.kbdZone] : snap.kbd_zones[0];
        if (z) {
          const c = z[1];
          const h = rgbToHex(c[0], c[1], c[2]);
          set({ kbdBright: z[0], kbdHex: h, kbdDraft: h });
        } else {
          if (snap.kbd_brightness != null) set({ kbdBright: snap.kbd_brightness });
          if (snap.kbd_rgb) {
            const c = snap.kbd_rgb;
            const h = rgbToHex(c[0], c[1], c[2]);
            set({ kbdHex: h, kbdDraft: h });
          }
        }
        if (snap.kbd_effect) set({ kbdFx: snap.kbd_effect });
        if (snap.kbd_rear_effect) set({ kbdRearFx: snap.kbd_rear_effect });
        if (snap.kbd_effect_speed) set({ kbdSpeed: snap.kbd_effect_speed });
        set({ kbdHydrated: true });
      },
    }),
    {
      name: "axioo-center",
      partialize: (s) => ({
        tab: s.tab,
        theme: s.theme,
        kbdZone: s.kbdZone,
        kbdHex: s.kbdHex,
        kbdDraft: s.kbdHex,
        kbdBright: s.kbdBright,
        kbdTouched: s.kbdTouched,
        kbdFx: s.kbdFx,
        kbdRearFx: s.kbdRearFx,
        kbdSpeed: s.kbdSpeed,
        kbdFollowWp: s.kbdFollowWp,
        fanCurve: s.fanCurve,
        fanManualDuty: s.fanManualDuty,
        showWinBtns: s.showWinBtns ?? true,
        closeBehavior: s.closeBehavior ?? "tray",
      }),
    },
  ),
);
