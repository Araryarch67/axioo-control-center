//! Axioo profiles: 3 power modes + per-mode `quiet-fan` toggle.
//!
//! Two-way sync dengan PPD itu 1-ke-1:
//! - Balanced      <-> PPD `power-saver`  (RAPL 44/120W)
//! - Entertainment <-> PPD `balanced`     (RAPL 44/160W, kurva medium)
//! - Performance   <-> PPD `performance`  (RAPL 44/160W, kurva agresif)
//!
//! `quiet_fan` murni opsi kipas: tiap mode punya varian kurva quiet yang
//! turun satu tingkat (Performance+quiet = kurva Entertainment normal,
//! Entertainment+quiet = kurva Balanced normal, Balanced+quiet = kurva
//! quiet khusus). Tidak menyentuh PPD/RAPL.

use axioo_lib::fan::REFERENCE_CURVE;

/// The three axioo power modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AxiooProfile {
    Balanced,
    Entertainment,
    Performance,
}

impl AxiooProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Entertainment => "Entertainment",
            Self::Performance => "Performance",
        }
    }

    /// Parse a profile name. Legacy `"quiet"` return `None`
    /// (callers map it to Balanced + `quiet_fan=true`).
    /// `"power-saver"`/`"powersaver"` parse sebagai Balanced
    /// (PPD `power-saver` = padanan 1-ke-1 mode Balanced).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "balanced" | "power-saver" | "powersaver" => Some(Self::Balanced),
            "entertainment" | "ent" => Some(Self::Entertainment),
            "performance" | "perf" => Some(Self::Performance),
            _ => None,
        }
    }

    /// PPD profile to set when this axioo profile is selected (two-way sync).
    pub fn ppd_profile(self) -> &'static str {
        match self {
            Self::Balanced => "power-saver",
            Self::Entertainment => "balanced",
            Self::Performance => "performance",
        }
    }

    /// Map an incoming PPD change to `(axioo profile, quiet_fan)`.
    /// Pemetaan 1-ke-1; `quiet` dipertahankan apa adanya (quiet-fan
    /// murni opsi kipas, tak tersentuh PPD).
    pub fn from_ppd(ppd: &str, current: Self, quiet: bool) -> (Self, bool) {
        match ppd {
            "power-saver" => (Self::Balanced, quiet),
            "balanced" => (Self::Entertainment, quiet),
            "performance" => (Self::Performance, quiet),
            _ => (current, quiet),
        }
    }

    /// (PL1 long_term watts, PL2 short_term watts) to write via RAPL sysfs.
    /// Stock di Studio X saat PPD=performance: 44W / 160W.
    pub fn rapl_limits_w(self) -> (f64, f64) {
        match self {
            Self::Balanced => (44.0, 120.0),
            // dGPU-friendly: stock caps, kurva medium.
            Self::Entertainment => (44.0, 160.0),
            // CPU digenjot via fan agresif; power caps tetap stock (aman).
            Self::Performance => (44.0, 160.0),
        }
    }

    /// Normal fan curve: SAMA untuk semua profil — meniru kurva auto EC
    /// firmware (`REFERENCE_CURVE`, ambang auto_duty_step). Profil hanya
    /// beda di RAPL/PPD, bukan kipas (kurva agresif custom terbukti
    /// terlalu berisik). Pakai [`AxiooProfile::step_duty`] di loop
    /// (dengan hysteresis), tabel ini hanya buat tampilan.
    pub fn curve_points(self) -> Vec<(i32, u8)> {
        REFERENCE_CURVE.to_vec()
    }

    /// Quiet = kunci maks 40% (duty min aman, kipas paling pelan).
    /// Ditampilkan sebagai garis datar; loop memakai [`AxiooProfile::step_duty`].
    pub fn quiet_curve_points(self) -> Vec<(i32, u8)> {
        vec![
            (20, axioo_lib::fan::MIN_FAN_DUTY_PCT),
            (100, axioo_lib::fan::MIN_FAN_DUTY_PCT),
        ]
    }

    /// Effective curve honoring the per-mode quiet toggle.
    pub fn effective_curve(self, quiet_fan: bool) -> Vec<(i32, u8)> {
        if quiet_fan {
            self.quiet_curve_points()
        } else {
            self.curve_points()
        }
    }

    /// Satu langkah duty buat loop daemon.
    /// - quiet → pin 40% apa pun suhunya (sunyi; CPU akan throttle
    ///   saat load berat — proteksi termal firmware tetap jalan).
    /// - normal → [`axioo_lib::fan::auto_duty_step`] (hysteresis ala EC auto).
    pub fn step_duty(self, quiet_fan: bool, temp_c: i32, current_duty: u8) -> u8 {
        if quiet_fan {
            return axioo_lib::fan::MIN_FAN_DUTY_PCT;
        }
        let _ = self;
        axioo_lib::fan::auto_duty_step(temp_c, current_duty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppd_roundtrip() {
        assert_eq!(AxiooProfile::Balanced.ppd_profile(), "power-saver");
        assert_eq!(AxiooProfile::Entertainment.ppd_profile(), "balanced");
        assert_eq!(AxiooProfile::Performance.ppd_profile(), "performance");
        // Incoming PPD 1-ke-1; quiet flag dipertahankan.
        assert_eq!(
            AxiooProfile::from_ppd("performance", AxiooProfile::Entertainment, true),
            (AxiooProfile::Performance, true)
        );
        assert_eq!(
            AxiooProfile::from_ppd("performance", AxiooProfile::Balanced, false),
            (AxiooProfile::Performance, false)
        );
        assert_eq!(
            AxiooProfile::from_ppd("power-saver", AxiooProfile::Performance, false),
            (AxiooProfile::Balanced, false)
        );
        assert_eq!(
            AxiooProfile::from_ppd("power-saver", AxiooProfile::Performance, true),
            (AxiooProfile::Balanced, true)
        );
        assert_eq!(
            AxiooProfile::from_ppd("balanced", AxiooProfile::Performance, true),
            (AxiooProfile::Entertainment, true)
        );
        assert_eq!(
            AxiooProfile::from_ppd("balanced", AxiooProfile::Balanced, false),
            (AxiooProfile::Entertainment, false)
        );
    }

    #[test]
    fn quiet_pins_and_normal_follows_ec_auto() {
        use axioo_lib::fan::{auto_duty_step, curve_duty, MIN_FAN_DUTY_PCT};
        for p in [
            AxiooProfile::Balanced,
            AxiooProfile::Entertainment,
            AxiooProfile::Performance,
        ] {
            // Quiet pin 40 di semua suhu.
            for t in [20, 50, 70, 95] {
                assert_eq!(p.step_duty(true, t, 90), MIN_FAN_DUTY_PCT);
                assert_eq!(curve_duty(&p.effective_curve(true), t), MIN_FAN_DUTY_PCT);
            }
            // Normal = EC auto step di semua profil.
            for (t, d) in [(25, 40), (65, 40), (65, 80), (85, 40)] {
                assert_eq!(p.step_duty(false, t, d), auto_duty_step(t, d));
            }
            // Kurva tampil = referensi (bukan agresif).
            assert_eq!(p.effective_curve(false), REFERENCE_CURVE.to_vec());
        }
        // Legacy names no longer parse as profiles.
        assert_eq!(AxiooProfile::parse("quiet"), None);
        assert_eq!(
            AxiooProfile::parse("power-saver"),
            Some(AxiooProfile::Balanced)
        );
    }
}
