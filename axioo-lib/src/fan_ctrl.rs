//! Privileged one-shot EC fan control (root only).
//!
//! SAFETY CONTRACT (see `docs/ec-fan-protocol.md`):
//! - The register map was validated read-only on Pongo Studio X
//!   (`0x07`≈coretemp package, `0xD0–0xD3`≈hwmon RPM, 5+ samples).
//! - One-shot writes only: manual duty or restore-EC-auto. Duty is
//!   clamped to 40–100% (fans stall below ~40%).
//! - Manual writes go to BOTH fans (`0x01` + `0x02`). Upstream
//!   `clevo-indicator` wrote `0x01` only, leaving the GPU fan dead.
//! - Every entry point refuses when `euid != 0`. The desktop GUI
//!   (running as root) calls these directly in a background thread,
//!   never via `pkexec`; the CLI goes through the same functions.
//! - The continuous curve-following loop belongs to the future `axiood`
//!   root daemon, NOT here and NOT in the GUI.
//!
//! Write path: x86 port I/O on `0x62/0x66` (`ioperm` needs root).
//! Verified after write by reading the `0xCE` duty mirror via `ec_sys`
//! when available (skipped with a warning when `ec_sys` is absent).

use std::fmt;
use std::time::{Duration, Instant};

use crate::fan::{self, EC_CMD_FAN_DUTY, EC_DATA, EC_FAN_INDEX_AUTO, EC_FAN_INDEX_CPU, EC_FAN_INDEX_GPU, EC_REG_FAN1_DUTY, EC_SC};

/// How long to wait for IBF=0 before giving up on one EC transaction.
const IBF_TIMEOUT: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub enum FanCtrlError {
    /// Refuses to touch the EC without root (use pkexec/sudo).
    NotRoot,
    /// Non-x86 platform: port I/O unavailable.
    UnsupportedArch,
    /// `ioperm` failed (needs CAP_SYS_RAWIO, i.e. root).
    IoPerm(std::io::Error),
    /// EC status IBF bit never cleared within [`IBF_TIMEOUT`].
    IoTimeout { step: &'static str },
    /// Write went out but the `0xCE` mirror doesn't match.
    VerifyMismatch { want_pct: u8, got_pct: u8 },
}

impl fmt::Display for FanCtrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FanCtrlError::NotRoot => write!(
                f,
                "needs root: re-run via pkexec or sudo (e.g. pkexec axioo-ctl fan set 70)"
            ),
            FanCtrlError::UnsupportedArch => {
                write!(f, "EC port I/O only available on x86/x86_64")
            }
            FanCtrlError::IoPerm(e) => write!(f, "ioperm(0x62/0x66) failed: {e}"),
            FanCtrlError::IoTimeout { step } => {
                write!(f, "EC timeout waiting IBF=0 during {step} (EC busy or unsupported)")
            }
            FanCtrlError::VerifyMismatch { want_pct, got_pct } => write!(
                f,
                "EC duty mirror mismatch: want ~{want_pct}%, 0xCE reads {got_pct}%"
            ),
        }
    }
}

impl std::error::Error for FanCtrlError {}

pub fn is_root() -> bool {
    // SAFETY: trivial libc getter, no invariants.
    unsafe { libc::geteuid() == 0 }
}

pub fn require_root() -> Result<(), FanCtrlError> {
    if is_root() { Ok(()) } else { Err(FanCtrlError::NotRoot) }
}

/// Pure: `(cmd, port, value)` triples for a manual duty on both fans.
/// Duty clamped to the safe range; raw byte via [`fan::duty_pct_to_raw`].
pub fn manual_triples(pct: u8) -> [(u8, u8, u8); 2] {
    let raw = fan::duty_pct_to_raw(pct);
    [(EC_CMD_FAN_DUTY, EC_FAN_INDEX_CPU, raw), (EC_CMD_FAN_DUTY, EC_FAN_INDEX_GPU, raw)]
}

/// Pure: triples restoring EC auto control on both fans
/// (`port=0xFF`, `value`=fan index).
pub fn auto_triples() -> [(u8, u8, u8); 2] {
    [(EC_CMD_FAN_DUTY, EC_FAN_INDEX_AUTO, EC_FAN_INDEX_CPU), (EC_CMD_FAN_DUTY, EC_FAN_INDEX_AUTO, EC_FAN_INDEX_GPU)]
}

