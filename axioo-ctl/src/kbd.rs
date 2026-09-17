//! `axioo-ctl kbd`: keyboard backlight status / get / set.
//!
//! Rust port of kkrdwn/pongo725-backlight's core logic (presets +
//! brightness + RGB over the kernel LED interface), with discovery and
//! driver-scale handling instead of a hardcoded sysfs path.

use axioo_lib::{fan_ctrl, kbd, kbd_effect::KbdEffect};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// Path state boot-restore milik daemon (lihat `axiood/src/kbd_state.rs`).
const KBD_STATE_PATH: &str = "/var/lib/axiood/kbd.json";

/// `axioo-ctl kbd restore`: terapkan warna terakhir tersimpan ke SEMUA
/// zona (dipakai udev saat LED muncul = titik paling awal di OS, jauh
/// sebelum axiood/SDDM — plus bisa dipanggil manual). Idempoten:
/// dipanggil sekali per node LED oleh udev tidak masalah.
/// Keluar 0 bila diterapkan atau belum ada state (udev tak boleh error);
/// keluar 1 hanya bila state ada tapi LED/driver tak ada.
pub fn restore() {
    let raw = match std::fs::read_to_string(KBD_STATE_PATH) {
        Ok(r) => r,
        Err(_) => {
            println!("kbd restore: no saved state ({KBD_STATE_PATH} missing) — skipping");
            return;
        }
    };
    // Cermin format `axiood/src/kbd_state.rs` (sumber kanonis).
    let v: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("kbd restore: state corrupt ({e}) — skipping");
            return;
        }
    };
    let num = |k: &str| v.get(k).and_then(|x| x.as_u64());
    let (brightness, r, g, b) = match (num("brightness"), num("r"), num("g"), num("b")) {
        (Some(br), Some(r), Some(g), Some(b)) => (
            br.min(255) as u32,
            r.min(255) as u8,
            g.min(255) as u8,
            b.min(255) as u8,
        ),
        _ => {
            eprintln!("kbd restore: state incomplete — skipping");
            return;
        }
    };
    let devs = kbd::discover();
    if devs.is_empty() {
        eprintln!("error: no keyboard-backlight LED found (see: axioo-ctl kbd status)");
        std::process::exit(1);
    }
    let mut n = 0;
    for d in &devs {
        if kbd::set(d, brightness.min(d.max_brightness), (r, g, b)).is_ok() {
            n += 1;
        }
    }
    if n == 0 {
        eprintln!("error: all LED writes failed");
        std::process::exit(1);
    }
    println!("kbd restore: brightness={brightness} rgb={r},{g},{b} ({n} zones)");
}

/// Simpan warna terakhir agar ke-restore sebelum SDDM saat boot.
/// Best-effort: via D-Bus daemon bila jalan (bisa sebagai user), langsung
/// ke file bila root + daemon mati. Gagal persist = peringatan saja,
/// tulis sysfs yang sudah sukses tidak dibatalkan.
fn persist_boot(brightness: u32, rgb: (u8, u8, u8)) {
    let effect = "static";
    let rear = "follow";
    let speed = 1.0f32;
    // 1. Via daemon (root yang tulis file).
    if let Ok(conn) = zbus::blocking::Connection::system() {
        if let Ok(proxy) = AxiooKbdProxyBlocking::new(&conn) {
            if proxy
                .set_kbd(brightness, rgb.0, rgb.1, rgb.2, effect, rear, speed)
                .is_ok()
            {
                return;
            }
        }
    }
    // 2. Langsung (hanya bisa bila root).
    if !fan_ctrl::is_root() {
        return;
    }
    if let Some(dir) = std::path::Path::new(KBD_STATE_PATH).parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let json = format!(
        "{{\"brightness\":{brightness},\"r\":{},\"g\":{},\"b\":{},\"effect\":\"{effect}\",\"rear\":\"{rear}\",\"speed\":{speed}}}",
        rgb.0, rgb.1, rgb.2,
    );
    if std::fs::write(KBD_STATE_PATH, json).is_err() {
        eprintln!("warning: failed to persist keyboard state for boot restore");
    }
}

