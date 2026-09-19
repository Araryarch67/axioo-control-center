//! axioo-lib: hardware introspection for Axioo laptops (+ one gated
//! privileged fan writer).
//!
//! Rule: everything is read-only sysfs/procfs EXCEPT
//! [`fan_ctrl`], which performs one-shot EC duty writes and refuses
//! unless running as root (desktop calls it via `pkexec axioo-ctl`).
//! The continuous fan-curve loop still belongs to the future `axiood`
//! daemon and must never live in CLI/GUI code.

pub mod battery;
pub mod cpu;
pub mod devices;
pub mod dmi;
pub mod ec;
pub mod fan;
pub mod fan_ctrl;
pub mod hwmon;
pub mod kbd;
pub mod kbd_effect;
pub mod leds;
pub mod memory;
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