// ---------- low-level x86 port I/O ----------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod port {
    use super::*;

    pub unsafe fn outb(port: u16, val: u8) {
        // SAFETY: caller holds ioperm on 0x62/0x66; EC ports are the
        // documented Clevo command/data ports.
        unsafe {
            std::arch::asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
        }
    }

    pub unsafe fn inb(port: u16) -> u8 {
        let val: u8;
        // SAFETY: same as outb.
        unsafe {
            std::arch::asm!("in al, dx", in("dx") port, out("al") val, options(nomem, nostack, preserves_flags));
        }
        val
    }

    /// RAII: `ioperm` on for 0x62+0x66, off again on drop.
    pub struct IoPermGuard;

    impl IoPermGuard {
        pub fn acquire() -> Result<Self, FanCtrlError> {
            // SAFETY: enables port I/O for exactly the two EC ports;
            // requires CAP_SYS_RAWIO (root). Released on drop.
            let ok_data = unsafe { libc::ioperm(EC_DATA as _, 1, 1) } == 0;
            let ok_sc = unsafe { libc::ioperm(EC_SC as _, 1, 1) } == 0;
            if !ok_data || !ok_sc {
                return Err(FanCtrlError::IoPerm(std::io::Error::last_os_error()));
            }
            Ok(IoPermGuard)
        }
    }

    impl Drop for IoPermGuard {
        fn drop(&mut self) {
            // SAFETY: revokes what acquire() granted; errors ignored on teardown.
            unsafe {
                libc::ioperm(EC_DATA as _, 1, 0);
                libc::ioperm(EC_SC as _, 1, 0);
            }
        }
    }

    fn wait_ibf_clear(step: &'static str) -> Result<(), FanCtrlError> {
        let t0 = Instant::now();
        loop {
            // SAFETY: status-port read under acquired ioperm.
            let status = unsafe { inb(EC_SC) };
            if status & 0x02 == 0 {
                return Ok(());
            }
            if t0.elapsed() > IBF_TIMEOUT {
                return Err(FanCtrlError::IoTimeout { step });
            }
            std::hint::spin_loop();
        }
    }

    /// `ec_io_do(cmd, port, value)` per docs/ec-fan-protocol.md section B.
    pub fn ec_cmd(cmd: u8, port: u8, value: u8, step: &'static str) -> Result<(), FanCtrlError> {
        wait_ibf_clear(step)?;
        // SAFETY: EC command sequence under acquired ioperm.
        unsafe { outb(EC_SC, cmd) };
        wait_ibf_clear(step)?;
        unsafe { outb(EC_DATA, port) };
        wait_ibf_clear(step)?;
        unsafe { outb(EC_DATA, value) };
        wait_ibf_clear(step)?;
        Ok(())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
mod port {
    use super::*;
    pub struct IoPermGuard;
    impl IoPermGuard {
        pub fn acquire() -> Result<Self, FanCtrlError> {
            Err(FanCtrlError::UnsupportedArch)
        }
    }
    pub fn ec_cmd(_: u8, _: u8, _: u8, _: &'static str) -> Result<(), FanCtrlError> {
        Err(FanCtrlError::UnsupportedArch)
    }
}

/// Report of a one-shot write + optional read-back verification.
#[derive(Debug, Clone)]
pub struct ApplyReport {
    /// Duty actually requested after clamping (manual) or `None` (auto).
    pub duty_pct: Option<u8>,
    /// `0xCE` mirror read-back when `ec_sys` was available.
    pub verified_pct: Option<u8>,
    pub verify_skipped: bool,
}

fn verify_mirror(want_pct: u8) -> Result<ApplyReport, FanCtrlError> {
    match crate::ec::read_map() {
        Ok(map) => {
            let got = fan::duty_raw_to_pct(map[EC_REG_FAN1_DUTY as usize]);
            if (got as i16 - want_pct as i16).abs() <= 2 {
                Ok(ApplyReport { duty_pct: Some(want_pct), verified_pct: Some(got), verify_skipped: false })
            } else {
                Err(FanCtrlError::VerifyMismatch { want_pct, got_pct: got })
            }
        }
        // ec_sys absent/unreadable: write still happened, just unverified.
        Err(_) => Ok(ApplyReport { duty_pct: Some(want_pct), verified_pct: None, verify_skipped: true }),
    }
}

/// One-shot manual duty on both fans (root only). Returns read-back report.
pub fn set_manual_duty(pct: u8) -> Result<ApplyReport, FanCtrlError> {
    require_root()?;
    let want = pct.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    let _guard = port::IoPermGuard::acquire()?;
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        for (i, (cmd, p, v)) in manual_triples(want).iter().enumerate() {
            let step = if i == 0 { "fan-cpu duty" } else { "fan-gpu duty" };
            port::ec_cmd(*cmd, *p, *v, step)?;
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        return Err(FanCtrlError::UnsupportedArch);
    }
    verify_mirror(want)
}

/// Restore EC auto control on both fans (root only).
pub fn set_auto() -> Result<(), FanCtrlError> {
    require_root()?;
    let _guard = port::IoPermGuard::acquire()?;
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        for (i, (cmd, p, v)) in auto_triples().iter().enumerate() {
            let step = if i == 0 { "fan-cpu auto" } else { "fan-gpu auto" };
            port::ec_cmd(*cmd, *p, *v, step)?;
        }
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    {
        return Err(FanCtrlError::UnsupportedArch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_triples_clamp_and_cover_both_fans() {
        // Below stall point clamps to 40.
        let lo = manual_triples(0);
        assert_eq!(lo, manual_triples(40));
        assert_eq!(lo[0].1, EC_FAN_INDEX_CPU);
        assert_eq!(lo[1].1, EC_FAN_INDEX_GPU);
        assert_eq!(lo[0].0, EC_CMD_FAN_DUTY);
        // Above max clamps to 100.
        assert_eq!(manual_triples(200)[0].2, fan::duty_pct_to_raw(100));
        // 70% encodes to the documented raw byte.
        assert_eq!(manual_triples(70)[0].2, fan::duty_pct_to_raw(70));
    }

    #[test]
    fn auto_triples_use_ff_selector() {
        let t = auto_triples();
        assert_eq!(t[0], (EC_CMD_FAN_DUTY, EC_FAN_INDEX_AUTO, EC_FAN_INDEX_CPU));
        assert_eq!(t[1], (EC_CMD_FAN_DUTY, EC_FAN_INDEX_AUTO, EC_FAN_INDEX_GPU));
    }

    #[test]
    fn refuses_without_root() {
        if !is_root() {
            assert!(matches!(set_manual_duty(70), Err(FanCtrlError::NotRoot)));
            assert!(matches!(set_auto(), Err(FanCtrlError::NotRoot)));
        }
    }
}
