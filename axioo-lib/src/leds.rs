//! LED class devices (`/sys/class/leds`), incl. keyboard backlight candidates.

use std::fs;

use crate::{parse_f64, read_trim_str};

#[derive(Debug, Clone)]
pub struct Led {
    pub name: String,
    pub brightness: f64,
    pub max_brightness: f64,
}

pub fn leds() -> Vec<Led> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/leds") else {
        return out;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let base = format!("/sys/class/leds/{name}");
        let brightness = read_trim_str(&format!("{base}/brightness"))
            .and_then(|v| parse_f64(&v))
            .unwrap_or(0.0);
        let max = read_trim_str(&format!("{base}/max_brightness"))
            .and_then(|v| parse_f64(&v))
            .unwrap_or(0.0);
        out.push(Led { name, brightness, max_brightness: max });
    }
    out
}

/// LEDs whose name suggests a keyboard backlight (future control target).
pub fn kbd_candidates() -> Vec<Led> {
    leds()
        .into_iter()
        .filter(|l| {
            let n = l.name.to_lowercase();
            n.contains("kbd") || n.contains("keyboard") || n.contains("backlight")
        })
        .collect()
}
