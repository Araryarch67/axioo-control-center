//! axioo-control-center: GPUI desktop frontend, Ryoku-mono style.
//!
//! Bahasa desain: monokrom brutalist (panel hairline, header `// ..._`,
//! pill seleksi putih, barcode, bottom action bar). Font: Iosevka Nerd
//! Font Mono. Sidebar bernomor + label Jepang, konten per tab.
//!
//! Privilege: app memastikan diri jalan sebagai root — bila euid != 0,
//! relaunch diri via `pkexec` SEKALI di awal (env Wayland diteruskan).
//! Tombol tulis EC memanggil `axioo-lib::fan_ctrl` langsung (one-shot
//! `set|auto`) di background thread; fallback pkexec per-aksi hanya bila
//! relaunch dibatalkan/gagal. Loop kurva kontinu tetap milik `axiood`.

use std::time::{Duration, Instant};

use axioo_lib::{
    battery, cpu,
    cpu::CpuTimes,
    dmi, ec,
    fan::{self, FanSnapshot},
    fan_ctrl, hwmon, kbd, memory, nvidia, rapl,
};
use gpui::{
    AnyElement, App, AppContext, Application, AsyncApp, Bounds, ClickEvent, Context, Div,
    IntoElement, InteractiveElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, PathBuilder, Render, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Timer, Window, WindowBounds, WindowOptions, canvas, div,
    point, px, rgb, rgba,
};

// ---------- Ryoku-mono palette ----------
const BG: u32 = 0x0a0a0b;
const PANEL: u32 = 0x111113;
const PANEL2: u32 = 0x1a1a1e;
const BORDER: u32 = 0x2b2b30;
const TEXT: u32 = 0xf2f2f0;
const DIM: u32 = 0x8e8e93;
const FAINT: u32 = 0x55555a;
const WHITE: u32 = 0xffffff;
const INK: u32 = 0x0a0a0b;

const FONT: &str = "Iosevka Nerd Font Mono";

const MODE_QUIET: &str = "Quiet";
const MODE_BALANCED: &str = "Balanced";
const MODE_ENT: &str = "Entertainment";
const MODE_PERF: &str = "Performance";
const MODE_CUSTOM: &str = "Custom";

fn mode_curve(name: &str) -> Vec<(i32, u8)> {
    match name {
        MODE_QUIET => vec![(35, 40), (50, 50), (65, 60), (78, 70), (88, 85)],
        MODE_BALANCED => vec![(25, 40), (40, 55), (55, 65), (70, 80), (82, 90)],
        MODE_PERF => vec![(20, 60), (32, 70), (45, 80), (58, 90), (70, 100)],
        _ => fan::REFERENCE_CURVE.to_vec(),
    }
}

fn mode_desc(name: &str) -> &'static str {
    match name {
        MODE_QUIET => "hening · kipas kalem",
        MODE_BALANCED => "harian · seimbang suhu & bising",
        MODE_ENT => "kasual · kurva referensi",
        MODE_PERF => "gaming · kurva agresif",
        _ => "kurva diedit manual (pratinjau)",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Performa,
    Kipas,
    Keyboard,
    Daya,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Performa => "Performa",
            Tab::Kipas => "Kipas",
            Tab::Keyboard => "Keyboard",
            Tab::Daya => "Daya",
        }
    }

    fn jp(self) -> &'static str {
        match self {
            Tab::Dashboard => "概要",
            Tab::Performa => "性能",
            Tab::Kipas => "ファン",
            Tab::Keyboard => "キーボード",
            Tab::Daya => "電源",
        }
    }

    fn section(self) -> &'static str {
        match self {
            Tab::Dashboard | Tab::Performa => "01 MONITOR",
            Tab::Kipas | Tab::Keyboard => "02 CONTROL",
            Tab::Daya => "03 SYSTEM",
        }
    }

    fn desc(self) -> &'static str {
        match self {
            Tab::Dashboard => "Ringkasan live: suhu, kipas, dan daya dalam satu pandang.",
            Tab::Performa => "Mode performa dan kurva pratinjau kipas.",
            Tab::Kipas => "Kontrol manual satu-kali langsung (root). Loop kontinu butuh axiood.",
            Tab::Keyboard => "Backlight keyboard: brightness + warna (butuh quirk DKMS).",
            Tab::Daya => "Baterai, memori, dan daya paket CPU (RAPL).",
        }
    }

    fn all() -> [Tab; 5] {
        [Tab::Dashboard, Tab::Performa, Tab::Kipas, Tab::Keyboard, Tab::Daya]
    }
}

#[derive(Clone)]
struct GpuRow {
    name: String,
    usage_pct: Option<f64>,
    temp_c: Option<f64>,
    power_w: Option<f64>,
    clock_mhz: Option<u64>,
}

/// Ukuran viewport + ujung konten buat scrollbar (ditulis paint canvas,
/// dibaca render frame berikut — jeda 1 frame, tak terasa).
#[derive(Clone, Copy, Default)]
struct ScrollMeas {
    view_h: f32,
    start_y: f32,
    end_y: f32,
}

#[derive(Clone, Default)]
struct SensorData {
    cpu_temp_line: String,
    cpu_freq_line: String,
    cpu_usage_pct: Option<f64>,
    governor: String,
    epp: String,
    gpus: Vec<GpuRow>,
    fan_rows: Vec<String>,
    bat_rows: Vec<String>,
    bat_pct: Option<f64>,
    mem_pct: Option<f64>,
    mem_line: String,
    pkg_watts: Option<f64>,
    ec: Option<FanSnapshot>,
    ec_err: Option<String>,
    max_temp_c: Option<i32>,
    kbd_nodes: usize,
    kbd_max: u32,
    kbd_brightness: Option<u32>,
    kbd_rgb: Option<(u8, u8, u8)>,
    /// Per-zona (brightness, rgb) sesuai urutan discover (0=kiri).
    kbd_zones: Vec<(u32, (u8, u8, u8))>,
    stamp: String,
}

struct RootView {
    product: String,
    cpu_model: String,
    data: SensorData,
    mode: String,
    curve: Vec<(i32, u8)>,
    fan_manual: bool,
    manual_duty: u8,
    /// true = mode mengikuti kurva (snapshot satu-kali dari suhu saat ini);
    /// false = statis (duty tetap pilihan sendiri).
    mode_auto: bool,
    /// Index titik kurva yang sedang di-drag (None = tidak ada).
    drag_point: Option<usize>,
    /// Zona keyboard target tulis (None = semua zona).
    kbd_zone_sel: Option<usize>,
    /// Popup color picker custom RGB terbuka.
    show_rgb_popup: bool,
    /// Warna custom yang sedang diracik.
    custom_rgb: (u8, u8, u8),
    /// Channel slider (0=R,1=G,2=B) yang sedang di-drag.
    drag_rgb: Option<usize>,
    tab: Tab,
    notice: Option<String>,
    /// True saat worker tulis EC berjalan.
    apply_busy: bool,
    apply_tx: std::sync::mpsc::Sender<String>,
    /// Toast aktif (None = tidak ada).
    toast: Option<Toast>,
    /// Offset scroll vertikal konten (px); reset tiap pindah tab.
    scroll_px: f32,
    /// Ukuran viewport + ujung konten (buat thumb scrollbar).
    scroll_meas: std::rc::Rc<std::cell::Cell<ScrollMeas>>,
}

impl RootView {
    fn new(
        product: String,
        cpu_model: String,
        apply_tx: std::sync::mpsc::Sender<String>,
    ) -> Self {
        Self {
            product,
            cpu_model,
            data: SensorData::default(),
            mode: MODE_BALANCED.to_string(),
            curve: mode_curve(MODE_BALANCED),
            fan_manual: false,
            manual_duty: 70,
            mode_auto: true,
            drag_point: None,
            kbd_zone_sel: None,
            show_rgb_popup: false,
            custom_rgb: (255, 255, 255),
            drag_rgb: None,
            tab: Tab::Dashboard,
            notice: if fan_ctrl::is_root() {
                None
            } else {
                Some("bukan root: tulis EC via pkexec (satu prompt).".to_string())
            },
            apply_busy: false,
            apply_tx,
            toast: None,
            scroll_px: 0.0,
            scroll_meas: std::rc::Rc::new(std::cell::Cell::new(ScrollMeas::default())),
        }
    }

    fn sort_curve(&mut self) {
        self.curve.sort_by_key(|&(t, _)| t);
    }

    fn mark_custom(&mut self) {
        self.mode = MODE_CUSTOM.to_string();
        self.notice = None;
    }

    fn shown_duty(&self) -> Option<u8> {
        if self.fan_manual {
            Some(self.manual_duty)
        } else {
            self.data.max_temp_c.map(|t| fan::curve_duty(&self.curve, t))
        }
    }

    fn fan_rpms(&self) -> (String, String) {
        match self.data.ec {
            Some(s) => (s.fan1_rpm.to_string(), s.fan2_rpm.to_string()),
            None => {
                let mut it = self.data.fan_rows.iter();
                (
                    it.next().cloned().unwrap_or_else(|| "-".to_string()),
                    it.next().cloned().unwrap_or_else(|| "-".to_string()),
                )
            }
        }
    }

    /// Kurva selesai diedit (drag/tombol) → langsung jadi acuan hidup:
    /// ikut kurva + tulis snapshot satu-kali. Tidak perlu TERAPKAN.
    fn curve_live(&mut self, cx: &mut Context<Self>, why: &str) {
        let snap = self
            .data
            .max_temp_c
            .map(|t| fan::curve_duty(&self.curve, t))
            .unwrap_or(self.manual_duty);
        let temp = self
            .data
            .max_temp_c
            .map_or("suhu —".to_string(), |t| format!("{t}°C"));
        self.mode_auto = true;
        self.fan_manual = true;
        self.manual_duty = snap;
        self.spawn_fan_write(
            cx,
            FanWrite::Manual(snap),
            format!("kurva {why} → {snap}% (snapshot {temp})"),
        );
    }
    /// Batas scroll = tinggi konten − tinggi viewport (dari ukur frame lalu).
    fn scroll_max(&self) -> f32 {
        let m = self.scroll_meas.get();
        if m.view_h <= 0.0 {
            return 0.0;
        }
        ((m.end_y - m.start_y) - m.view_h).clamp(0.0, 4000.0)
    }

    /// Tulis EC satu-kali di thread latar: langsung via `fan_ctrl`;
    /// kalau proses bukan root (`NotRoot`), otomatis fallback pkexec.
    /// Hasil ke toast + notice pendek.
    fn spawn_fan_write(&mut self, cx: &mut Context<Self>, write: FanWrite, label: String) {
        if self.apply_busy {
            return;
        }
        self.apply_busy = true;
        self.notice = Some(format!("{label}…"));
        let tx = self.apply_tx.clone();
        std::thread::spawn(move || {
            let msg = match write {
                FanWrite::Manual(d) => match fan_ctrl::set_manual_duty(d) {
                    Ok(r) => match (r.verified_pct, r.verify_skipped) {
                        (Some(v), _) => format!("{label}: OK (EC {v}%)"),
                        (None, true) => format!("{label}: OK (tanpa verifikasi ec_sys)"),
                        _ => format!("{label}: OK"),
                    },
                    Err(fan_ctrl::FanCtrlError::NotRoot) => {
                        format!("{label}: {}", run_pkexec_fallback(&["set", &d.to_string()]))
                    }
                    Err(e) => format!("{label}: gagal: {e}"),
                },
                FanWrite::Auto => match fan_ctrl::set_auto() {
                    Ok(()) => format!("{label}: OK (EC auto)"),
                    Err(fan_ctrl::FanCtrlError::NotRoot) => {
                        format!("{label}: {}", run_pkexec_fallback(&["auto"]))
                    }
                    Err(e) => format!("{label}: gagal: {e}"),
                },
            };
            let _ = tx.send(msg);
        });
        cx.notify();
    }
}

