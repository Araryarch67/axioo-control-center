//! `com.axioo.Control` D-Bus service (thin layer over shared state).

use std::sync::Arc;
use tokio::sync::RwLock;
use zbus::interface;

use crate::profile::AxiooProfile;
use crate::state::DaemonState;

pub const AXIOO_SERVICE: &str = "com.axioo.Control";
pub const AXIOO_PATH: &str = "/com/axioo/Control";

pub struct AxiooControl {
    pub state: Arc<RwLock<DaemonState>>,
}

#[interface(name = "com.axioo.Control")]
impl AxiooControl {
    /// Current axioo profile name, e.g. "Balanced".
    async fn get_profile(&self) -> String {
        self.state.read().await.profile.as_str().to_string()
    }

    /// Set axioo profile; returns the applied name.
    /// Legacy `"Quiet"`/`"power-saver"` map to Balanced + quiet-fan on.
    /// PPD + RAPL apply happens in the main loop via pending flag.
    async fn set_profile(&self, profile: String) -> zbus::fdo::Result<String> {
        let mut st = self.state.write().await;
        if profile.eq_ignore_ascii_case("quiet") || profile.eq_ignore_ascii_case("power-saver") {
            st.profile = AxiooProfile::Balanced;
            st.quiet_fan = true;
        } else {
            st.profile = AxiooProfile::parse(&profile).ok_or_else(|| {
                zbus::fdo::Error::InvalidArgs(format!(
                    "unknown profile '{profile}' (pilih: Balanced/Entertainment/Performance)"
                ))
            })?;
        }
        st.pending_apply = true;
        Ok(st.label())
    }

    /// Current PPD profile as last seen by the daemon.
    async fn get_ppd_profile(&self) -> String {
        self.state.read().await.ppd_profile.clone()
    }

    /// Last fan duty applied by the loop (percent).
    async fn get_fan_duty(&self) -> u8 {
        self.state.read().await.last_duty
    }

    /// Effective fan curve `(temp_c, duty_pct)` incl. quiet toggle.
    /// Single source of truth buat chart GUI — jangan duplikasi tabel
    /// kurva di klien, baca dari sini.
    async fn get_curve(&self) -> Vec<(i32, u8)> {
        let st = self.state.read().await;
        st.profile.effective_curve(st.quiet_fan)
    }

    /// Per-mode quiet-fan toggle state.
    async fn get_quiet_fan(&self) -> bool {
        self.state.read().await.quiet_fan
    }

    /// Set quiet-fan (`true`/`false`); returns the full label.
    async fn set_quiet_fan(&self, quiet: bool) -> String {
        let mut st = self.state.write().await;
        st.quiet_fan = quiet;
        st.pending_apply = true;
        st.label()
    }

    #[zbus(property)]
    async fn profile(&self) -> String {
        self.get_profile().await
    }
}
