//! Keyboard RGB effects — userspace animation over `kbd::set`.
//!
//! Hardware hanya punya 3-4 zona statis; efek animasi via tulis
//! periodik ke sysfs (tetap root, aman `brightness`/`multi_intensity`).
//! "Static" = diam (legacy), lainnya overlay animasi. Loop stop via `AtomicBool`.
//! Music/Spectrum saat ini simulasi beat (sine bervariasi); upgrade ke
//! capture PipeWire/Pulse via `cpal` tinggal ganti sumber `t` → RMS.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;

use crate::kbd;

/// Mode efek — `Static` = diam, lainnya animasi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KbdEffect {
    Static,
    Breathing,
    Wave,
    Rainbow,
    Cycle,
    Aurora,
    Twinkle,
    Pulse,
    Gradient,
    Music,
    Spectrum,
    Reactive,
}

impl KbdEffect {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Breathing => "breathing",
            Self::Wave => "wave",
            Self::Rainbow => "rainbow",
            Self::Cycle => "cycle",
            Self::Aurora => "aurora",
            Self::Twinkle => "twinkle",
            Self::Pulse => "pulse",
            Self::Gradient => "gradient",
            Self::Music => "music",
            Self::Spectrum => "spectrum",
            Self::Reactive => "reactive",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Static => "Static",
            Self::Breathing => "Breathing",
            Self::Wave => "Wave",
            Self::Rainbow => "Rainbow",
            Self::Cycle => "Cycle",
            Self::Aurora => "Aurora",
            Self::Twinkle => "Twinkle",
            Self::Pulse => "Pulse",
            Self::Gradient => "Gradient",
            Self::Music => "Music",
            Self::Spectrum => "Spectrum",
            Self::Reactive => "Reactive",
        }
    }

    pub fn desc(&self) -> &'static str {
        match self {
            Self::Static => "solid color — use presets or custom color",
            Self::Breathing => "breathing — fade in/out selected color",
            Self::Wave => "wave — hue flows left → numpad 40°/s",
            Self::Rainbow => "rainbow — all zones sync 45°/s",
            Self::Cycle => "cycle — 6 presets with blend",
            Self::Aurora => "aurora — slow pastel wave 15°/s",
            Self::Twinkle => "twinkle — random sparkle every 0.4s",
            Self::Pulse => "pulse — double heartbeat",
            Self::Gradient => "gradient — fixed rainbow per zone",
            Self::Music => "music — beat pulsing (simulated, cpal-ready)",
            Self::Spectrum => "spectrum — per-zone low/mid/high (simulated)",
            Self::Reactive => "reactive — white flash (placeholder)",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "static" | "off" | "solid" => Some(Self::Static),
            "breathing" | "breathe" => Some(Self::Breathing),
            "wave" => Some(Self::Wave),
            "rainbow" => Some(Self::Rainbow),
            "cycle" | "spectrum-cycle" => Some(Self::Cycle),
            "aurora" | "boreal" => Some(Self::Aurora),
            "twinkle" | "sparkle" | "random" => Some(Self::Twinkle),
            "pulse" | "heartbeat" => Some(Self::Pulse),
            "gradient" | "grad" => Some(Self::Gradient),
            "music" | "audio" | "beat" => Some(Self::Music),
            "spectrum" => Some(Self::Spectrum),
            "reactive" | "typing" => Some(Self::Reactive),
            _ => None,
        }
    }

    pub fn all() -> [Self; 12] {
        [
            Self::Static,
            Self::Breathing,
            Self::Wave,
            Self::Rainbow,
            Self::Cycle,
            Self::Aurora,
            Self::Twinkle,
            Self::Pulse,
            Self::Gradient,
            Self::Music,
            Self::Spectrum,
            Self::Reactive,
        ]
    }
}

