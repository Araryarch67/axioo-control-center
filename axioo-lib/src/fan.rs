//! Clevo EC fan protocol constants + pure conversions.
//!
//! Reference: `docs/ec-fan-protocol.md`
//! (from hajilok/clevo-axioo-dual-fan-linux, cross-checked against
//! TUXEDO's `tuxedo-fan-control`).
//!
//! MVP rule: this module contains NO I/O. Actual EC reads/writes need
//! root (`ioperm` on ports 0x62/0x66 or `/sys/kernel/debug/ec/ec0/io`)
//! and belong to the future privileged daemon (`axiood`), only after
//! the protocol is validated read-only on the target model.

/// EC status/command port.
pub const EC_SC: u16 = 0x66;
/// EC data port.
pub const EC_DATA: u16 = 0x62;
/// EC "read register" command byte.
pub const EC_SC_READ_CMD: u8 = 0x80;

/// EC command: write fan duty.
pub const EC_CMD_FAN_DUTY: u8 = 0x99;

/// Fan selector for [`EC_CMD_FAN_DUTY`].
pub const EC_FAN_INDEX_CPU: u8 = 0x01;
/// Fan selector for [`EC_CMD_FAN_DUTY`].
pub const EC_FAN_INDEX_GPU: u8 = 0x02;
/// `port` value meaning "switch fan back to AUTO" (`value` = fan index).
pub const EC_FAN_INDEX_AUTO: u8 = 0xFF;

pub const EC_REG_CPU_TEMP: u8 = 0x07;
pub const EC_REG_GPU_TEMP: u8 = 0xCD;
pub const EC_REG_FAN1_DUTY: u8 = 0xCE;
pub const EC_REG_FAN1_RPM_HI: u8 = 0xD0;
pub const EC_REG_FAN1_RPM_LO: u8 = 0xD1;
pub const EC_REG_FAN2_RPM_HI: u8 = 0xD2;
pub const EC_REG_FAN2_RPM_LO: u8 = 0xD3;

/// Fans stall below ~40% on most Clevo ECs.
pub const MIN_FAN_DUTY_PCT: u8 = 40;
pub const MAX_FAN_DUTY_PCT: u8 = 100;

/// Duty percent (clamped to 40..=100) -> raw EC byte.
pub fn duty_pct_to_raw(pct: u8) -> u8 {
    let clamped = pct.clamp(MIN_FAN_DUTY_PCT, MAX_FAN_DUTY_PCT);
    (clamped as f64 / 100.0 * 255.0) as u8
}

/// Raw EC duty byte -> percent.
pub fn duty_raw_to_pct(raw: u8) -> u8 {
    (raw as f64 / 255.0 * 100.0) as u8
}

/// Raw hi/lo RPM registers -> RPM (`2156220 / raw16`, 0 when stopped).
pub fn rpm_from_regs(hi: u8, lo: u8) -> u32 {
    let raw = ((hi as u32) << 8) | lo as u32;
    2156220u32.checked_div(raw).unwrap_or(0)
}

/// Decoded fan-relevant EC registers (read-only view, no I/O here).
/// Register layout: `docs/ec-fan-protocol.md` section C.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FanSnapshot {
    /// Raw byte at `EC_REG_CPU_TEMP` (directly °C on validated models).
    pub cpu_temp_raw: u8,
    /// Raw byte at `EC_REG_GPU_TEMP` (0 = GPU idle/Optimus asleep).
    pub gpu_temp_raw: u8,
    /// Raw byte at `EC_REG_FAN1_DUTY` (mirror of last written duty).
    pub fan1_duty_raw: u8,
    /// `fan1_duty_raw` converted to percent.
    pub fan1_duty_pct: u8,
    /// Fan 1 RPM from `EC_REG_FAN1_RPM_HI/LO`.
    pub fan1_rpm: u32,
    /// Fan 2 / GPU RPM from `EC_REG_FAN2_RPM_HI/LO`.
    pub fan2_rpm: u32,
    /// Raw RPM register bytes, kept for cross-checking alternate maps
    /// (e.g. Studio X may carry GPU RPM at `0xD4/0xD5`).
    pub rpm_regs: [u8; 4],
}

