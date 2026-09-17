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
    /// Kosong bila `ec_auto` (firmware yang pegang); garis datar bila manual.
    async fn get_curve(&self) -> Vec<(i32, u8)> {
        let st = self.state.read().await;
        if st.fan_ec_auto {
            return Vec::new();
        }
        if let Some(d) = st.fan_manual {
            return vec![(20, d), (100, d)];
        }
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

    /// Current fan mode: "curve" | "manual" | "ec_auto".
    async fn get_fan_mode(&self) -> String {
        self.state.read().await.fan_mode().to_string()
    }

    /// Kembalikan kipas ke firmware EC auto. Loop berhenti menulis duty
    /// (satu-kali `set_auto()` di tick berikut); RAPL/PPD tetap ikut profil.
    async fn set_fan_ec_auto(&self, auto: bool) -> String {
        let mut st = self.state.write().await;
        st.fan_ec_auto = auto;
        if auto {
            st.fan_manual = None;
        }
        st.pending_apply = true;
        st.label()
    }

    /// Kunci duty manual via loop daemon (clamp 40–100% di sini —
    /// klien JS untrusted). Mengalahkan kurva DAN ec_auto (ec_auto
    /// otomatis dimatikan agar UI tak perlu urutan panggilan khusus).
    /// Tulis EC-nya tetap di main loop, bukan di sini.
    async fn set_fan_manual(&self, duty: u8) -> String {
        let want = duty.clamp(
            axioo_lib::fan::MIN_FAN_DUTY_PCT,
            axioo_lib::fan::MAX_FAN_DUTY_PCT,
        );
        let mut st = self.state.write().await;
        st.fan_ec_auto = false;
        st.fan_manual = Some(want);
        st.pending_apply = true;
        st.label()
    }

    /// Kembali ke kurva daemon (hapus override manual).
    async fn clear_fan_override(&self) -> String {
        let mut st = self.state.write().await;
        st.fan_manual = None;
        st.pending_apply = true;
        st.label()
    }

    /// Simpan + terapkan warna keyboard (dipakai GUI/CLI sebagai user —
    /// mereka tak bisa tulis `/var/lib` langsung). Daemon (root) yang
    /// tulis sysfs + file state; boot-restore baca file itu sebelum SDDM.
    /// Hanya warna dasar statis yang diterapkan di sini; animasi efek
    /// jalan di proses GUI/CLI, warna dasarnya cukup untuk SDDM.
    #[allow(clippy::too_many_arguments)]
    async fn set_kbd(
        &self,
        brightness: u32,
        r: u8,
        g: u8,
        b: u8,
        effect: String,
        rear: String,
        speed: f32,
    ) -> zbus::fdo::Result<String> {
        let p = crate::kbd_state::sanitize(brightness, r, g, b, &effect, &rear, speed)
            .map_err(zbus::fdo::Error::InvalidArgs)?;
        let n = crate::kbd_state::apply_static(p.brightness, p.rgb)
            .map_err(zbus::fdo::Error::Failed)?;
        crate::kbd_state::save(&p).map_err(zbus::fdo::Error::Failed)?;
        Ok(format!(
            "keyboard saved: brightness {} rgb({},{},{}) effect={} ({} zones)",
            p.brightness, p.rgb.0, p.rgb.1, p.rgb.2, p.effect, n,
        ))
    }

    /// State keyboard tersimpan (JSON; "" bila belum pernah disimpan).
    async fn get_kbd(&self) -> String {
        match crate::kbd_state::load() {
            Some(p) => format!(
                "{{\"brightness\":{},\"r\":{},\"g\":{},\"b\":{},\"effect\":\"{}\",\"rear\":\"{}\",\"speed\":{}}}",
                p.brightness, p.rgb.0, p.rgb.1, p.rgb.2, p.effect, p.rear, p.speed,
            ),
            None => String::new(),
        }
    }

    #[zbus(property)]
    async fn profile(&self) -> String {
        self.get_profile().await
    }
}