#[zbus::proxy(
    interface = "com.axioo.Control",
    default_service = "com.axioo.Control",
    default_path = "/com/axioo/Control"
)]
trait AxiooKbd {
    #[allow(non_snake_case)]
    #[allow(clippy::too_many_arguments)]
    fn set_kbd(
        &self,
        brightness: u32,
        r: u8,
        g: u8,
        b: u8,
        effect: &str,
        rear: &str,
        speed: f32,
    ) -> zbus::Result<String>;
}

pub fn status() {
    let devs = kbd::discover();
    if devs.is_empty() {
        println!("keyboard backlight: NOT FOUND");
        println!("  no *kbd* LED node under /sys/class/leds.");
        println!("  likely causes: clevo/tuxedo driver not loaded, or the 2025");
        println!("  firmware reports an unrecognized backlight type.");
        println!("  next: yay -S clevo-drivers-dkms-git && sudo modprobe clevo_acpi clevo_wmi tuxedo_keyboard");
        println!("  then re-run: axioo-ctl kbd status  (details: docs/kbd-backlight.md)");
        return;
    }
    for d in &devs {
        println!("keyboard backlight: {}", d.name);
        println!("  sysfs:          {}", d.dir.display());
        println!("  max_brightness: {}", d.max_brightness);
        println!("  channels:       {}", d.channels.join(" "));
        match kbd::read_state(d) {
            Some(s) => println!(
                "  current:        brightness={} rgb=({},{},{})",
                s.brightness, s.rgb.0, s.rgb.1, s.rgb.2
            ),
            None => println!("  current:        (unreadable)"),
        }
    }
}

pub fn get() {
    let devs = kbd::discover();
    if devs.is_empty() {
        eprintln!("error: no keyboard-backlight LED found (see: axioo-ctl kbd status)");
        std::process::exit(1);
    }
    for d in &devs {
        match kbd::read_state(d) {
            Some(s) => println!(
                "{} brightness={} rgb={},{},{}",
                d.name, s.brightness, s.rgb.0, s.rgb.1, s.rgb.2
            ),
            None => println!("{} (unreadable)", d.name),
        }
    }
}