// ---------- one-shot EC write (langsung, tanpa pkexec) ----------

/// Perintah tulis EC satu-kali untuk [`RootView::spawn_fan_write`].
#[derive(Clone)]
enum FanWrite {
    Manual(u8),
    Auto,
}

/// Notifikasi mengambang (pojok kanan bawah): hasil aksi, auto-hilang
/// 5 detik, klik untuk tutup. Bar bawah tetap teks pendek statis.
struct Toast {
    text: String,
    at: Instant,
    error: bool,
}

/// Cari helper `axioo-ctl`: sibling di sebelah binary GUI, else PATH.
/// Dipakai HANYA untuk fallback pkexec saat proses GUI bukan root.
fn axioo_ctl_path() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sib = dir.join("axioo-ctl");
            if sib.exists() {
                return sib.to_string_lossy().into_owned();
            }
        }
    }
    "axioo-ctl".to_string()
}

/// Fallback non-root: `pkexec axioo-ctl fan ...` (satu prompt; polkit
/// keep-alive membuat terapan berikut lancar). Hanya dipakai saat tulis
/// langsung ditolak (`NotRoot`).
fn run_pkexec_fallback(args: &[&str]) -> String {
    let ctl = axioo_ctl_path();
    // `timeout` agar prompt auth yang digantung tidak bikin status sibuk
    // selamanya (keluar 124 bila auth tak selesai 120 dtk).
    let mut cmd = std::process::Command::new("timeout");
    cmd.args(["120", "pkexec", &ctl, "fan"]).args(args);
    match cmd.output() {
        Err(e) => format!(
            "pkexec gagal: {e} (coba manual: sudo {ctl} fan {})",
            args.join(" ")
        ),
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if out.status.success() {
                let short: String = stdout.chars().take(160).collect();
                if short.is_empty() {
                    "OK via pkexec".to_string()
                } else {
                    format!("OK via pkexec ({short})")
                }
            } else {
                let mut msg = stdout;
                if msg.is_empty() {
                    msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
                }
                if out.status.code() == Some(124) {
                    return "pkexec timeout: auth tak selesai 120 dtk".to_string();
                }
                if msg.contains("No session") || msg.to_lowercase().contains("polkit") {
                    return format!(
                        "butuh polkit agent → jalankan manual: sudo {ctl} fan {}",
                        args.join(" ")
                    );
                }
                if msg.is_empty() {
                    msg = "dibatalkan / gagal otentikasi".to_string();
                }
                // Rapikan stderr pkexec yang berisik ("Not authorized…"
                // + "incident reported") agar toast enak dibaca.
                let clean = msg
                    .replace("This incident has been reported.", "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let clean = if clean.contains("Not authorized") {
                    "akses ditolak (auth dibatalkan / password salah)".to_string()
                } else {
                    clean
                };
                format!("gagal: {}", clean.chars().take(200).collect::<String>())
            }
        }
    }
}

// ---------- color utils (pure) ----------

fn lighten(c: u32, amt: f32) -> u32 {
    let mix = |ch: u32| ((ch as f32 + (255.0 - ch as f32) * amt).round() as u32).min(255);
    (mix((c >> 16) & 0xFF) << 16) | (mix((c >> 8) & 0xFF) << 8) | mix(c & 0xFF)
}

fn darken(c: u32, amt: f32) -> u32 {
    let mix = |ch: u32| ((ch as f32 * (1.0 - amt)).round() as u32).min(255);
    (mix((c >> 16) & 0xFF) << 16) | (mix((c >> 8) & 0xFF) << 8) | mix(c & 0xFF)
}

/// Skalakan RGB backlight dengan brightness (0..1) jadi hex u32.
/// Murni (ada unit test): glow((255,0,0), 1.0) == 0xFF0000.
fn glow(rgb: (u8, u8, u8), scale: f32) -> u32 {
    let s = scale.clamp(0.0, 1.0);
    let r = (rgb.0 as f32 * s).round() as u32;
    let g = (rgb.1 as f32 * s).round() as u32;
    let b = (rgb.2 as f32 * s).round() as u32;
    (r << 16) | (g << 8) | b
}

/// Padding dalam picker RGB; dipakai paint + hit-test.
const RGB_PAD: f32 = 10.0;
const RGB_ROWS: usize = 3;

/// Kolom-x knob untuk nilai 0..255 di canvas selebar w.
fn rgb_knob_x(val: u8, w: f32) -> f32 {
    RGB_PAD + val as f32 / 255.0 * (w - 2.0 * RGB_PAD)
}

/// Posisi piksel-dalam-canvas → (channel 0=R,1=G,2=B, nilai 0..255).
fn rgb_xy_to_val(lx: f32, ly: f32, w: f32, h: f32) -> (usize, u8) {
    let row_h = (h - 2.0 * RGB_PAD) / RGB_ROWS as f32;
    let ch = ((ly - RGB_PAD) / row_h).floor().clamp(0.0, 2.0) as usize;
    let v = ((lx - RGB_PAD) / (w - 2.0 * RGB_PAD) * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8;
    (ch, v)
}

/// Warna track slider channel c pada fraksi 0..1 (hitam → channel penuh).
fn rgb_track_color(c: usize, f: f32) -> u32 {
    let v = (f.clamp(0.0, 1.0) * 255.0).round() as u32;
    match c {
        0 => v << 16,
        1 => v << 8,
        _ => v,
    }
}

// ---------- mono widgets ----------

/// Tombol kotak mono. `primary` = pill putih teks gelap.
fn btn(
    id: &str,
    label: &str,
    primary: bool,
    cx: &mut Context<RootView>,
    f: impl Fn(&mut RootView, &ClickEvent, &mut Window, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    let (bg, fg, bd) = if primary { (WHITE, INK, WHITE) } else { (PANEL, TEXT, BORDER) };
    let hover_bg = if primary { darken(WHITE, 0.12) } else { lighten(PANEL, 0.35) };
    let press_bg = if primary { darken(WHITE, 0.25) } else { darken(PANEL, 0.3) };
    div()
        .id(SharedString::from(id.to_string()))
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(if primary { bd } else { lighten(BORDER, 0.3) }))
        .text_color(rgb(fg))
        .text_sm()
        .whitespace_nowrap()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(hover_bg)))
        .active(move |s| s.bg(rgb(press_bg)))
        .child(label.to_string())
        .on_click(cx.listener(f))
}

/// Tombol kompak (baris editor kurva): padding + teks kecil, anti-wrap.
fn btn_sm(
    id: &str,
    label: &str,
    cx: &mut Context<RootView>,
    f: impl Fn(&mut RootView, &ClickEvent, &mut Window, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id.to_string()))
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(lighten(BORDER, 0.3)))
        .text_color(rgb(TEXT))
        .text_xs()
        .whitespace_nowrap()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(lighten(PANEL, 0.35))))
        .active(move |s| s.bg(rgb(darken(PANEL, 0.3))))
        .child(label.to_string())
        .on_click(cx.listener(f))
}

/// Pill status bar atas (ROOT / EC / update): satu baris horizontal,
/// tinggi seragam, teks tak terbungkus (tak ada lagi dot menumpuk di
/// atas teks seperti tombol vertikal sebelumnya).
fn status_pill(label: String, active: bool) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(rgb(PANEL2))
        .border_1()
        .border_color(rgb(BORDER))
        .text_color(rgb(if active { TEXT } else { DIM }))
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .truncate()
        .max_w(px(280.))
        .child(label)
}

/// Item navigasi sidebar: `// Label` + kanji kanan; pill putih saat aktif.
fn nav_item(
    tab: Tab,
    selected: bool,
    cx: &mut Context<RootView>,
    f: impl Fn(&mut RootView, &ClickEvent, &mut Window, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("nav-{}", tab.label())))
        .w_full()
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(if selected { WHITE } else { PANEL }))
        .text_color(rgb(if selected { INK } else { TEXT }))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| {
            s.bg(rgb(if selected {
                darken(WHITE, 0.1)
            } else {
                lighten(PANEL, 0.4)
            }))
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .text_sm()
                        .child(format!("{} {}", if selected { "//" } else { "  " }, tab.label())),
                )
                .child(div().text_color(rgb(if selected { INK } else { FAINT })).text_sm().child(tab.jp().to_string())),
        )
        .on_click(cx.listener(f))
}

/// Opsi segmen (AUTO/MANUAL, Calm/Rich): putih saat aktif.
fn seg_opt(
    id: &str,
    label: &str,
    selected: bool,
    cx: &mut Context<RootView>,
    f: impl Fn(&mut RootView, &ClickEvent, &mut Window, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id.to_string()))
        .flex_1()
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(if selected { WHITE } else { PANEL }))
        .text_color(rgb(if selected { INK } else { DIM }))
        .text_sm()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| {
            s.bg(rgb(if selected {
                darken(WHITE, 0.1)
            } else {
                lighten(PANEL, 0.4)
            }))
        })
        .active(move |s| s.bg(rgb(if selected { darken(WHITE, 0.2) } else { PANEL })))
        .child(
            div()
                .w_full()
                .flex()
                .flex_row()
                .justify_center()
                .child(label.to_string()),
        )
        .on_click(cx.listener(f))
}

/// Kartu mode performa: border putih + badge AKTIF saat terpilih.
fn mode_card(
    name: &str,
    hint: &str,
    selected: bool,
    cx: &mut Context<RootView>,
    f: impl Fn(&mut RootView, &ClickEvent, &mut Window, &mut Context<RootView>) + 'static,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("mode-{name}")))
        .flex_1()
        .flex()
        .flex_col()
        .gap_1()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(if selected { PANEL2 } else { PANEL }))
        .border_1()
        .border_color(rgb(if selected { WHITE } else { BORDER }))
        .cursor_pointer()
        .hover(move |s| s.bg(rgb(lighten(PANEL2, 0.15))))
        .active(move |s| s.bg(rgb(darken(PANEL2, 0.2))))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .text_color(rgb(if selected { WHITE } else { TEXT }))
                        .text_sm()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_color(rgb(if selected { WHITE } else { FAINT }))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(if selected { "● AKTIF" } else { "○" }.to_string()),
                ),
        )
        .child(div().text_color(rgb(DIM)).text_xs().child(hint.to_string()))
        .on_click(cx.listener(f))
}

/// Panel kartu: hairline border + header `// TITLE_` + `+`.
/// `w_full` + `overflow_hidden` agar teks panjang tidak meluber keluar
/// kartu dan baris kartu selalu sama tinggi (kartu mengisi wrapper kolom).
fn card(title: &str, children: impl IntoIterator<Item = impl IntoElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded_md()
        .bg(rgb(PANEL))
        .border_1()
        .border_color(rgb(BORDER))
        .w_full()
        .flex_1()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .text_color(rgb(DIM))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(format!("// {title}_")),
                )
                .child(div().text_color(rgb(FAINT)).text_xs().child("+".to_string())),
        )
        .children(children)
}

/// Baris `label .... value`. Value dipotong ellipsis agar label dan
/// value tidak pernah menempel (`GOVpowersave`) atau meluber keluar kartu.
fn kv(tag: &str, val: String) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .w_full()
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(DIM))
                .text_sm()
                .child(tag.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_color(rgb(TEXT))
                .text_sm()
                .text_right()
                .truncate()
                .child(val),
        )
}

