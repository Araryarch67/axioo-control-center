//! Shared daemon state.

use crate::profile::AxiooProfile;

#[derive(Debug)]
pub struct DaemonState {
    pub profile: AxiooProfile,
    /// Per-mode quiet-fan toggle (fan curve only, never touches PPD/RAPL).
    pub quiet_fan: bool,
    /// Bila `true`, loop TIDAK menulis duty — kipas dikembalikan ke
    /// firmware EC auto (`fan_ctrl::set_auto()` satu-kali saat masuk mode).
    /// RAPL/PPD tetap mengikuti profil. Default `true` (EC auto; daemon
    /// hanya atur RAPL/PPD sampai user pilih Curve/Manual di GUI).
    /// Mengalahkan [`DaemonState::fan_manual`].
    pub fan_ec_auto: bool,
    /// Override manual: `Some(duty)` = kunci duty ini via loop daemon
    /// (validasi + clamp di D-Bus setter). `None` = ikut kurva.
    /// Mengalahkan [`DaemonState::fan_ec_auto`] (setter mematikannya).
    pub fan_manual: Option<u8>,
    pub ppd_profile: String,
    pub last_duty: u8,
    /// Set by D-Bus `SetProfile`/`SetQuietFan` or PPD watcher; consumed by main loop.
    pub pending_apply: bool,
}

impl DaemonState {
    pub fn new(profile: AxiooProfile, quiet_fan: bool, ppd_profile: String) -> Self {
        Self {
            profile,
            quiet_fan,
            fan_ec_auto: true,
            fan_manual: None,
            ppd_profile,
            last_duty: 40,
            pending_apply: true,
        }
    }

    pub fn label(&self) -> String {
        let mut s = if self.quiet_fan {
            format!("{} (quiet-fan)", self.profile.as_str())
        } else {
            self.profile.as_str().to_string()
        };
        if self.fan_ec_auto {
            s.push_str(" + ec-auto");
        } else if let Some(d) = self.fan_manual {
            s.push_str(&format!(" + manual {d}%"));
        }
        s
    }

    /// Mode kipas buat UI: "curve" | "manual" | "ec_auto".
    pub fn fan_mode(&self) -> &'static str {
        if self.fan_ec_auto {
            "ec_auto"
        } else if self.fan_manual.is_some() {
            "manual"
        } else {
            "curve"
        }
    }
}
