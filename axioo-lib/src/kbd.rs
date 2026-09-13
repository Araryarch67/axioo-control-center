//! Keyboard backlight control via the kernel LED class interface.
//!
//! Rust port of kkrdwn/pongo725-backlight (Python/GTK, Axioo Pongo 725):
//! instead of shelling out to sysfs with a hardcoded
//! `/sys/devices/platform/tuxedo_keyboard/leds/rgb:kbd_backlight` path,
//! this module **discovers** the LED node, honors the driver's own
//! `max_brightness`, and respects the `multi_index` channel order.
//!
//! ## Adjustments for Pongo Studio X (2025)
//! * Path discovery instead of a hardcoded platform path (the platform
//!   device name is firmware-dependent).
//! * Brightness is clamped to the driver's `max_brightness` (the Python
//!   tool assumes 0-255; `tuxedo_keyboard` may expose e.g. 0-10).
//! * `multi_intensity` is written in `multi_index` channel order instead
//!   of assuming red-green-blue.
//! * Writes need the `tuxedo_keyboard`/`clevo` driver loaded
//!   (`clevo-drivers-dkms-git` + `modprobe`) AND write permission
//!   (root). Failures explain exactly which one is missing. The 2025
//!   firmware may report an unrecognized backlight type, in which case
//!   the driver registers no LED at all — see `docs/kbd-backlight.md`.
//!
//! Safety: only writes to LED class `brightness`/`multi_intensity`
//! attributes. No EC poking here.

use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::{parse_f64, read_trim};

/// A discovered keyboard-backlight LED class device.
#[derive(Debug, Clone)]
pub struct KbdBacklight {
    /// LED class name, e.g. `rgb:kbd_backlight`.
    pub name: String,
    /// Sysfs dir, e.g. `/sys/class/leds/rgb:kbd_backlight`.
    pub dir: PathBuf,
    /// Driver-advertised brightness scale (do NOT assume 255).
    pub max_brightness: u32,
    /// Channel order for `multi_intensity`, e.g. ["red","green","blue"].
    pub channels: Vec<String>,
}

/// Current backlight state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbdState {
    pub brightness: u32,
    pub rgb: (u8, u8, u8),
}

/// Discover keyboard-backlight LED devices under `/sys/class/leds`.
/// Matches names containing `kbd`, `keyboard` or `backlight`.
pub fn discover() -> Vec<KbdBacklight> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/leds") else {
        return out;
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let l = n.to_lowercase();
            l.contains("kbd") || l.contains("keyboard") || l.contains("backlight")
        })
        .collect();
    names.sort();
    for name in names {
        let dir = PathBuf::from(format!("/sys/class/leds/{name}"));
        let max = read_trim(&dir.join("max_brightness"))
            .and_then(|v| parse_f64(&v))
            .map(|v| v as u32)
            .unwrap_or(255);
        let channels = read_trim(&dir.join("multi_index"))
            .map(|s| s.split_whitespace().map(str::to_lowercase).collect())
            .unwrap_or_else(|| {
                vec!["red".to_string(), "green".to_string(), "blue".to_string()]
            });
        out.push(KbdBacklight { name, dir, max_brightness: max, channels });
    }
    out
}

/// Read current brightness + color. `None` when unreadable.
pub fn read_state(kb: &KbdBacklight) -> Option<KbdState> {
    let brightness = read_trim(&kb.dir.join("brightness"))
        .and_then(|v| parse_f64(&v))
        .map(|v| v as u32)?;
    let rgb = read_trim(&kb.dir.join("multi_intensity"))
        .and_then(|s| parse_intensity_triplet(&s))
        .unwrap_or((255, 255, 255));
    Some(KbdState { brightness, rgb })
}

/// Failure modes for [`set`], with actionable explanations.
#[derive(Debug)]
pub enum SetError {
    /// The LED node does not exist: driver missing or firmware type unknown.
    NoDevice { detail: String },
    /// Write failed (usually permission).
    Io(io::Error),
}

impl fmt::Display for SetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::NoDevice { detail } => write!(
                f,
                "no keyboard-backlight LED found ({detail}). On this machine that means the \
                 clevo/tuxedo kernel driver is not loaded or the firmware reports an \
                 unrecognized backlight type. Try: yay -S clevo-drivers-dkms-git && \
                 sudo modprobe clevo_acpi clevo_wmi tuxedo_keyboard — see docs/kbd-backlight.md"
            ),
            SetError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => write!(
                f,
                "permission denied writing LED sysfs (need root). Re-run with sudo; \
                 the future axiood daemon will own these writes"
            ),
            SetError::Io(e) => write!(f, "sysfs write failed: {e}"),
        }
    }
}