/// Bar segmen mono (menyala putih). Segmen lentur mengikuti lebar kartu
/// agar tidak overflow di kartu sempit; angka persen tidak menyusut.
fn segbar(pct: Option<f64>) -> Div {
    let lit = pct.map_or(0, |v| (v.clamp(0.0, 100.0) / 5.0).round() as usize);
    let mut segs = Vec::with_capacity(20);
    for i in 0..20 {
        segs.push(
            div()
                .flex_1()
                .min_w(px(4.))
                .max_w(px(12.))
                .h(px(10.))
                .rounded_sm()
                .bg(rgb(if i < lit { TEXT } else { BORDER })),
        );
    }
    div().flex().flex_row().items_center().gap_2().w_full().child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(2.))
            .flex_1()
            .min_w(px(0.))
            .children(segs),
    ).child(
        div()
            .flex_shrink_0()
            .text_color(rgb(TEXT))
            .text_sm()
            .child(pct.map_or("-".to_string(), |v| format!("{v:.0}%"))),
    )
}

fn hero_number(big: String, sub: String) -> Div {
    div()
        .flex()
        .flex_row()
        .items_end()
        .justify_between()
        .gap_3()
        .w_full()
        .child(
            div()
                .flex_shrink_0()
                .text_color(rgb(WHITE))
                .text_size(px(36.))
                .font_weight(gpui::FontWeight::BOLD)
                .child(big),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_color(rgb(DIM))
                .text_sm()
                .text_right()
                .truncate()
                .child(sub),
        )
}

/// Barcode dekoratif: selalu penuh selebar sidebar via `justify_between`
/// (bar tetap proporsional, sisa ruang terbagi jadi celah) — tak pernah
/// terpotong di tepi seperti sebelumnya.
fn barcode() -> Div {
    let units = [3., 1., 2., 1., 1., 4., 1., 2., 1., 3., 1., 1., 2., 4., 1., 2., 1., 1., 3., 2., 1., 4., 1., 2.];
    let mut bars = Vec::with_capacity(units.len());
    for (i, u) in units.iter().enumerate() {
        bars.push(div().w(px(u * 2.0)).h(px(24.)).bg(rgb(if i % 4 == 3 { FAINT } else { TEXT })));
    }
    div().flex().flex_col().gap_1().w_full().child(
        div().flex().flex_row().items_end().justify_between().w_full().children(bars),
    ).child(div().text_color(rgb(FAINT)).text_xs().child("AXIOO HUB".to_string()))
}

/// Padding dalam chart kurva (px logis); dipakai paint + hit-test drag.
const CHART_PAD: f32 = 16.0;

/// Piksel-dalam-canvas → (suhu °C, duty %) untuk drag titik kurva.
fn chart_xy_to_data(lx: f32, ly: f32, w: f32, h: f32) -> (i32, u8) {
    let t = (20.0 + (lx - CHART_PAD) / (w - 2.0 * CHART_PAD) * 80.0)
        .round()
        .clamp(20.0, 100.0) as i32;
    let d = (40.0 + (h - CHART_PAD - ly) / (h - 2.0 * CHART_PAD) * 60.0)
        .round()
        .clamp(40.0, 100.0) as u8;
    (t, d)
}

/// (suhu °C, duty %) → piksel-dalam-canvas (kebalikan fungsi di atas).
fn chart_data_to_xy(t: i32, d: u8, w: f32, h: f32) -> (f32, f32) {
    let x = CHART_PAD + (t as f32 - 20.0) / 80.0 * (w - 2.0 * CHART_PAD);
    let y = h - CHART_PAD - (d as f32 - 40.0) / 60.0 * (h - 2.0 * CHART_PAD);
    (x, y)
}

/// Duty pada suhu `t` via interpolasi linear antar titik (dot live).
fn chart_duty_at(pts: &[(i32, u8)], t: i32) -> u8 {
    let mut s = pts.to_vec();
    s.sort_by_key(|&(tt, _)| tt);
    if s.is_empty() {
        return fan::MIN_FAN_DUTY_PCT;
    }
    if t <= s[0].0 {
        return s[0].1;
    }
    if t >= s[s.len() - 1].0 {
        return s[s.len() - 1].1;
    }
    for w in s.windows(2) {
        let (t0, d0) = w[0];
        let (t1, d1) = w[1];
        if t >= t0 && t <= t1 {
            if t1 == t0 {
                return d0;
            }
            let f = (t - t0) as f32 / (t1 - t0) as f32;
            return (d0 as f32 + f * (d1 as f32 - d0 as f32)).round() as u8;
        }
    }
    s[s.len() - 1].1
}