impl FanSnapshot {
    /// Highest of CPU/GPU temp as `i32`, for [`auto_duty_step`].
    /// A GPU reading of 0 means "asleep", so it never raises the max.
    pub fn max_temp_c(&self) -> i32 {
        self.cpu_temp_raw.max(self.gpu_temp_raw) as i32
    }
}

/// Decode a 256-byte EC map into the fan registers of interest.
/// Pure function: takes the map as read via `ec::read_map()`.
pub fn snapshot(map: &[u8; 256]) -> FanSnapshot {
    let duty_raw = map[EC_REG_FAN1_DUTY as usize];
    let hi1 = map[EC_REG_FAN1_RPM_HI as usize];
    let lo1 = map[EC_REG_FAN1_RPM_LO as usize];
    let hi2 = map[EC_REG_FAN2_RPM_HI as usize];
    let lo2 = map[EC_REG_FAN2_RPM_LO as usize];
    FanSnapshot {
        cpu_temp_raw: map[EC_REG_CPU_TEMP as usize],
        gpu_temp_raw: map[EC_REG_GPU_TEMP as usize],
        fan1_duty_raw: duty_raw,
        fan1_duty_pct: duty_raw_to_pct(duty_raw),
        fan1_rpm: rpm_from_regs(hi1, lo1),
        fan2_rpm: rpm_from_regs(hi2, lo2),
        rpm_regs: [hi1, lo1, hi2, lo2],
    }
}

/// Reference curve breakpoints mirroring [`auto_duty_step`], as editable
/// `(temp_c, duty_pct)` points for UI/daemon configuration.
pub const REFERENCE_CURVE: [(i32, u8); 7] = [
    (20, 40),
    (30, 50),
    (40, 60),
    (50, 70),
    (60, 80),
    (70, 90),
    (80, 100),
];

/// Stepwise curve lookup over caller-owned `(temp_c, duty_pct)` points.
/// Returns the duty of the last point with threshold <= `temp_c`;
/// below the first point returns the first point's duty; empty curve
/// returns [`MIN_FAN_DUTY_PCT`]. Points are expected sorted by temp
/// (the GUI editor keeps them sorted); duties are clamped to the safe
/// range on the way out.
pub fn curve_duty(points: &[(i32, u8)], temp_c: i32) -> u8 {
    let mut duty = None;
    for &(t, d) in points {
        if temp_c >= t {
            duty = Some(d);
        } else {
            break;
        }
    }
    duty.or_else(|| points.first().map(|&(_, d)| d))
        .unwrap_or(MIN_FAN_DUTY_PCT)
        .clamp(MIN_FAN_DUTY_PCT, MAX_FAN_DUTY_PCT)
}

