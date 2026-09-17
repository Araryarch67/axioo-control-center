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
}

let pollTimer: number | undefined;

const LEGACY_TAB = "axioo.tab";
const LEGACY_THEME = "axioo.theme";

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
          } catch {
            fails++;
            if (fails === 2) {
              get().notice("Backend unreachable — run via ./dev.sh.", true);
            }
          }
        };
        tick();
        pollTimer = window.setInterval(tick, 1000);
      },
      stopPolling: () => {
        if (pollTimer !== undefined) {
          window.clearInterval(pollTimer);
          pollTimer = undefined;
        }
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
      }),
    },
  ),
);