/// Set brightness (clamped to the driver's max) + RGB color.
/// Channel values are ordered per the device's `multi_index`.
pub fn set(kb: &KbdBacklight, brightness: u32, rgb: (u8, u8, u8)) -> Result<(), SetError> {
    if !kb.dir.exists() {
        return Err(SetError::NoDevice { detail: format!("{} absent", kb.dir.display()) });
    }
    let b = brightness.min(kb.max_brightness);
    fs::write(kb.dir.join("brightness"), b.to_string()).map_err(SetError::Io)?;
    fs::write(kb.dir.join("multi_intensity"), map_channels(&kb.channels, rgb))
        .map_err(SetError::Io)?;
    Ok(())
}

/// Parse a "R G B" sysfs triplet (values clamped to u8).
pub fn parse_intensity_triplet(s: &str) -> Option<(u8, u8, u8)> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    let nums: Option<Vec<u32>> = parts.iter().map(|p| p.parse().ok()).collect();
    let nums = nums?;
    Some((
        nums[0].min(255) as u8,
        nums[1].min(255) as u8,
        nums[2].min(255) as u8,
    ))
}

/// Parse a CLI "R,G,B" triplet.
pub fn parse_rgb_arg(s: &str) -> Option<(u8, u8, u8)> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let r: u8 = parts[0].trim().parse().ok()?;
    let g: u8 = parts[1].trim().parse().ok()?;
    let b: u8 = parts[2].trim().parse().ok()?;
    Some((r, g, b))
}

/// Order RGB values per the device's channel list.
/// Unknown channel names get 0 (safe: off rather than wrong color).
pub fn map_channels(channels: &[String], rgb: (u8, u8, u8)) -> String {
    channels
        .iter()
        .map(|c| match c.as_str() {
            s if s.contains("red") => rgb.0.to_string(),
            s if s.contains("green") => rgb.1.to_string(),
            s if s.contains("blue") => rgb.2.to_string(),
            _ => "0".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Named presets, same set as the Python reference tool.
pub fn preset(name: &str) -> Option<(u8, u8, u8)> {
    match name.to_lowercase().as_str() {
        "red" => Some((255, 0, 0)),
        "yellow" => Some((255, 255, 0)),
        "green" => Some((0, 255, 0)),
        "cyan" => Some((0, 255, 255)),
        "blue" => Some((0, 0, 255)),
        "white" => Some((255, 255, 255)),
        "off" => Some((0, 0, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sysfs_triplet() {
        assert_eq!(parse_intensity_triplet("255 0 128"), Some((255, 0, 128)));
        assert_eq!(parse_intensity_triplet("300 0 0"), Some((255, 0, 0)));
        assert_eq!(parse_intensity_triplet("255 0"), None);
        assert_eq!(parse_intensity_triplet("a b c"), None);
    }

    #[test]
    fn parses_cli_rgb() {
        assert_eq!(parse_rgb_arg("255,0,0"), Some((255, 0, 0)));
        assert_eq!(parse_rgb_arg(" 0, 128, 255 "), Some((0, 128, 255)));
        assert_eq!(parse_rgb_arg("256,0,0"), None);
        assert_eq!(parse_rgb_arg("red"), None);
    }

    #[test]
    fn channels_follow_device_order() {
        let ch = vec!["red".into(), "green".into(), "blue".into()];
        assert_eq!(map_channels(&ch, (1, 2, 3)), "1 2 3");
        let swapped = vec!["blue".into(), "green".into(), "red".into()];
        assert_eq!(map_channels(&swapped, (1, 2, 3)), "3 2 1");
        let odd = vec!["red".into(), "mystery".into()];
        assert_eq!(map_channels(&odd, (9, 9, 9)), "9 0");
    }

    #[test]
    fn presets_match_reference_tool() {
        assert_eq!(preset("red"), Some((255, 0, 0)));
        assert_eq!(preset("WHITE"), Some((255, 255, 255)));
        assert_eq!(preset("off"), Some((0, 0, 0)));
        assert_eq!(preset("purple"), None);
    }
}