pub fn set(brightness: Option<u32>, rgb: Option<String>, preset: Option<String>, dry_run: bool) {
    let rgb = match (rgb, preset) {
        (Some(s), _) => match kbd::parse_rgb_arg(&s) {
            Some(v) => v,
            None => {
                eprintln!("error: bad --rgb value (want R,G,B with 0-255 each, e.g. 255,0,0)");
                std::process::exit(2);
            }
        },
        (None, Some(p)) => match kbd::preset(&p) {
            Some(v) => v,
            None => {
                eprintln!("error: unknown preset '{p}' (red|yellow|green|cyan|blue|white|off)");
                std::process::exit(2);
            }
        },
        (None, None) => (255, 255, 255),
    };

    let devs = kbd::discover();
    if devs.is_empty() {
        eprintln!("error: no keyboard-backlight LED found (see: axioo-ctl kbd status)");
        std::process::exit(1);
    }
    let mut failed = false;
    let mut saved: Option<(u32, (u8, u8, u8))> = None;
    for d in &devs {
        // Keep current brightness when the flag is omitted (like the
        // reference tool keeps the slider value).
        let b = match brightness {
            Some(v) => v,
            None => kbd::read_state(d)
                .map(|s| s.brightness)
                .unwrap_or(d.max_brightness),
        };
        if dry_run {
            println!(
                "[dry-run] {} <- brightness={} (max {}) multi_intensity=\"{}\"",
                d.name,
                b.min(d.max_brightness),
                d.max_brightness,
                kbd::map_channels(&d.channels, rgb),
            );
            continue;
        }
        match kbd::set(d, b, rgb) {
            Ok(()) => {
                println!(
                    "{} <- brightness={} rgb={},{},{}",
                    d.name,
                    b.min(d.max_brightness),
                    rgb.0,
                    rgb.1,
                    rgb.2
                );
                saved = saved.or(Some((b.min(d.max_brightness), rgb)));
            }
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(3);
    }
    if !dry_run {
        if let Some((b, c)) = saved {
            persist_boot(b, c);
        }
    }
}

/// `axioo-ctl kbd brighter|dimmer`: geser brightness relatif ±1 langkah
/// (langkah = max/10, min 1). Untuk bind Fn-keys di compositor
/// (butuh node LED + root, sama seperti `set`).
pub fn brighter() {
    nudge(1);
}

pub fn dimmer() {
    nudge(-1);
}

fn nudge(dir: i32) {
    let devs = kbd::discover();
    if devs.is_empty() {
        eprintln!("error: no keyboard-backlight LED found (see: axioo-ctl kbd status)");
        std::process::exit(1);
    }
    let mut failed = false;
    let mut saved: Option<(u32, (u8, u8, u8))> = None;
    for d in &devs {
        let cur = kbd::read_state(d).map(|s| s.brightness).unwrap_or(0);
        let rgb = kbd::read_state(d).map(|s| s.rgb).unwrap_or((255, 255, 255));
        let step = (d.max_brightness / 10).max(1) as i32;
        let next = (cur as i32 + dir * step).clamp(0, d.max_brightness as i32) as u32;
        match kbd::set(d, next, rgb) {
            Ok(()) => {
                println!("{} <- brightness={next}", d.name);
                saved = saved.or(Some((next, rgb)));
            }
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(3);
    }
    if let Some((b, c)) = saved {
        persist_boot(b, c);
    }
}

/// `axioo-ctl kbd effect <name>`: animasi userspace (butuh root, Ctrl-C untuk stop).
/// `static` = stop animasi + kembali ke warna diam.
/// `--rear <fx>` = animasi independen untuk lightbar belakang (butuh 5 node);
/// keyboard dibiarkan bila efek utama `static`.
pub fn effect(name: &str, rgb: Option<String>, preset: Option<String>, rear: Option<String>) {
    let rear_fx = match rear {
        None => None,
        Some(r) if r.eq_ignore_ascii_case("follow") || r.is_empty() => None,
        Some(r) => match KbdEffect::parse(&r) {
            Some(e) => Some(e),
            None => {
                eprintln!("error: unknown rear effect '{r}'");
                std::process::exit(2);
            }
        },
    };
    let eff = match KbdEffect::parse(name) {
        Some(e) => e,
        None => {
            eprintln!(
                "error: unknown effect '{name}' (choose: {})",
                KbdEffect::all()
                    .iter()
                    .map(|e| e.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
            );
            std::process::exit(2);
        }
    };
    if eff == KbdEffect::Static && rear_fx.is_none() {
        // stop: kembalikan ke warna base
        let base = match (rgb, preset) {
            (Some(s), _) => kbd::parse_rgb_arg(&s).unwrap_or((255, 255, 255)),
            (None, Some(p)) => kbd::preset(&p).unwrap_or((255, 255, 255)),
            _ => kbd::discover()
                .first()
                .and_then(kbd::read_state)
                .map(|s| s.rgb)
                .unwrap_or((255, 255, 255)),
        };
        let devs = kbd::discover();
        if devs.is_empty() {
            eprintln!("error: no LED found");
            std::process::exit(1);
        }
        let mut saved: Option<(u32, (u8, u8, u8))> = None;
        for d in &devs {
            let b = kbd::read_state(d)
                .map(|s| s.brightness)
                .unwrap_or(d.max_brightness);
            let _ = kbd::set(d, b, base);
            println!("{} <- static rgb={},{},{}", d.name, base.0, base.1, base.2);
            saved = saved.or(Some((b.min(d.max_brightness), base)));
        }
        if let Some((b, c)) = saved {
            persist_boot(b, c);
        }
        return;
    }
    let base = match (rgb, preset) {
        (Some(s), _) => match kbd::parse_rgb_arg(&s) {
            Some(v) => v,
            None => {
                eprintln!("error: bad --rgb R,G,B");
                std::process::exit(2);
            }
        },
        (None, Some(p)) => match kbd::preset(&p) {
            Some(v) => v,
            None => {
                eprintln!("error: unknown preset '{p}'");
                std::process::exit(2);
            }
        },
        _ => (0, 180, 255), // default breathing: cyanish
    };
    let devs = kbd::discover();
    if devs.is_empty() {
        eprintln!("error: no keyboard-backlight LED found");
        std::process::exit(1);
    }
    let brightness = devs
        .first()
        .map(|d| {
            kbd::read_state(d)
                .map(|s| s.brightness)
                .unwrap_or(d.max_brightness)
        })
        .unwrap_or(255);
    println!(
        "effect {} base rgb={},{},{} brightness={} — Ctrl-C to stop",
        eff.as_str(),
        base.0,
        base.1,
        base.2,
        brightness
    );
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    ctrlc_handler(s);
    run_split(eff, rear_fx, base, brightness, 1.0, stop);
}

/// Loop gabungan CLI (sama seperti split loop backend GUI): zona keyboard
/// pakai efek utama, rear (indeks terakhir bila ≥5 node) pakai efek rear.
/// Main `static` + rear independen = keyboard dibiarkan, hanya rear jalan.
fn run_split(
    main: KbdEffect,
    rear: Option<KbdEffect>,
    base: (u8, u8, u8),
    brightness: u32,
    speed: f32,
    stop: Arc<AtomicBool>,
) {
    use axioo_lib::kbd_effect;
    let devs = kbd::discover();
    let n = devs.len();
    let rear_idx = if rear.is_some() && n >= 5 {
        Some(n - 1)
    } else {
        None
    };
    let rear_start: Option<((u8, u8, u8), u32)> = rear_idx
        .and_then(|ri| devs.get(ri))
        .and_then(kbd::read_state)
        .map(|s| (s.rgb, s.brightness));
    let static_main = main == KbdEffect::Static;
    let sp = speed.clamp(0.1, 4.0);
    let mut t = 0.0f32;
    let dt = 0.06;
    while !stop.load(Ordering::Relaxed) {
        if static_main {
            if let (Some(ri), Some(rfx)) = (rear_idx, rear.as_ref()) {
                if let (Some(d), Some(c)) =
                    (devs.get(ri), kbd_effect::tick(rfx, base, t, 1).first())
                {
                    let _ = kbd::set(d, brightness, *c);
                }
            }
        } else {
            let mut cols = kbd_effect::tick(&main, base, t, n);
            if let (Some(ri), Some(rfx)) = (rear_idx, rear.as_ref()) {
                if let Some(c) = kbd_effect::tick(rfx, base, t, 1).first() {
                    cols[ri] = *c;
                }
            }
            for (d, rgb) in devs.iter().zip(cols.iter()) {
                let _ = kbd::set(d, brightness, *rgb);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
        t += dt * sp;
    }
    if static_main {
        if let (Some(ri), Some(((r, g, b), br))) = (rear_idx, rear_start) {
            if let Some(d) = devs.get(ri) {
                let _ = kbd::set(d, br, (r, g, b));
            }
        }
    } else {
        for d in &devs {
            let _ = kbd::set(d, brightness, base);
        }
    }
}

fn ctrlc_handler(stop: Arc<AtomicBool>) {
    let _ = ctrlc::set_handler(move || {
        stop.store(true, Ordering::Relaxed);
    });
}