/// Reference auto-curve step (with hysteresis), mirroring the C tool.
/// `temp_c` = max(CPU, GPU) temp, `duty` = current duty. Returns the new
/// duty, or the current one when inside the hysteresis band.
pub fn auto_duty_step(temp_c: i32, duty: u8) -> u8 {
    if temp_c >= 80 && duty < 100 {
        return 100;
    }
    if temp_c >= 70 && duty < 90 {
        return 90;
    }
    if temp_c >= 60 && duty < 80 {
        return 80;
    }
    if temp_c >= 50 && duty < 70 {
        return 70;
    }
    if temp_c >= 40 && duty < 60 {
        return 60;
    }
    if temp_c >= 30 && duty < 50 {
        return 50;
    }
    if temp_c >= 20 && duty < 40 {
        return 40;
    }
    if temp_c <= 15 && duty > 40 {
        return 40;
    }
    if temp_c <= 25 && duty > 50 {
        return 50;
    }
    if temp_c <= 35 && duty > 60 {
        return 60;
    }
    if temp_c <= 45 && duty > 70 {
        return 70;
    }
    if temp_c <= 55 && duty > 80 {
        return 80;
    }
    if temp_c <= 65 && duty > 90 {
        return 90;
    }
    duty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duty_roundtrip_within_clamp() {
        for pct in [40u8, 50, 70, 100] {
            let raw = duty_pct_to_raw(pct);
            let back = duty_raw_to_pct(raw);
            assert!(
                (back as i16 - pct as i16).abs() <= 1,
                "{pct} -> {raw} -> {back}"
            );
        }
    }

    #[test]
    fn duty_clamps_low_end() {
        assert_eq!(duty_pct_to_raw(0), duty_pct_to_raw(40));
        assert_eq!(duty_pct_to_raw(150), duty_pct_to_raw(100));
    }

    #[test]
    fn rpm_zero_when_stopped() {
        assert_eq!(rpm_from_regs(0, 0), 0);
    }

    #[test]
    fn rpm_sane_range() {
        // ~2400 RPM like the live acpi_fan reading on this machine.
        let raw16 = 2156220 / 2400;
        let rpm = rpm_from_regs((raw16 >> 8) as u8, (raw16 & 0xFF) as u8);
        assert!((rpm as i32 - 2400).abs() < 50, "got {rpm}");
    }

    #[test]
    fn auto_curve_steps_up_and_holds() {
        assert_eq!(auto_duty_step(85, 40), 100);
        assert_eq!(auto_duty_step(62, 40), 80);
        // Inside hysteresis band: hold current duty.
        assert_eq!(auto_duty_step(62, 80), 80);
        assert_eq!(auto_duty_step(50, 80), 80);
        // Cool down: step down gradually.
        assert_eq!(auto_duty_step(50, 90), 80);
    }

    #[test]
    fn snapshot_decodes_regs() {
        let mut map = [0u8; 256];
        map[EC_REG_CPU_TEMP as usize] = 55;
        map[EC_REG_GPU_TEMP as usize] = 0; // asleep
        map[EC_REG_FAN1_DUTY as usize] = duty_pct_to_raw(70);
        let raw16 = 2156220 / 2400;
        map[EC_REG_FAN1_RPM_HI as usize] = (raw16 >> 8) as u8;
        map[EC_REG_FAN1_RPM_LO as usize] = (raw16 & 0xFF) as u8;
        // Fan 2 stopped.
        let snap = snapshot(&map);
        assert_eq!(snap.cpu_temp_raw, 55);
        assert_eq!(snap.gpu_temp_raw, 0);
        assert!(
            (snap.fan1_duty_pct as i16 - 70).abs() <= 1,
            "got {}",
            snap.fan1_duty_pct
        );
        assert!(
            (snap.fan1_rpm as i32 - 2400).abs() < 50,
            "got {}",
            snap.fan1_rpm
        );
        assert_eq!(snap.fan2_rpm, 0);
        assert_eq!(snap.max_temp_c(), 55);
    }

    #[test]
    fn snapshot_max_ignores_sleeping_gpu() {
        let mut map = [0u8; 256];
        map[EC_REG_CPU_TEMP as usize] = 48;
        map[EC_REG_GPU_TEMP as usize] = 0;
        assert_eq!(snapshot(&map).max_temp_c(), 48);
        map[EC_REG_GPU_TEMP as usize] = 62;
        assert_eq!(snapshot(&map).max_temp_c(), 62);
    }

    #[test]
    fn curve_duty_steps_at_thresholds() {
        let pts = REFERENCE_CURVE;
        assert_eq!(curve_duty(&pts, 10), 40); // below first -> first duty
        assert_eq!(curve_duty(&pts, 20), 40);
        assert_eq!(curve_duty(&pts, 29), 40);
        assert_eq!(curve_duty(&pts, 30), 50);
        assert_eq!(curve_duty(&pts, 65), 80);
        assert_eq!(curve_duty(&pts, 80), 100);
        assert_eq!(curve_duty(&pts, 120), 100); // above last -> hold
    }

    #[test]
    fn curve_duty_empty_and_clamped() {
        assert_eq!(curve_duty(&[], 70), MIN_FAN_DUTY_PCT);
        assert_eq!(curve_duty(&[(50, 5)], 60), MIN_FAN_DUTY_PCT);
        assert_eq!(curve_duty(&[(50, 200)], 60), MAX_FAN_DUTY_PCT);
    }
}