/// Chart kurva yang jelas + bisa drag: grid vertikal & horizontal, garis
/// tebal, arsiran bawah kurva, titik besar, marker live + dot di kurva.
/// Titik digeser dengan mouse: tekan dekat titik, tahan, geser, lepas.
fn curve_chart(
    curve: Vec<(i32, u8)>,
    live_temp: Option<i32>,
    drag_idx: Option<usize>,
    bounds_cell: std::rc::Rc<std::cell::Cell<Option<Bounds<gpui::Pixels>>>>,
    cx: &mut Context<RootView>,
) -> Stateful<Div> {
    let cell_down = bounds_cell.clone();
    let cell_move = bounds_cell.clone();
    div()
        .id(SharedString::from("curve-chart"))
        .w_full()
        .child(
            canvas(
                move |_bounds, _window, _cx| (curve.clone(), live_temp, drag_idx),
                move |bounds: Bounds<gpui::Pixels>,
                      (pts, live, drag): (Vec<(i32, u8)>, Option<i32>, Option<usize>),
                      window: &mut Window,
                      _cx: &mut App| {
                    bounds_cell.set(Some(bounds));
                    let w: f32 = bounds.size.width.into();
                    let h: f32 = bounds.size.height.into();
                    let ox: f32 = bounds.origin.x.into();
                    let oy: f32 = bounds.origin.y.into();
                    let x_of = |t: f32| ox + CHART_PAD + (t - 20.0) / 80.0 * (w - 2.0 * CHART_PAD);
                    let y_of =
                        |d: f32| oy + h - CHART_PAD - (d - 40.0) / 60.0 * (h - 2.0 * CHART_PAD);
                    // Grid penuh: vertikal tiap 20°C + horizontal tiap 15%.
                    let mut grid = PathBuilder::stroke(px(1.0));
                    for t in [20.0, 40.0, 60.0, 80.0, 100.0] {
                        grid.move_to(point(px(x_of(t)), px(oy + 4.0)));
                        grid.line_to(point(px(x_of(t)), px(oy + h - 4.0)));
                    }
                    for d in [40.0, 55.0, 70.0, 85.0, 100.0] {
                        grid.move_to(point(px(ox + CHART_PAD), px(y_of(d))));
                        grid.line_to(point(px(ox + w - CHART_PAD), px(y_of(d))));
                    }
                    if let Ok(p) = grid.build() {
                        window.paint_path(p, rgb(BORDER));
                    }
                    let mut sorted = pts.clone();
                    sorted.sort_by_key(|&(t, _)| t);
                    if sorted.len() == 1 {
                        sorted.push((100, sorted[0].1));
                    }
                    if sorted.is_empty() {
                        return;
                    }
                    // Arsiran di bawah kurva.
                    let yb = y_of(40.0);
                    let mut poly = Vec::with_capacity(sorted.len() + 2);
                    poly.push(point(
                        px(x_of(sorted[0].0.clamp(20, 100) as f32)),
                        px(yb),
                    ));
                    for (t, d) in sorted.iter() {
                        poly.push(point(
                            px(x_of((*t).clamp(20, 100) as f32)),
                            px(y_of(*d as f32)),
                        ));
                    }
                    poly.push(point(
                        px(x_of(sorted[sorted.len() - 1].0.clamp(20, 100) as f32)),
                        px(yb),
                    ));
                    let mut area = PathBuilder::fill();
                    area.add_polygon(&poly, true);
                    if let Ok(p) = area.build() {
                        window.paint_path(p, rgba(0xFFFFFF1C));
                    }
                    // Garis kurva tebal.
                    let mut line = PathBuilder::stroke(px(3.0));
                    line.move_to(poly[1].clone());
                    for q in poly.iter().skip(2).take(sorted.len().saturating_sub(1)) {
                        line.line_to(q.clone());
                    }
                    if let Ok(p) = line.build() {
                        window.paint_path(p, rgb(WHITE));
                    }
                    // Marker live: garis + dot tepat di kurva (interpolasi).
                    if let Some(lt) = live {
                        let ltf = lt.clamp(20, 100) as f32;
                        let lx = x_of(ltf);
                        let mut m = PathBuilder::stroke(px(1.5));
                        m.move_to(point(px(lx), px(oy + 4.0)));
                        m.line_to(point(px(lx), px(oy + h - 4.0)));
                        if let Ok(p) = m.build() {
                            window.paint_path(p, rgb(DIM));
                        }
                        let ld = chart_duty_at(&sorted, lt.clamp(20, 100));
                        let ly = y_of(ld as f32);
                        let mut dot = PathBuilder::fill();
                        let r = 5.0;
                        dot.add_polygon(
                            &[
                                point(px(lx - r), px(ly)),
                                point(px(lx), px(ly - r)),
                                point(px(lx + r), px(ly)),
                                point(px(lx), px(ly + r)),
                            ],
                            true,
                        );
                        if let Ok(p) = dot.build() {
                            window.paint_path(p, rgb(WHITE));
                        }
                    }
                    // Titik: besar; yang sedang di-drag lebih besar.
                    let drag_val = drag.and_then(|di| pts.get(di)).copied();
                    for (t, d) in sorted.iter() {
                        let x = x_of((*t).clamp(20, 100) as f32);
                        let y = y_of(*d as f32);
                        let r = if drag_val == Some((*t, *d)) { 8.0 } else { 5.0 };
                        let mut dot = PathBuilder::fill();
                        dot.add_polygon(
                            &[
                                point(px(x - r), px(y)),
                                point(px(x), px(y - r)),
                                point(px(x + r), px(y)),
                                point(px(x), px(y + r)),
                            ],
                            true,
                        );
                        if let Ok(p) = dot.build() {
                            window.paint_path(p, rgb(WHITE));
                        }
                    }
                },
            )
            .w_full()
            .h(px(150.)),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |v, ev: &MouseDownEvent, _window, cx| {
                let b = match cell_down.get() {
                    Some(b) => b,
                    None => return,
                };
                let w: f32 = b.size.width.into();
                let h: f32 = b.size.height.into();
                let ox: f32 = b.origin.x.into();
                let oy: f32 = b.origin.y.into();
                let ex: f32 = ev.position.x.into();
                let ey: f32 = ev.position.y.into();
                let (lx, ly) = (ex - ox, ey - oy);
                let mut best: Option<(usize, f32)> = None;
                for (i, (t, d)) in v.curve.iter().enumerate() {
                    let (qx, qy) = chart_data_to_xy(*t, *d, w, h);
                    let dist = ((lx - qx).powi(2) + (ly - qy).powi(2)).sqrt();
                    if dist < 24.0 && best.map_or(true, |(_, bd)| dist < bd) {
                        best = Some((i, dist));
                    }
                }
                if let Some((i, _)) = best {
                    v.drag_point = Some(i);
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(cx.listener(move |v, ev: &MouseMoveEvent, _window, cx| {
            let i = match v.drag_point {
                Some(i) => i,
                None => return,
            };
            let b = match cell_move.get() {
                Some(b) => b,
                None => return,
            };
            let w: f32 = b.size.width.into();
            let h: f32 = b.size.height.into();
            let ox: f32 = b.origin.x.into();
            let oy: f32 = b.origin.y.into();
            let ex: f32 = ev.position.x.into();
            let ey: f32 = ev.position.y.into();
            let (t, d) = chart_xy_to_data(ex - ox, ey - oy, w, h);
            // Urutan titik dijaga: suhu terjepit di antara tetangga.
            let n = v.curve.len();
            let lo = if i > 0 { v.curve[i - 1].0 + 2 } else { 20 };
            let hi = if i + 1 < n { v.curve[i + 1].0 - 2 } else { 100 };
            let t = if hi >= lo { t.clamp(lo, hi) } else { v.curve[i].0 };
            if let Some(p) = v.curve.get_mut(i) {
                p.0 = t;
                p.1 = d;
            }
            v.mode = MODE_CUSTOM.to_string();
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|v, _ev: &MouseUpEvent, _window, cx| {
                if v.drag_point.take().is_some() {
                    v.sort_curve();
                    v.mark_custom();
                    v.curve_live(cx, "drag");
                    cx.notify();
                }
            }),
        )
}

fn edit_temp(v: &mut RootView, i: usize, delta: i32) {
    if let Some(p) = v.curve.get_mut(i) {
        p.0 = (p.0 + delta).clamp(20, 100);
    }
    v.sort_curve();
    v.mark_custom();
}

fn edit_duty(v: &mut RootView, i: usize, delta: i16) {
    if let Some(p) = v.curve.get_mut(i) {
        p.1 = (p.1 as i16 + delta)
            .clamp(fan::MIN_FAN_DUTY_PCT as i16, fan::MAX_FAN_DUTY_PCT as i16) as u8;
    }
    v.mark_custom();
}

// ---------- content builders ----------

impl RootView {
    fn mode_cards(&mut self, cx: &mut Context<Self>) -> Div {
        let mut row: Vec<Stateful<Div>> = Vec::new();
        for (m, hint) in [
            (MODE_QUIET, "hening · duty rendah"),
            (MODE_BALANCED, "harian · seimbang"),
            (MODE_ENT, "kasual · referensi"),
            (MODE_PERF, "gaming · agresif"),
        ] {
            let sel = self.mode == m;
            let name = m.to_string();
            row.push(mode_card(m, hint, sel, cx, move |v, _, _, cx| {
                // Klik mode = LANGSUNG tulis EC satu-kali (snapshot:
                // duty dihitung dari suhu CPU saat ini + kurva mode tsb).
                // Loop tracking kontinu tetap milik axiood di masa depan.
                let curve = mode_curve(&name);
                let snapshot = v
                    .data
                    .max_temp_c
                    .map(|t| fan::curve_duty(&curve, t))
                    .unwrap_or_else(|| {
                        curve.first().map(|(_, d)| *d).unwrap_or(fan::MIN_FAN_DUTY_PCT)
                    });
                v.mode = name.clone();
                v.curve = curve;
                v.fan_manual = true;
                if v.mode_auto {
                    v.manual_duty = snapshot;
                }
                let duty = v.manual_duty;
                let temp_txt = v
                    .data
                    .max_temp_c
                    .map_or("suhu —".to_string(), |t| format!("{t}°C"));
                let label = if v.mode_auto {
                    format!("mode {name} auto → kipas {duty}% (snapshot {temp_txt})")
                } else {
                    format!("mode {name} statis → kipas {duty}%")
                };
                v.spawn_fan_write(cx, FanWrite::Manual(duty), label);
                cx.notify();
            }));
        }
        let status = format!("● MODE AKTIF: {} — {}", self.mode.to_uppercase(), mode_desc(&self.mode));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .rounded_md()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .text_color(rgb(DIM))
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("// MODE_".to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_color(rgb(WHITE))
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .truncate()
                            .child(status),
                    ),
            )
            .child(div().flex().flex_row().gap_2().w_full().children(row))
            .child(
                div()
                    .text_color(rgb(FAINT))
                    .text_xs()
                    .child("Klik mode = langsung tulis EC satu-kali (duty dari suhu saat ini). Tracking kurva kontinu butuh axiood.".to_string()),
            )
    }

    /// Detail mode aktif (tab Performa): Auto = snapshot kurva ditulis
    /// satu-kali dari suhu saat ini; Statis = duty tetap pilihan sendiri
    /// via stepper. Semua aksi langsung tulis EC (tanpa pkexec).
    fn mode_detail_card(&mut self, cx: &mut Context<Self>) -> Div {
        let ec_sel = !self.fan_manual;
        let kurva_sel = self.fan_manual && self.mode_auto;
        let duty_txt = format!("{}%", self.manual_duty);
        let temp_txt = self
            .data
            .max_temp_c
            .map_or("suhu —".to_string(), |t| format!("{t}°C"));
        let snapshot = self
            .data
            .max_temp_c
            .map(|t| fan::curve_duty(&self.curve, t))
            .unwrap_or(self.manual_duty);
        let info = if ec_sel {
            "EC firmware mengatur penuh — kurva tidak aktif".to_string()
        } else if kurva_sel {
            format!("snapshot {temp_txt} → {snapshot}% · kurva {} (kontinu butuh axiood)", self.mode)
        } else {
            "duty tetap — tidak mengikuti suhu".to_string()
        };
        let duty_row: Div = if ec_sel {
            div()
                .flex()
                .flex_row()
                .items_center()
                .w_full()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_color(rgb(DIM))
                        .text_sm()
                        .truncate()
                        .child(info),
                )
        } else if kurva_sel {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .w_full()
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_color(rgb(DIM))
                        .text_sm()
                        .truncate()
                        .child(info),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(WHITE))
                        .text_size(px(34.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(duty_txt),
                )
        } else {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(btn("md-", "− 5%", false, cx, |v, _, _, cx| {
                            v.manual_duty =
                                v.manual_duty.saturating_sub(5).max(fan::MIN_FAN_DUTY_PCT);
                            v.mode_auto = false;
                            v.fan_manual = true;
                            let d = v.manual_duty;
                            let m = v.mode.clone();
                            v.spawn_fan_write(cx, FanWrite::Manual(d), format!("{m} statis → {d}%"));
                            cx.notify();
                        }))
                        .child(btn("md+", "+ 5%", false, cx, |v, _, _, cx| {
                            v.manual_duty = (v.manual_duty + 5).min(fan::MAX_FAN_DUTY_PCT);
                            v.mode_auto = false;
                            v.fan_manual = true;
                            let d = v.manual_duty;
                            let m = v.mode.clone();
                            v.spawn_fan_write(cx, FanWrite::Manual(d), format!("{m} statis → {d}%"));
                            cx.notify();
                        }))
                        .child(btn("md-max", "Max 100%", false, cx, |v, _, _, cx| {
                            v.mode_auto = false;
                            v.fan_manual = true;
                            v.manual_duty = fan::MAX_FAN_DUTY_PCT;
                            let m = v.mode.clone();
                            v.spawn_fan_write(
                                cx,
                                FanWrite::Manual(fan::MAX_FAN_DUTY_PCT),
                                format!("{m} statis → 100%"),
                            );
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(WHITE))
                        .text_size(px(34.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(duty_txt),
                )
        };
        card(
            &format!("MODE {}", self.mode.to_uppercase()),
            [
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .w_full()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(seg_opt("md-auto", "❄ Auto Kurva", kurva_sel, cx, |v, _, _, cx| {
                        let snap = v
                            .data
                            .max_temp_c
                            .map(|t| fan::curve_duty(&v.curve, t))
                            .unwrap_or(v.manual_duty);
                        let m = v.mode.clone();
                        let temp = v
                            .data
                            .max_temp_c
                            .map_or("suhu —".to_string(), |t| format!("{t}°C"));
                        v.mode_auto = true;
                        v.fan_manual = true;
                        v.manual_duty = snap;
                        v.spawn_fan_write(
                            cx,
                            FanWrite::Manual(snap),
                            format!("{m} auto → {snap}% (snapshot {temp})"),
                        );
                        cx.notify();
                    }))
                    .child(seg_opt("md-man", "⚙ Statis", !kurva_sel && !ec_sel, cx, |v, _, _, cx| {
                        let m = v.mode.clone();
                        let d = v.manual_duty;
                        v.mode_auto = false;
                        v.fan_manual = true;
                        v.spawn_fan_write(cx, FanWrite::Manual(d), format!("{m} statis → {d}%"));
                        cx.notify();
                    }))
                    .child(seg_opt("md-ec", "◉ Auto EC", ec_sel, cx, |v, _, _, cx| {
                        let m = v.mode.clone();
                        v.fan_manual = false;
                        v.spawn_fan_write(cx, FanWrite::Auto, format!("{m} → AUTO (EC)"));
                        cx.notify();
                    })),
                duty_row,
                div()
                    .text_color(rgb(FAINT))
                    .text_xs()
                    .child("Auto Kurva = snapshot kurva · Auto EC = firmware penuh · Statis = duty tetap. Tracking kontinu butuh axiood.".to_string()),
            ],
        )
    }

    fn sys_card(&self) -> Div {
        card(
            "SISTEM",
            [
                div().text_color(rgb(TEXT)).text_sm().truncate().child(self.cpu_model.clone()),
                div().text_color(rgb(DIM)).text_xs().truncate().child(self.product.clone()),
                kv("GOV", self.data.governor.clone()),
                kv("EPP", self.data.epp.clone()),
                kv("FREQ", self.data.cpu_freq_line.clone()),
            ],
        )
    }

    fn temp_card(&self) -> Div {
        card(
            "SUHU",
            [
                hero_number(
                    self.data.max_temp_c.map_or("-".to_string(), |t| format!("{t}°C")),
                    self.data.cpu_temp_line.clone(),
                ),
                kv("FREQ", self.data.cpu_freq_line.clone()),
                div().text_color(rgb(DIM)).text_xs().child("CPU usage".to_string()),
                segbar(self.data.cpu_usage_pct),
            ],
        )
    }

    fn gpu_card(&self) -> Div {
        let mut items: Vec<Div> = Vec::new();
        if self.data.gpus.is_empty() {
            items.push(div().text_color(rgb(DIM)).text_sm().child("GPU tidur / tak terdeteksi (Optimus?)".to_string()));
        }
        for g in &self.data.gpus {
            items.push(div().text_color(rgb(TEXT)).text_sm().truncate().child(g.name.clone()));
            items.push(hero_number(
                g.temp_c.map_or("-".to_string(), |t| format!("{t:.0}°C")),
                g.power_w.map_or("power —".to_string(), |p| format!("{p:.0} W")),
            ));
            items.push(div().text_color(rgb(DIM)).text_xs().child("Util".to_string()));
            items.push(segbar(g.usage_pct));
            items.push(kv(
                "CLOCK",
                g.clock_mhz.map_or("-".to_string(), |c| format!("{c} MHz")),
            ));
        }
        card("GPU", items)
    }

    /// Kartu kipas ringkas (Dashboard): duty + RPM + pratinjau kurva.
    fn fan_mini_card(&self) -> Div {
        let (rpm1, rpm2) = self.fan_rpms();
        let duty = self.shown_duty().map_or("-".to_string(), |d| format!("{d}%"));
        let state = if self.fan_manual {
            format!("● MANUAL {}%", self.manual_duty)
        } else {
            "● AUTO (EC)".to_string()
        };
        let preview = match self.data.max_temp_c {
            Some(t) => {
                let d = fan::curve_duty(&self.curve, t);
                format!("live {t}°C → {d}% · kurva {}", self.mode)
            }
            None => "live —".to_string(),
        };
        card(
            "KIPAS",
            [
                hero_number(duty, state),
                kv("FAN 1", format!("{rpm1} RPM")),
                kv("FAN 2", format!("{rpm2} RPM")),
                div().text_color(rgb(DIM)).text_xs().truncate().child(preview),
            ],
        )
    }

    fn fan_control_card(&mut self, cx: &mut Context<Self>) -> Div {
        let manual = self.fan_manual;
        let duty_txt = self.shown_duty().map_or("-".to_string(), |d| format!("{d}%"));
        let state = if manual {
            format!("● KONTROL: MANUAL {}% — langsung tulis EC", self.manual_duty)
        } else {
            "● KONTROL: AUTO (EC) — langsung tulis EC".to_string()
        };
        let (rpm1, rpm2) = self.fan_rpms();
        let preview = match self.data.max_temp_c {
            Some(t) => {
                let d = fan::curve_duty(&self.curve, t);
                format!("live {t}°C → {d}% · kurva {}", self.mode)
            }
            None => "live —".to_string(),
        };
        card(
            "KONTROL",
            [
                // segmented AUTO | MANUAL
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .w_full()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(seg_opt("fan-auto", "❄ Auto (EC)", !manual, cx, |v, _, _, cx| {
                        v.fan_manual = false;
                        v.spawn_fan_write(cx, FanWrite::Auto, "kontrol → AUTO (EC)".to_string());
                        cx.notify();
                    }))
                    .child(seg_opt("fan-man", "⚙ Manual", manual, cx, |v, _, _, cx| {
                        v.fan_manual = true;
                        v.mode_auto = false;
                        let d = v.manual_duty;
                        v.spawn_fan_write(cx, FanWrite::Manual(d), format!("kontrol → MANUAL {d}%"));
                        cx.notify();
                    })),
                div()
                    .text_color(rgb(WHITE))
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(state),
                // stepper
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(btn("fd-", "− 5%", false, cx, |v, _, _, cx| {
                                v.manual_duty =
                                    v.manual_duty.saturating_sub(5).max(fan::MIN_FAN_DUTY_PCT);
                                v.fan_manual = true;
                                v.mode_auto = false;
                                let d = v.manual_duty;
                                v.spawn_fan_write(cx, FanWrite::Manual(d), format!("manual → {d}%"));
                                cx.notify();
                            }))
                            .child(btn("fd+", "+ 5%", false, cx, |v, _, _, cx| {
                                v.manual_duty = (v.manual_duty + 5).min(fan::MAX_FAN_DUTY_PCT);
                                v.fan_manual = true;
                                v.mode_auto = false;
                                let d = v.manual_duty;
                                v.spawn_fan_write(cx, FanWrite::Manual(d), format!("manual → {d}%"));
                                cx.notify();
                            }))
                            .child(btn("fan-max", "Max 100%", false, cx, |v, _, _, cx| {
                                v.fan_manual = true;
                                v.mode_auto = false;
                                v.manual_duty = fan::MAX_FAN_DUTY_PCT;
                                v.spawn_fan_write(
                                    cx,
                                    FanWrite::Manual(fan::MAX_FAN_DUTY_PCT),
                                    "manual → 100%".to_string(),
                                );
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .text_color(rgb(WHITE))
                            .text_size(px(30.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .flex_shrink_0()
                            .child(duty_txt),
                    ),
                kv("FAN 1", format!("{rpm1} RPM")),
                kv("FAN 2", format!("{rpm2} RPM")),
                div().text_color(rgb(DIM)).text_xs().truncate().child(preview),
                div().text_color(rgb(FAINT)).text_xs().truncate().child(
                    self.data.ec_err.clone().unwrap_or_else(|| "EC: live".to_string()),
                ),
            ],
        )
    }

    fn curve_card(&mut self, cx: &mut Context<Self>) -> Div {
        let mut rows: Vec<Div> = Vec::new();
        for (i, (t, d)) in self.curve.clone().iter().enumerate() {
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .w_full()
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .text_sm()
                            .child(format!("≥ {t:>3}°C → {d:>3}%")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .child(btn_sm(&format!("ct-{i}-"), "−T", cx, move |v, _, _, cx| {
                                edit_temp(v, i, -5);
                                v.curve_live(cx, "edit");
                                cx.notify();
                            }))
                            .child(btn_sm(&format!("ct-{i}+"), "+T", cx, move |v, _, _, cx| {
                                edit_temp(v, i, 5);
                                v.curve_live(cx, "edit");
                                cx.notify();
                            }))
                            .child(btn_sm(&format!("cd-{i}-"), "−D", cx, move |v, _, _, cx| {
                                edit_duty(v, i, -5);
                                v.curve_live(cx, "edit");
                                cx.notify();
                            }))
                            .child(btn_sm(&format!("cd-{i}+"), "+D", cx, move |v, _, _, cx| {
                                edit_duty(v, i, 5);
                                v.curve_live(cx, "edit");
                                cx.notify();
                            }))
                            .child(btn_sm(&format!("cdel-{i}"), "×", cx, move |v, _, _, cx| {
                                if v.curve.len() > 1 {
                                    v.curve.remove(i);
                                    v.mark_custom();
                                    v.curve_live(cx, "edit");
                                }
                                cx.notify();
                            })),
                    ),
            );
        }
        let bounds_cell =
            std::rc::Rc::<std::cell::Cell<Option<Bounds<gpui::Pixels>>>>::new(
                std::cell::Cell::new(None),
            );
        let mut items: Vec<Div> = vec![
            div().w_full().child(curve_chart(
                self.curve.clone(),
                self.data.max_temp_c,
                self.drag_point,
                bounds_cell,
                cx,
            )),
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .w_full()
                .px(px(10.))
                .text_color(rgb(FAINT))
                .text_xs()
                .child(div().child("20°".to_string()))
                .child(div().child("40°".to_string()))
                .child(div().child("60°".to_string()))
                .child(div().child("80°".to_string()))
                .child(div().child("100°".to_string())),
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .flex_shrink_0()
                        .child(btn("c-add", "+ titik", false, cx, |v, _, _, cx| {
                    let (lt, _) = v.curve.last().copied().unwrap_or((20, 40));
                    v.curve.push(((lt + 10).min(100), fan::MAX_FAN_DUTY_PCT));
                    v.sort_curve();
                    v.mark_custom();
                    v.curve_live(cx, "titik+");
                    cx.notify();
                }))
                .child(btn("c-reset", "reset", false, cx, |v, _, _, cx| {
                    let m = v.mode.clone();
                    v.curve = mode_curve(&m);
                    v.notice = None;
                    v.curve_live(cx, "reset");
                    cx.notify();
                })),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_color(rgb(FAINT))
                        .text_xs()
                        .text_right()
                        .truncate()
                        .child("drag titik · ±T/±D presisi".to_string()),
                ),
        ];
        items.extend(rows);
        card("KURVA", items)
    }

    fn mem_card(&self) -> Div {
        card(
            "MEMORI",
            [
                segbar(self.data.mem_pct),
                div().text_color(rgb(TEXT)).text_sm().child(self.data.mem_line.clone()),
            ],
        )
    }

    fn bat_card(&self) -> Div {
        let mut items: Vec<Div> = Vec::new();
        if let Some(p) = self.data.bat_pct {
            items.push(hero_number(format!("{p:.0}%"), "kapasitas".to_string()));
            items.push(segbar(Some(p)));
        }
        if self.data.bat_rows.is_empty() {
            items.push(div().text_color(rgb(DIM)).text_sm().child("(baterai —)".to_string()));
        }
        for b in &self.data.bat_rows {
            items.push(div().text_color(rgb(TEXT)).text_sm().child(b.clone()));
        }
        card("BATERAI", items)
    }

    fn power_card(&self) -> Div {
        let watts = self.data.pkg_watts.map_or("-".to_string(), |p| format!("{p:.1} W"));
        card(
            "DAYA CPU",
            [
                hero_number(watts, "paket (RAPL)".to_string()),
                div().text_color(rgb(FAINT)).text_xs().child("read-only · counter butuh root".to_string()),
            ],
        )
    }

    /// Tulis backlight langsung (sysfs LED, aman; root) ke zona target
    /// (`None` = semua zona). Sinkron (±ms) + hasil ke toast.
    fn kbd_apply(
        &mut self,
        cx: &mut Context<Self>,
        brightness: u32,
        rgb: (u8, u8, u8),
        target: Option<usize>,
        label: String,
    ) {
        let devs = kbd::discover();
        let targets: Vec<&kbd::KbdBacklight> = match target {
            None => devs.iter().collect(),
            Some(i) => devs.get(i).into_iter().collect(),
        };
        if targets.is_empty() {
            self.toast = Some(Toast {
                text: format!("{label}: gagal: LED keyboard tak ada (quirk DKMS?)"),
                at: Instant::now(),
                error: true,
            });
            cx.notify();
            return;
        }
        let mut fails = 0;
        for d in &targets {
            if kbd::set(d, brightness, rgb).is_err() {
                fails += 1;
            }
        }
        self.toast = Some(Toast {
            text: if fails == 0 {
                format!("{label}: OK ({} zona)", targets.len())
            } else {
                format!("{label}: gagal di {fails}/{} zona", targets.len())
            },
            at: Instant::now(),
            error: fails != 0,
        });
        cx.notify();
    }

    /// Teks target zona: "semua" / Z1..Z3 / Numpad (urutan discover).
    fn kbd_target_txt(sel: Option<usize>) -> String {
        match sel {
            None => "semua".to_string(),
            Some(3) => "Numpad".to_string(),
            Some(i) => format!("Z{}", i + 1),
        }
    }

    /// State acuan zona target: (brightness, rgb); default zona 0.
    fn kbd_zone_state(&self, tgt: Option<usize>) -> (u32, (u8, u8, u8)) {
        tgt.and_then(|i| self.data.kbd_zones.get(i).copied()).unwrap_or((
            self.data.kbd_brightness.unwrap_or(0),
            self.data.kbd_rgb.unwrap_or((255, 255, 255)),
        ))
    }

    fn kbd_max(&self) -> u32 {
        if self.data.kbd_max == 0 {
            255
        } else {
            self.data.kbd_max
        }
    }

    /// Visualizer keyboard: 5×15 key menyala ikut warna × brightness
    /// per zona (dibagi rata ke zona terdeteksi).
    fn kbd_visual_card(&self) -> Div {
        let max = self.kbd_max().max(1) as f32;
        let nz = self.data.kbd_zones.len().clamp(1, 6);
        let mut rows: Vec<Div> = Vec::with_capacity(5);
        for _ in 0..5 {
            let mut groups: Vec<Vec<Div>> = (0..nz).map(|_| Vec::new()).collect();
            for c in 0..15 {
                let z = (c * nz / 15).min(nz - 1);
                let (b, col) = self.data.kbd_zones.get(z).copied().unwrap_or((0, (0, 0, 0)));
                let g = glow(col, b as f32 / max);
                let bc = if g == 0 { BORDER } else { g };
                groups[z].push(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .h(px(24.))
                        .rounded_sm()
                        .bg(rgb(PANEL2))
                        .border_1()
                        .border_color(rgb(bc)),
                );
            }
            rows.push(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .w_full()
                    .children(groups.into_iter().map(|keys| {
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(3.))
                            .flex_1()
                            .min_w(px(0.))
                            .children(keys)
                    })),
            );
        }
        let zone_txt = if self.data.kbd_zones.is_empty() {
            "LED tak ada".to_string()
        } else {
            self.data
                .kbd_zones
                .iter()
                .enumerate()
                .map(|(i, (b, (r, g, bl)))| format!("Z{} {b} #{r:02X}{g:02X}{bl:02X}", i + 1))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        card(
            "VISUAL",
            [
                div().flex().flex_col().gap_1().w_full().children(rows),
                div().text_color(rgb(DIM)).text_xs().truncate().child(zone_txt),
            ],
        )
    }

    fn kbd_status_card(&self) -> Div {        if self.data.kbd_nodes == 0 {
            return card(
                "KEYBOARD",
                [
                    div().text_color(rgb(TEXT)).text_sm().child("LED tak ada".to_string()),
                    div().text_color(rgb(DIM)).text_xs().child(
                        "butuh quirk DKMS 0x17 (packaging/clevo-drivers-axioo)".to_string(),
                    ),
                ],
            );
        }
        let rgb_txt = self
            .data
            .kbd_rgb
            .map_or("-".to_string(), |(r, g, b)| format!("#{r:02X}{g:02X}{b:02X}"));
        card(
            "KEYBOARD",
            [
                kv("NODE", format!("{} zona", self.data.kbd_nodes)),
                kv("MAX", self.data.kbd_max.to_string()),
                kv(
                    "NYALA",
                    match self.data.kbd_brightness {
                        Some(b) => format!("{b} · {rgb_txt}"),
                        None => "-".to_string(),
                    },
                ),
            ],
        )
    }

    fn kbd_bright_card(&mut self, cx: &mut Context<Self>) -> Div {
        let max = self.kbd_max();
        let cur = self.data.kbd_brightness.unwrap_or(0);
        let pct = Some(cur as f64 / max.max(1) as f64 * 100.0);
        card(
            "BRIGHTNESS",
            [
                hero_number(format!("{cur}"), format!("/ {max}")),
                segbar(pct),
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(btn("kb-", "−", false, cx, move |v, _, _, cx| {
                        let max = v.kbd_max();
                        let tgt = v.kbd_zone_sel;
                        let (cur, rgb) = v.kbd_zone_state(tgt);
                        let b = cur.saturating_sub((max / 10).max(1));
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, b, rgb, tgt, format!("kbd {t} brightness → {b}"));
                        cx.notify();
                    }))
                    .child(btn("kb+", "+", false, cx, move |v, _, _, cx| {
                        let max = v.kbd_max();
                        let tgt = v.kbd_zone_sel;
                        let (cur, mut rgb) = v.kbd_zone_state(tgt);
                        if rgb == (0, 0, 0) {
                            rgb = (255, 255, 255);
                        }
                        let b = cur.saturating_add((max / 10).max(1)).min(max);
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, b, rgb, tgt, format!("kbd {t} brightness → {b}"));
                        cx.notify();
                    }))
                    .child(btn("kb-max", "Max", false, cx, move |v, _, _, cx| {
                        let max = v.kbd_max();
                        let tgt = v.kbd_zone_sel;
                        let (_, mut rgb) = v.kbd_zone_state(tgt);
                        if rgb == (0, 0, 0) {
                            rgb = (255, 255, 255);
                        }
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, max, rgb, tgt, format!("kbd {t} brightness → {max}"));
                        cx.notify();
                    }))
                    .child(btn("kb-off", "Off", false, cx, move |v, _, _, cx| {
                        let tgt = v.kbd_zone_sel;
                        let (_, rgb) = v.kbd_zone_state(tgt);
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, 0, rgb, tgt, format!("kbd {t} off"));
                        cx.notify();
                    })),
            ],
        )
    }

    /// Slider RGB custom: 3 bar drag (R/G/B). Pratinjau saat drag,
    /// tulis ke zona target saat dilepas.
    fn rgb_sliders(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let cell: std::rc::Rc<std::cell::Cell<Option<Bounds<gpui::Pixels>>>> =
            std::rc::Rc::new(std::cell::Cell::new(None));
        let cell_down = cell.clone();
        let cell_move = cell.clone();
        let cur = self.custom_rgb;
        div()
            .id(SharedString::from("rgb-picker"))
            .w_full()
            .child(
                canvas(
                    move |_bounds, _window, _cx| cur,
                    move |bounds: Bounds<gpui::Pixels>,
                          col: (u8, u8, u8),
                          window: &mut Window,
                          _cx: &mut App| {
                        cell.set(Some(bounds));
                        let w: f32 = bounds.size.width.into();
                        let h: f32 = bounds.size.height.into();
                        let ox: f32 = bounds.origin.x.into();
                        let oy: f32 = bounds.origin.y.into();
                        let vals = [col.0, col.1, col.2];
                        let row_h = (h - 2.0 * RGB_PAD) / RGB_ROWS as f32;
                        for c in 0..3 {
                            let y0 = oy + RGB_PAD + c as f32 * row_h + 5.0;
                            let y1 = oy + RGB_PAD + (c + 1) as f32 * row_h - 5.0;
                            for s in 0..32 {
                                let x0 = ox + RGB_PAD + s as f32 / 32.0 * (w - 2.0 * RGB_PAD);
                                let x1 =
                                    ox + RGB_PAD + (s + 1) as f32 / 32.0 * (w - 2.0 * RGB_PAD);
                                let mut seg = PathBuilder::fill();
                                seg.add_polygon(
                                    &[
                                        point(px(x0), px(y0)),
                                        point(px(x1), px(y0)),
                                        point(px(x1), px(y1)),
                                        point(px(x0), px(y1)),
                                    ],
                                    true,
                                );
                                if let Ok(p) = seg.build() {
                                    window.paint_path(p, rgb(rgb_track_color(c, s as f32 / 31.0)));
                                }
                            }
                            let kx = ox + rgb_knob_x(vals[c], w);
                            let mut knob = PathBuilder::stroke(px(2.0));
                            knob.move_to(point(px(kx), px(y0 - 2.0)));
                            knob.line_to(point(px(kx), px(y1 + 2.0)));
                            if let Ok(p) = knob.build() {
                                window.paint_path(p, rgb(WHITE));
                            }
                        }
                    },
                )
                .w_full()
                .h(px(108.)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |v, ev: &MouseDownEvent, _window, cx| {
                    let b = match cell_down.get() {
                        Some(b) => b,
                        None => return,
                    };
                    let w: f32 = b.size.width.into();
                    let h: f32 = b.size.height.into();
                    let ox: f32 = b.origin.x.into();
                    let oy: f32 = b.origin.y.into();
                    let ex: f32 = ev.position.x.into();
                    let ey: f32 = ev.position.y.into();
                    let (ch, val) = rgb_xy_to_val(ex - ox, ey - oy, w, h);
                    match ch {
                        0 => v.custom_rgb.0 = val,
                        1 => v.custom_rgb.1 = val,
                        _ => v.custom_rgb.2 = val,
                    }
                    v.drag_rgb = Some(ch);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |v, ev: &MouseMoveEvent, _window, cx| {
                let ch = match v.drag_rgb {
                    Some(c) => c,
                    None => return,
                };
                let b = match cell_move.get() {
                    Some(b) => b,
                    None => return,
                };
                let w: f32 = b.size.width.into();
                let h: f32 = b.size.height.into();
                let ox: f32 = b.origin.x.into();
                let oy: f32 = b.origin.y.into();
                let ex: f32 = ev.position.x.into();
                let ey: f32 = ev.position.y.into();
                let (_, val) = rgb_xy_to_val(ex - ox, ey - oy, w, h);
                match ch {
                    0 => v.custom_rgb.0 = val,
                    1 => v.custom_rgb.1 = val,
                    _ => v.custom_rgb.2 = val,
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|v, _ev: &MouseUpEvent, _window, cx| {
                    if v.drag_rgb.take().is_some() {
                        let max = v.kbd_max();
                        let tgt = v.kbd_zone_sel;
                        let (cur_b, _) = v.kbd_zone_state(tgt);
                        let b = if cur_b == 0 { max } else { cur_b };
                        let rgb = v.custom_rgb;
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, b, rgb, tgt, format!("kbd {t} custom"));
                        cx.notify();
                    }
                }),
            )
    }

    /// Popup color picker custom RGB.
    fn rgb_popup_card(&mut self, cx: &mut Context<Self>) -> Div {
        let (r, g, b) = self.custom_rgb;
        let hex = (r as u32) << 16 | (g as u32) << 8 | b as u32;
        let items: Vec<AnyElement> = vec![
            div()
                .w_full()
                .h(px(28.))
                .rounded_md()
                .bg(rgb(hex))
                .border_1()
                .border_color(rgb(BORDER))
                .into_any_element(),
            div().w_full().child(self.rgb_sliders(cx)).into_any_element(),
            div()
                .text_color(rgb(DIM))
                .text_sm()
                .truncate()
                .child(format!("R {r} · G {g} · B {b} · #{hex:06X}"))
                .into_any_element(),
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(btn("rgb-apply", "Terapkan", true, cx, |v, _, _, cx| {
                    let max = v.kbd_max();
                    let tgt = v.kbd_zone_sel;
                    let (cur_b, _) = v.kbd_zone_state(tgt);
                    let bb = if cur_b == 0 { max } else { cur_b };
                    let rgb = v.custom_rgb;
                    let t = Self::kbd_target_txt(tgt);
                    v.kbd_apply(cx, bb, rgb, tgt, format!("kbd {t} custom"));
                    cx.notify();
                }))
                .child(btn("rgb-close", "Tutup", false, cx, |v, _, _, cx| {
                    v.show_rgb_popup = false;
                    cx.notify();
                }))
                .into_any_element(),
        ];
        card("CUSTOM RGB", items)
    }

    fn kbd_color_card(&mut self, cx: &mut Context<Self>) -> Div {
        const PRESETS: [(&str, (u8, u8, u8), u32); 7] = [
            ("red", (255, 0, 0), 0xFF0000),
            ("yellow", (255, 255, 0), 0xFFFF00),
            ("green", (0, 255, 0), 0x00FF00),
            ("cyan", (0, 255, 255), 0x00FFFF),
            ("blue", (0, 0, 255), 0x0000FF),
            ("white", (255, 255, 255), 0xFFFFFF),
            ("off", (0, 0, 0), 0x000000),
        ];
        let cur_rgb = self.data.kbd_rgb;
        let mut tiles: Vec<Stateful<Div>> = Vec::new();
        for (name, col, hex) in PRESETS {
            let sel = cur_rgb == Some(col);
            let label = name.to_string();
            let dot = if name == "off" {
                div()
                    .w(px(16.))
                    .h(px(16.))
                    .rounded_sm()
                    .bg(rgb(0x000000))
                    .border_1()
                    .border_color(rgb(BORDER))
            } else {
                div().w(px(16.)).h(px(16.)).rounded_sm().bg(rgb(hex))
            };
            tiles.push(
                div()
                    .id(SharedString::from(format!("kbd-{name}")))
                    .flex_1()
                    .min_w(px(96.))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(rgb(PANEL2))
                    .border_1()
                    .border_color(rgb(if sel { WHITE } else { BORDER }))
                    .cursor_pointer()
                    .hover(move |s| s.bg(rgb(lighten(PANEL2, 0.2))))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(dot)
                            .child(
                                div()
                                    .text_color(rgb(if sel { WHITE } else { TEXT }))
                                    .text_sm()
                                    .whitespace_nowrap()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(format!("{}{}", if sel { "● " } else { "" }, label)),
                            ),
                    )
                    .on_click(cx.listener(move |v, _, _, cx| {
                        let max = v.kbd_max();
                        let tgt = v.kbd_zone_sel;
                        let (cur, _) = v.kbd_zone_state(tgt);
                        let b = if label == "off" {
                            0
                        } else if cur == 0 {
                            max
                        } else {
                            cur
                        };
                        let t = Self::kbd_target_txt(tgt);
                        v.kbd_apply(cx, b, col, tgt, format!("kbd {t} {label}"));
                        cx.notify();
                    })),
            );
        }
        // Tile Custom: buka/tutup popup color picker.
        let custom_hex =
            (self.custom_rgb.0 as u32) << 16 | (self.custom_rgb.1 as u32) << 8 | self.custom_rgb.2 as u32;
        let custom_open = self.show_rgb_popup;
        tiles.push(
            div()
                .id(SharedString::from("kbd-custom"))
                .flex_1()
                .min_w(px(96.))
                .px_3()
                .py_2()
                .rounded_md()
                .bg(rgb(PANEL2))
                .border_1()
                .border_color(rgb(if custom_open { WHITE } else { BORDER }))
                .cursor_pointer()
                .hover(move |s| s.bg(rgb(lighten(PANEL2, 0.2))))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(16.))
                                .h(px(16.))
                                .rounded_sm()
                                .bg(rgb(custom_hex))
                                .border_1()
                                .border_color(rgb(BORDER)),
                        )
                        .child(
                            div()
                                .text_color(rgb(if custom_open { WHITE } else { TEXT }))
                                .text_sm()
                                .whitespace_nowrap()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(format!("{}Custom", if custom_open { "● " } else { "" })),
                        ),
                )
                .on_click(cx.listener(|v, _, _, cx| {
                    if v.show_rgb_popup {
                        v.show_rgb_popup = false;
                    } else {
                        let tgt = v.kbd_zone_sel;
                        let (_, rgb) = v.kbd_zone_state(tgt);
                        v.custom_rgb = rgb;
                        v.show_rgb_popup = true;
                    }
                    cx.notify();
                })),
        );
        card(
            "WARNA",
            [
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .w_full()
                    .p_1()
                    .rounded_md()
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(seg_opt("kbz-all", "Semua", self.kbd_zone_sel.is_none(), cx, |v, _, _, cx| {
                        v.kbd_zone_sel = None;
                        cx.notify();
                    }))
                    .child(seg_opt("kbz-0", "Kiri", self.kbd_zone_sel == Some(0), cx, |v, _, _, cx| {
                        v.kbd_zone_sel = Some(0);
                        cx.notify();
                    }))
                    .child(seg_opt("kbz-1", "Tengah", self.kbd_zone_sel == Some(1), cx, |v, _, _, cx| {
                        v.kbd_zone_sel = Some(1);
                        cx.notify();
                    }))
                    .child(seg_opt("kbz-2", "Kanan", self.kbd_zone_sel == Some(2), cx, |v, _, _, cx| {
                        v.kbd_zone_sel = Some(2);
                        cx.notify();
                    }))
                    .child(seg_opt("kbz-3", "Numpad", self.kbd_zone_sel == Some(3), cx, |v, _, _, cx| {
                        v.kbd_zone_sel = Some(3);
                        cx.notify();
                    })),
                div().flex().flex_row().flex_wrap().gap_2().w_full().children(tiles),
                div().text_color(rgb(FAINT)).text_xs().child(
                    "target zona di atas — berlaku untuk warna + brightness".to_string(),
                ),
            ],
        )
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.tab;

        // ---- sidebar nav ----
        let mut nav: Vec<Div> = Vec::new();
        let mut last_section = "";
        for t in Tab::all() {
            if t.section() != last_section {
                last_section = t.section();
                nav.push(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .w_full()
                        .child(div().text_color(rgb(FAINT)).text_xs().child(last_section.to_string()))
                        .child(div().h(px(1.)).flex_1().bg(rgb(BORDER))),
                );
            }
            let sel = tab == t;
            nav.push(div().w_full().child(nav_item(t, sel, cx, move |v, _, _, cx| {
                v.tab = t;
                v.scroll_px = 0.0;
                cx.notify();
            })));
        }

        // ---- cluster status kanan: tiga pill sejajar ----
        let is_root = fan_ctrl::is_root();
        let ec_live = self.data.ec.is_some();
        let ec_txt = self.data.ec.map_or("○ EC butuh root".to_string(), |s| {
            format!("● EC {}°C · {} RPM", s.cpu_temp_raw, s.fan1_rpm)
        });

        // ---- tab content ----
        let content: Div = match tab {
            Tab::Dashboard => div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .w_full()
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.temp_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.fan_mini_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.sys_card())),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .w_full()
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.gpu_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.mem_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.bat_card())),
                ),
            Tab::Performa => div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(self.mode_cards(cx))
                .child(self.mode_detail_card(cx))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .w_full()
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.temp_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.gpu_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.power_card())),
                ),
            Tab::Kipas => div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .w_full()
                        .child(div().flex().flex_col().w(px(340.)).flex_shrink_0().child(self.fan_control_card(cx)))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.curve_card(cx))),
                ),
            Tab::Keyboard => div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(div().w_full().child(self.kbd_visual_card()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_start()
                        .gap_2()
                        .w_full()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.kbd_status_card()),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.kbd_bright_card(cx)),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w(px(0.))
                        .child(self.kbd_color_card(cx)),
                ),
            Tab::Daya => div()
                .flex()
                .flex_col()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .w_full()
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.bat_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.power_card()))
                        .child(div().flex().flex_col().flex_1().min_w(px(0.)).child(self.mem_card())),
                ),
        };

        let notice = self.notice.clone().unwrap_or_else(|| "siap.".to_string());

        // ---- toast: mengambang kanan-bawah, tak menggeser layout ----
        let toast_el: AnyElement = match &self.toast {
            Some(t) => div()
                .id(SharedString::from("toast"))
                .absolute()
                .bottom(px(60.))
                .right(px(16.))
                .max_w(px(460.))
                .p_3()
                .rounded_md()
                .bg(rgb(PANEL2))
                .border_1()
                .border_color(rgb(if t.error { WHITE } else { BORDER }))
                .cursor_pointer()
                .child(
                    div()
                        .text_color(rgb(if t.error { WHITE } else { DIM }))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(
                            if t.error {
                                "■ GAGAL — klik untuk tutup".to_string()
                            } else {
                                "■ OK — klik untuk tutup".to_string()
                            },
                        ),
                )
                .child(
                    div()
                        .text_color(rgb(TEXT))
                        .text_sm()
                        .line_clamp(4)
                        .child(t.text.clone()),
                )
                .on_click(cx.listener(|v, _, _, cx| {
                    v.toast = None;
                    cx.notify();
                }))
                .into_any_element(),
            None => div().into_any_element(),
        };

        // ---- scrollbar ala kit: thumb dari ukur frame lalu ----
        let meas = self.scroll_meas.get();
        let scroll_max = self.scroll_max();
        let scrollbar_el: AnyElement = if scroll_max > 1.0 && meas.view_h > 0.0 {
            let content_h = (scroll_max + meas.view_h).max(1.0);
            let thumb_h = (meas.view_h * meas.view_h / content_h).clamp(24.0, meas.view_h);
            let thumb_y = self.scroll_px / scroll_max * (meas.view_h - thumb_h);
            div()
                .absolute()
                .top(px(thumb_y))
                .right(px(2.))
                .w(px(4.))
                .h(px(thumb_h))
                .rounded_sm()
                .bg(rgb(DIM))
                .into_any_element()
        } else {
            div().into_any_element()
        };
        let meas_view = self.scroll_meas.clone();
        let meas_end = self.scroll_meas.clone();
        let meas_abs = self.scroll_meas.clone();
        let scroll_offset = self.scroll_px;

        // ---- popup custom RGB: kanan-atas, hanya di tab Keyboard ----
        let popup_el: AnyElement = if self.show_rgb_popup && self.tab == Tab::Keyboard {
            div()
                .absolute()
                .top(px(56.))
                .right(px(16.))
                .w(px(300.))
                .child(self.rgb_popup_card(cx))
                .into_any_element()
        } else {
            div().into_any_element()
        };

        div()
            .size_full()
            .flex()
            .flex_row()
            .relative()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family(FONT)
            .text_sm()
            // ===== sidebar =====
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .w(px(248.))
                    .h_full()
                    .bg(rgb(PANEL))
                    .border_r_1()
                    .border_color(rgb(BORDER))
                    .child(
                        // header block
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .w_full()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_color(rgb(WHITE))
                                                    .text_size(px(26.))
                                                    .font_weight(gpui::FontWeight::BOLD)
                                                    .child("力".to_string()),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .child(
                                                        div()
                                                            .text_color(rgb(WHITE))
                                                            .text_sm()
                                                            .font_weight(gpui::FontWeight::BOLD)
                                                            .child("AXIOO".to_string()),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_color(rgb(DIM))
                                                            .text_xs()
                                                            .child("//CONTROL_".to_string()),
                                                    ),
                                            ),
                                    )
                                    .child(div().text_color(rgb(FAINT)).text_xs().child("///".to_string())),
                            )
                            .child(div().text_color(rgb(FAINT)).text_xs().child(self.product.clone())),
                    )
                    .children(nav)
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .px_2()
                                            .rounded_sm()
                                            .bg(rgb(PANEL2))
                                            .border_1()
                                            .border_color(rgb(BORDER))
                                            .text_color(rgb(DIM))
                                            .text_xs()
                                            .child("STABLE // 01".to_string()),
                                    ),
                            )
                            .child(barcode())
                    ),
            )
            // ===== main column =====
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        // top breadcrumb bar
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_color(rgb(FAINT)).text_sm().child("— 力".to_string()))
                                    .child(
                                        div()
                                            .text_color(rgb(DIM))
                                            .text_xs()
                                            .child(tab.section().to_string()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(status_pill(
                                        if is_root {
                                            "● ROOT".to_string()
                                        } else {
                                            "○ USER".to_string()
                                        },
                                        is_root,
                                    ))
                                    .child(status_pill(ec_txt, ec_live))
                                    .child(status_pill(self.data.stamp.clone(), false)),
                            ),
                    )
                    // title block
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .px_4()
                            .pt_3()
                            .child(
                                div()
                                    .text_color(rgb(WHITE))
                                    .text_size(px(32.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(tab.label().to_string()),
                            )
                            .child(div().text_color(rgb(DIM)).text_sm().child(tab.desc().to_string())),
                    )
                    .child(
                        div()
                            .id(SharedString::from("scroll-body"))
                            .flex_1()
                            .min_h(px(0.))
                            .relative()
                            .overflow_hidden()
                            .child(
                                div()
                                    .w_full()
                                    .mt(px(-scroll_offset))
                                    .px_4()
                                    .py_3()
                                    .child(
                                        canvas(
                                            move |_bounds, _window, _cx| (),
                                            move |bounds: Bounds<gpui::Pixels>,
                                                  _: (),
                                                  _: &mut Window,
                                                  _: &mut App| {
                                                let oy: f32 = bounds.origin.y.into();
                                                let mut m = meas_view.get();
                                                m.start_y = oy;
                                                meas_view.set(m);
                                            },
                                        )
                                        .w_full()
                                        .h(px(1.)),
                                    )
                                    .child(content)
                                    .child(
                                        canvas(
                                            move |_bounds, _window, _cx| (),
                                            move |bounds: Bounds<gpui::Pixels>,
                                                  _: (),
                                                  _: &mut Window,
                                                  _: &mut App| {
                                                let oy: f32 = bounds.origin.y.into();
                                                let h: f32 = bounds.size.height.into();
                                                let mut m = meas_end.get();
                                                m.end_y = oy + h;
                                                meas_end.set(m);
                                            },
                                        )
                                        .w_full()
                                        .h(px(1.)),
                                    ),
                            )
                            .child(scrollbar_el)
                            .child(
                                canvas(
                                    move |_bounds, _window, _cx| (),
                                    move |bounds: Bounds<gpui::Pixels>,
                                          _: (),
                                          _: &mut Window,
                                          _: &mut App| {
                                        let h: f32 = bounds.size.height.into();
                                        let mut m = meas_abs.get();
                                        m.view_h = h;
                                        meas_abs.set(m);
                                    },
                                )
                                .absolute()
                                .top(px(0.))
                                .bottom(px(0.))
                                .left(px(0.))
                                .right(px(0.)),
                            )
                            .on_scroll_wheel(|ev, window, cx| {
                                if let Some(Some(root)) = window.root::<RootView>() {
                                    cx.update_entity(&root, |v, _| {
                                        let dy: f32 = ev.delta.pixel_delta(px(16.)).y.into();
                                        v.scroll_px = (v.scroll_px + dy).clamp(0.0, v.scroll_max());
                                    });
                                    window.refresh();
                                }
                            }),
                    )
                    // bottom action bar
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .px_4()
                            .py_2()
                            .border_t_1()
                            .border_color(rgb(BORDER))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_color(rgb(FAINT))
                                            .text_sm()
                                            .child("///".to_string()),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .text_color(rgb(DIM))
                                            .text_xs()
                                            .truncate()
                                            .child(format!("■ {notice}")),
                                    ),
                            )
                            .child(div().text_color(rgb(FAINT)).text_sm().child("制御 開".to_string())),
                    ),
            )
            .child(toast_el)
            .child(popup_el)
    }
}

