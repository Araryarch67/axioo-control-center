//! axioo-lib: read-only hardware introspection for Axioo laptops.
//!
//! MVP rule: this crate NEVER writes to hardware. Every function only reads
//! sysfs / procfs or spawns read-only helpers (e.g. `nvidia-smi -q` style
//! queries). EC writes, WMI method calls and sysfs stores belong to a future
//! `axioo-lib::control` module gated behind explicit review.

pub mod battery;
pub mod cpu;
pub mod dmi;
pub mod ec;
pub mod fan;
pub mod hwmon;
pub mod kbd;
pub mod leds;
pub mod nvidia;
pub mod platform;
pub mod rapl;

use std::fs;
use std::path::Path;

pub(crate) fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub(crate) fn read_trim_str(path: &str) -> Option<String> {
    read_trim(Path::new(path))
}

pub(crate) fn parse_f64(s: &str) -> Option<f64> {
    s.trim().parse().ok()
}

pub(crate) fn parse_u64(s: &str) -> Option<u64> {
    s.trim().parse().ok()
}
