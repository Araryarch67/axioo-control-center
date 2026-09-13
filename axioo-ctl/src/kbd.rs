//! `axioo-ctl kbd`: keyboard backlight status / get / set.
//!
//! Rust port of kkrdwn/pongo725-backlight's core logic (presets +
//! brightness + RGB over the kernel LED interface), with discovery and
//! driver-scale handling instead of a hardcoded sysfs path.

use axioo_lib::kbd;

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
    for d in &devs {
        // Keep current brightness when the flag is omitted (like the
        // reference tool keeps the slider value).
        let b = match brightness {
            Some(v) => v,
            None => kbd::read_state(d).map(|s| s.brightness).unwrap_or(d.max_brightness),
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
            Ok(()) => println!(
                "{} <- brightness={} rgb={},{},{}",
                d.name,
                b.min(d.max_brightness),
                rgb.0,
                rgb.1,
                rgb.2
            ),
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(3);
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
    for d in &devs {
        let cur = kbd::read_state(d).map(|s| s.brightness).unwrap_or(0);
        let rgb = kbd::read_state(d).map(|s| s.rgb).unwrap_or((255, 255, 255));
        let step = (d.max_brightness / 10).max(1) as i32;
        let next = (cur as i32 + dir * step).clamp(0, d.max_brightness as i32) as u32;
        match kbd::set(d, next, rgb) {
            Ok(()) => println!("{} <- brightness={next}", d.name),
            Err(e) => {
                eprintln!("error: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(3);
    }
}