/// HSV (h 0-360, s 0-1, v 0-1) → RGB 0-255.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn lerp(a: u8, b: u8, k: f32) -> u8 {
    (a as f32 * (1.0 - k) + b as f32 * k)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// Satu tick → warna per zona. `t` detik, `base` warna pilihan, `nzones` 4 ideal.
pub fn tick(effect: &KbdEffect, base: (u8, u8, u8), t: f32, nzones: usize) -> Vec<(u8, u8, u8)> {
    let n = nzones.max(1);
    match effect {
        KbdEffect::Static => vec![base; n],
        KbdEffect::Breathing => {
            let f = ((t * 3.8).sin() + 1.0) / 2.0;
            let k = 0.2 + 0.8 * f;
            let rgb = (
                (base.0 as f32 * k).round() as u8,
                (base.1 as f32 * k).round() as u8,
                (base.2 as f32 * k).round() as u8,
            );
            vec![rgb; n]
        }
        KbdEffect::Wave => (0..n)
            .map(|i| {
                let h = (t * 40.0 + i as f32 * 70.0).rem_euclid(360.0);
                hsv_to_rgb(h, 1.0, 1.0)
            })
            .collect(),
        KbdEffect::Rainbow => {
            let h = (t * 45.0).rem_euclid(360.0);
            vec![hsv_to_rgb(h, 1.0, 1.0); n]
        }
        KbdEffect::Cycle => {
            const PRESETS: [(u8, u8, u8); 6] = [
                (255, 0, 0),
                (255, 255, 0),
                (0, 255, 0),
                (0, 255, 255),
                (0, 0, 255),
                (255, 0, 255),
            ];
            let period = 2.0;
            let idx = ((t / period) as usize) % PRESETS.len();
            let frac = (t % period) / period;
            let rgb = if frac < 0.85 {
                PRESETS[idx]
            } else {
                let nxt = PRESETS[(idx + 1) % PRESETS.len()];
                let k = (frac - 0.85) / 0.15;
                (
                    lerp(PRESETS[idx].0, nxt.0, k),
                    lerp(PRESETS[idx].1, nxt.1, k),
                    lerp(PRESETS[idx].2, nxt.2, k),
                )
            };
            vec![rgb; n]
        }
        KbdEffect::Aurora => (0..n)
            .map(|i| {
                let h = (t * 15.0 + i as f32 * 35.0 + (t * 0.7).sin() * 20.0).rem_euclid(360.0);
                hsv_to_rgb(h, 0.65, 1.0)
            })
            .collect(),
        KbdEffect::Twinkle => {
            // pseudo-random via sin hash per zona+time bucket
            (0..n)
                .map(|i| {
                    let bucket = (t * 2.5).floor() as u64;
                    let h = ((bucket
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(i as u64 * 1442695040888963407))
                        % 360) as f32;
                    let on = ((t * 6.0 + i as f32 * 1.3).sin() * 0.5 + 0.5) > 0.72;
                    if on {
                        hsv_to_rgb(h, 0.9, 1.0)
                    } else {
                        (
                            (base.0 as f32 * 0.18).round() as u8,
                            (base.1 as f32 * 0.18).round() as u8,
                            (base.2 as f32 * 0.18).round() as u8,
                        )
                    }
                })
                .collect()
        }
        KbdEffect::Pulse => {
            // heartbeat: dua puncak per periode 1.1s
            let phase = (t * 1.8).rem_euclid(std::f32::consts::TAU);
            let beat =
                (phase.sin().powf(2.0) * 0.6 + (phase * 2.0).sin().powf(8.0) * 0.4).clamp(0.0, 1.0);
            let k = 0.15 + 0.85 * beat;
            let rgb = (
                (base.0 as f32 * k).round() as u8,
                (base.1 as f32 * k).round() as u8,
                (base.2 as f32 * k).round() as u8,
            );
            vec![rgb; n]
        }
        KbdEffect::Gradient => (0..n)
            .map(|i| {
                let h = (i as f32 / n as f32) * 300.0; // 0..300° tetap
                hsv_to_rgb(h, 1.0, 1.0)
            })
            .collect(),
        KbdEffect::Music => {
            // Simulasi RMS beat: gabungan sine 2Hz + 3.7Hz + hash — nanti ganti RMS cpal
            let beat = ((t * 6.0).sin() * 0.5 + (t * 11.3).sin() * 0.3 + (t * 2.1).sin() * 0.2)
                .abs()
                .clamp(0.0, 1.0);
            let k = 0.35 + 0.65 * beat.powf(1.6);
            let h = (t * 28.0 + beat * 60.0).rem_euclid(360.0);
            let rgb_base = hsv_to_rgb(h, 0.9, 1.0);
            let rgb = (
                (rgb_base.0 as f32 * k).round() as u8,
                (rgb_base.1 as f32 * k).round() as u8,
                (rgb_base.2 as f32 * k).round() as u8,
            );
            vec![rgb; n]
        }
        KbdEffect::Spectrum => (0..n)
            .map(|i| {
                // per-zona band low/mid/high simulasi
                let freq = 4.0 + i as f32 * 5.5;
                let band = ((t * freq).sin() * 0.5 + 0.5).powf(1.2);
                let h = (i as f32 * 65.0 + t * 18.0).rem_euclid(360.0);
                let v = 0.4 + 0.6 * band;
                hsv_to_rgb(h, 0.95, v)
            })
            .collect(),
        KbdEffect::Reactive => {
            // placeholder: flash singkat tiap 1.8s (nanti hook evdev key)
            let flash = ((t * 3.5).sin() > 0.92) as u8;
            if flash == 1 {
                vec![(255, 255, 255); n]
            } else {
                vec![base; n]
            }
        }
    }
}

