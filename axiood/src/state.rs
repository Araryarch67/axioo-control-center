//! Shared daemon state.

use crate::profile::AxiooProfile;

#[derive(Debug)]
pub struct DaemonState {
    pub profile: AxiooProfile,
    /// Per-mode quiet-fan toggle (fan curve only, never touches PPD/RAPL).
    pub quiet_fan: bool,
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
            ppd_profile,
            last_duty: 40,
            pending_apply: true,
        }
    }

    pub fn label(&self) -> String {
        if self.quiet_fan {
            format!("{} (quiet-fan)", self.profile.as_str())
        } else {
            self.profile.as_str().to_string()
        }
    }
}