fn f1(v: Option<f64>, unit: &str) -> String {
    v.map_or_else(|| "-".to_string(), |x| format!("{x:.1}{unit}"))
}

struct Sampler {
    prev_stat: Option<CpuTimes>,
    prev_rapl: Vec<rapl::RaplDomain>,
    prev_t: Instant,
    n: u64,
}

fn sample(s: &mut Sampler) -> SensorData {
    let now = Instant::now();
    let dt = now.duration_since(s.prev_t).as_secs_f64().max(0.01);
    s.prev_t = now;
    s.n += 1;

    let c = cpu::sample();
    let cpu_temp_line = format!("{} / {}", f1(c.package_temp_c, "°C"), f1(c.max_core_temp_c, "°C"));
    let cpu_freq_line = format!("{} / {}", f1(c.avg_mhz, "MHz"), f1(c.max_mhz, "MHz"));
    let max_temp_c = c
        .package_temp_c
        .into_iter()
        .chain(c.max_core_temp_c)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .map(|v| v as i32);

    let cur_stat = cpu::read_times();
    let cpu_usage_pct = match (s.prev_stat, cur_stat) {
        (Some(p), Some(q)) => cpu::usage_between(&p, &q),
        _ => None,
    };
    if cur_stat.is_some() {
        s.prev_stat = cur_stat;
    }

    let cur_rapl = rapl::domains();
    let pkg_watts = cur_rapl
        .iter()
        .find(|d| d.id.ends_with(":0"))
        .and_then(|after| {
            s.prev_rapl
                .iter()
                .find(|b| b.id == after.id)
                .and_then(|before| rapl::watts(before, after, dt))
        })
        .or_else(|| {
            cur_rapl.iter().find_map(|after| {
                s.prev_rapl
                    .iter()
                    .find(|b| b.id == after.id)
                    .and_then(|before| rapl::watts(before, after, dt))
            })
        });
    s.prev_rapl = cur_rapl;

    let gpus: Vec<GpuRow> = nvidia::gpus()
        .unwrap_or_default()
        .iter()
        .map(|g| GpuRow {
            name: format!("{} · {}", g.name, g.temp_c.map_or("-".to_string(), |v| format!("{v:.0}°C"))),
            usage_pct: g.usage_pct,
            temp_c: g.temp_c,
            power_w: g.power_w,
            clock_mhz: g.gr_clock_mhz,
        })
        .collect();

    // Nilai = angka RPM saja (label sudah di header `FAN 1` / `FAN 2`
    // di kartu, agar tidak menempel jadi `1fan1 3401`).
    let fan_rows = hwmon::fans().iter().map(|f| format!("{}", f.rpm)).collect();

    let bats = battery::batteries();
    let bat_pct = bats.first().and_then(|b| b.capacity_pct);
    let bat_rows = bats
        .iter()
        .map(|b| {
            format!(
                "{} {}% {} {}",
                b.name,
                b.capacity_pct.map_or("-".to_string(), |x| format!("{x:.0}")),
                b.status.as_deref().unwrap_or("?"),
                f1(b.power_w, "W"),
            )
        })
        .collect();

    let (mem_pct, mem_line) = match memory::read() {
        Some(m) => (Some(m.used_pct()), format!("{:.1} / {:.1} GB", m.used_gb(), m.total_gb())),
        None => (None, "-".to_string()),
    };

    let (ec, ec_err) = match ec::read_map() {
        Ok(m) => (Some(fan::snapshot(&m)), None),
        Err(e) => {
            // Pesan mentah OS terlalu panjang untuk kartu ("Permission denied
            // (os error 13)... debugfs"); ringkas agar layout tidak jebol.
            let msg = e.to_string();
            let short = if msg.contains("Permission denied") || msg.contains("os error 13") {
                "EC: butuh root — baca via sudo/pkexec".to_string()
            } else {
                msg.chars().take(160).collect()
            };
            (None, Some(short))
        }
    };

    let kbds = kbd::discover();
    let mut kbd_zones: Vec<(u32, (u8, u8, u8))> = Vec::new();
    for kb in &kbds {
        if let Some(s) = kbd::read_state(kb) {
            kbd_zones.push((s.brightness, s.rgb));
        }
    }
    let (kbd_nodes, kbd_max, kbd_brightness, kbd_rgb) = match kbds.first() {
        Some(kb) => (
            kbds.len(),
            kb.max_brightness,
            kbd::read_state(kb).map(|s| s.brightness),
            kbd::read_state(kb).map(|s| s.rgb),
        ),
        None => (0, 0, None, None),
    };

    SensorData {
        cpu_temp_line,
        cpu_freq_line,
        cpu_usage_pct,
        governor: c.governor.clone().unwrap_or_else(|| "?".to_string()),
        epp: c.epp.clone().unwrap_or_else(|| "?".to_string()),
        gpus,
        fan_rows,
        bat_rows,
        bat_pct,
        mem_pct,
        mem_line,
        pkg_watts,
        ec,
        ec_err,
        max_temp_c,
        kbd_nodes,
        kbd_max,
        kbd_brightness,
        kbd_rgb,
        kbd_zones,
        stamp: format!("live · update #{}", s.n),
    }
}