/// Blokir thread sampai `stop` true.
pub fn run_blocking(effect: KbdEffect, base: (u8, u8, u8), brightness: u32, stop: Arc<AtomicBool>) {
    let devs = kbd::discover();
    if devs.is_empty() {
        return;
    }
    let nz = devs.len();
    let mut t = 0.0f32;
    let dt = 0.06;
    while !stop.load(Ordering::Relaxed) {
        let cols = tick(&effect, base, t, nz);
        for (d, rgb) in devs.iter().zip(cols.iter()) {
            let _ = kbd::set(d, brightness, *rgb);
        }
        thread::sleep(Duration::from_millis(60));
        t += dt;
    }
    for d in &devs {
        let _ = kbd::set(d, brightness, base);
    }
}

pub fn spawn(effect: KbdEffect, base: (u8, u8, u8), brightness: u32) -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    thread::spawn(move || run_blocking(effect, base, brightness, s));
    stop
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_roundtrips() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
    }

    #[test]
    fn tick_shapes() {
        for e in KbdEffect::all() {
            assert_eq!(tick(&e, (10, 20, 30), 0.0, 4).len(), 4);
            assert_eq!(tick(&e, (10, 20, 30), 1.3, 4).len(), 4);
        }
        let b0 = tick(&KbdEffect::Breathing, (255, 255, 255), 0.0, 1)[0];
        let b1 = tick(&KbdEffect::Breathing, (255, 255, 255), 0.8, 1)[0];
        assert_ne!(b0, b1);
    }

    #[test]
    fn parse_effects() {
        assert_eq!(KbdEffect::parse("wave"), Some(KbdEffect::Wave));
        assert_eq!(KbdEffect::parse("STATIC"), Some(KbdEffect::Static));
        assert_eq!(KbdEffect::parse("music"), Some(KbdEffect::Music));
        assert_eq!(KbdEffect::parse("spectrum"), Some(KbdEffect::Spectrum));
        assert_eq!(KbdEffect::parse("aurora"), Some(KbdEffect::Aurora));
        assert_eq!(KbdEffect::parse("unknown"), None);
    }
}