/// App harus jalan sebagai root: kalau euid != 0, relaunch diri via
/// pkexec (prompt sekali di awal), lalu proses ini menunggu sampai GUI
/// root ditutup. Env Wayland diteruskan agar window bisa dibuka.
/// Batal/gagal → lanjut tanpa root (tulis EC pakai fallback per-aksi).
/// Bypass (dev): `AXIOO_ALLOW_USER=1`.
fn ensure_root_or_relaunch() {
    if fan_ctrl::is_root() || std::env::var("AXIOO_ALLOW_USER").is_ok() {
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cmd = std::process::Command::new("pkexec");
    cmd.arg("env");
    // Display/Wayland penting agar window root bisa dibuka di sesi user.
    // XAUTHORITY wajib eksplisit: tanpa ini Xlib jatuh ke /root/.Xauthority
    // ("Authorization required, but no authorization protocol specified").
    let xauth = std::env::var("XAUTHORITY").ok().filter(|v| !v.is_empty()).or_else(|| {
        std::env::var("HOME").ok().map(|h| format!("{h}/.Xauthority"))
    });
    for (key, val) in [
        ("WAYLAND_DISPLAY", std::env::var("WAYLAND_DISPLAY").ok()),
        ("XDG_RUNTIME_DIR", std::env::var("XDG_RUNTIME_DIR").ok()),
        ("DISPLAY", std::env::var("DISPLAY").ok()),
        ("XAUTHORITY", xauth),
    ] {
        if let Some(v) = val {
            if !v.is_empty() {
                cmd.arg(format!("{key}={v}"));
            }
        }
    }
    cmd.arg(&exe);
    cmd.args(&args);
    match cmd.status() {
        Ok(st) if st.success() => std::process::exit(0),
        Ok(st) => eprintln!(
            "axioo-control-center: pkexec dibatalkan/gagal ({st}); lanjut tanpa root."
        ),
        Err(e) => eprintln!(
            "axio-control-center: pkexec tak bisa dijalankan ({e}); lanjut tanpa root."
        ),
    }
}

fn main() {
    eprintln!("axioo-control-center: starting (GUI)…");
    ensure_root_or_relaunch();
    Application::new().run(|app: &mut App| {
        let product = dmi::read_dmi()
            .get("product_name")
            .cloned()
            .unwrap_or_else(|| "Axioo".to_string());
        let cpu_model = cpu::model_name().unwrap_or_else(|| "CPU".to_string());

        let (tx, rx) = std::sync::mpsc::channel::<SensorData>();
        let (apply_tx, apply_rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut s = Sampler {
                prev_stat: cpu::read_times(),
                prev_rapl: rapl::domains(),
                prev_t: Instant::now(),
                n: 0,
            };
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let data = sample(&mut s);
                if tx.send(data).is_err() {
                    break;
                }
            }
        });

        let view = app.new(|_| RootView::new(product, cpu_model, apply_tx));

        let bounds = Bounds::centered(None, gpui::size(px(1280.), px(840.)), app);
        app.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            {
                let view = view.clone();
                move |_, _| view
            },
        )
        .unwrap();

        app.spawn(async move |cx: &mut AsyncApp| {
            loop {
                Timer::after(Duration::from_millis(250)).await;
                let mut latest: Option<SensorData> = None;
                loop {
                    match rx.try_recv() {
                        Ok(data) => latest = Some(data),
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let mut apply_msgs: Vec<String> = Vec::new();
                loop {
                    match apply_rx.try_recv() {
                        Ok(m) => apply_msgs.push(m),
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                if latest.is_none() && apply_msgs.is_empty() {
                    continue;
                }
                if view
                    .update(cx, |v, cx| {
                        if let Some(data) = latest {
                            v.data = data;
                        }
                        if !apply_msgs.is_empty() {
                            v.apply_busy = false;
                            if let Some(m) = apply_msgs.last().cloned() {
                                let error = m.contains("gagal");
                                v.toast = Some(Toast { text: m, at: Instant::now(), error });
                            }
                            v.notice = Some("siap.".to_string());
                        }
                        // Toast auto-hilang setelah 5 detik.
                        if let Some(t) = &v.toast {
                            if t.at.elapsed() > Duration::from_secs(5) {
                                v.toast = None;
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    });
    eprintln!("axioo-control-center: exited (semua window ditutup).");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_scales_rgb() {
        assert_eq!(glow((255, 0, 0), 1.0), 0xFF0000);
        assert_eq!(glow((255, 255, 255), 0.0), 0x000000);
        assert_eq!(glow((0, 255, 0), 0.5), 0x008000);
    }

    #[test]
    fn chart_xy_roundtrip() {
        let (t, d) = chart_xy_to_data(100.0, 80.0, 400.0, 190.0);
        let (x, y) = chart_data_to_xy(t, d, 400.0, 190.0);
        assert!((x - 100.0).abs() < 3.0 && (y - 80.0).abs() < 3.0);
    }

    #[test]
    fn chart_duty_interpolates() {
        let pts = vec![(20, 40), (60, 80)];
        assert_eq!(chart_duty_at(&pts, 20), 40);
        assert_eq!(chart_duty_at(&pts, 40), 60);
        assert_eq!(chart_duty_at(&pts, 100), 80);
    }

    #[test]
    fn rgb_slider_roundtrip() {
        let (ch, v) = rgb_xy_to_val(60.0, 20.0, 300.0, 108.0);
        assert_eq!(ch, 0);
        let x = rgb_knob_x(v, 300.0);
        assert!((x - 60.0).abs() < 3.0);
        assert_eq!(rgb_track_color(0, 1.0), 0xFF0000);
        assert_eq!(rgb_track_color(1, 0.5), 0x008000);
        assert_eq!(rgb_track_color(2, 0.0), 0x000000);
    }
}
