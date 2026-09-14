//! Klien D-Bus tipis (blocking) buat GUI: `com.axioo.Control` + fallback PPD.
//!
//! Aturan main (biar tak ada state aneh):
//! - Daemon jalan → daemon satu-satunya sumber kebenaran (profil, quiet,
//!   kurva, duty). GUI cuma tampilkan + kirim `Set*`.
//! - Daemon mati → fallback baca PPD langsung (tampilan saja); tulis EC
//!   tetap lewat jalur legacy one-shot yang sudah ada.
//! - Tabel kurva JANGAN diduplikasi di sini: chart baca `GetCurve` daemon.

use zbus::{blocking::Connection, proxy};

#[proxy(
    interface = "com.axioo.Control",
    default_service = "com.axioo.Control",
    default_path = "/com/axioo/Control"
)]
trait AxiooControl {
    async fn get_profile(&self) -> zbus::Result<String>;
    async fn set_profile(&self, profile: &str) -> zbus::Result<String>;
    async fn get_ppd_profile(&self) -> zbus::Result<String>;
    async fn get_quiet_fan(&self) -> zbus::Result<bool>;
    async fn set_quiet_fan(&self, quiet: bool) -> zbus::Result<String>;
    async fn get_fan_duty(&self) -> zbus::Result<u8>;
    async fn get_curve(&self) -> zbus::Result<Vec<(i32, u8)>>;
}

#[proxy(
    interface = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
trait PpdOld {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PpdNew {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}

/// Snapshot state profil buat thread sampler → UI.
#[derive(Clone, Debug, Default)]
pub struct ProfileState {
    /// `false` = axiood tak terjangkau (tampilan dari PPD saja).
    pub daemon: bool,
    /// "Balanced" | "Entertainment" | "Performance" (+ "Custom" legacy lokal).
    pub profile: String,
    pub quiet_fan: bool,
    /// Profil PPD mentah apa adanya ("balanced", "performance", ...).
    pub ppd: String,
    /// Kurva efektif dari daemon; kosong bila daemon mati.
    pub curve: Vec<(i32, u8)>,
    /// Duty terakhir dari loop daemon; 0 bila tak diketahui.
    pub fan_duty: u8,
}

fn ppd_active(conn: &Connection) -> Option<String> {
    if let Ok(p) = PpdNewProxyBlocking::builder(conn)
        .destination("org.freedesktop.UPower.PowerProfiles")
        .ok()?
        .build()
    {
        if let Ok(v) = p.active_profile() {
            return Some(v);
        }
    }
    PpdOldProxyBlocking::builder(conn)
        .destination("net.hadess.PowerProfiles")
        .ok()?
        .build()
        .ok()?
        .active_profile()
        .ok()
}

/// Label tampilan + hint quiet saat daemon mati.
/// `power-saver` → Balanced dengan quiet (cermin mapping daemon).
pub fn ppd_display(ppd: &str) -> (&'static str, bool) {
    match ppd {
        "power-saver" => ("Balanced", true),
        "balanced" => ("Balanced", false),
        "performance" => ("Performance", false),
        _ => ("Balanced", false),
    }
}

/// Satu round-trip state (dipanggil tiap ~2 detik dari sampler thread).
pub fn query(conn: &Connection) -> ProfileState {
    if let Ok(proxy) = AxiooControlProxyBlocking::new(conn) {
        if let Ok(profile) = proxy.get_profile() {
            return ProfileState {
                daemon: true,
                profile,
                quiet_fan: proxy.get_quiet_fan().unwrap_or(false),
                ppd: proxy.get_ppd_profile().unwrap_or_else(|_| "-".to_string()),
                curve: proxy.get_curve().unwrap_or_default(),
                fan_duty: proxy.get_fan_duty().unwrap_or(0),
            };
        }
    }
    // Fallback: PPD langsung, tampilan saja.
    match ppd_active(conn) {
        Some(ppd) => {
            let (label, quiet) = ppd_display(&ppd);
            ProfileState {
                daemon: false,
                profile: label.to_string(),
                quiet_fan: quiet,
                ppd,
                curve: Vec::new(),
                fan_duty: 0,
            }
        }
        None => ProfileState {
            daemon: false,
            ppd: "-".to_string(),
            profile: "Balanced".to_string(),
            ..Default::default()
        },
    }
}

/// Minta daemon ganti profil. `Err` bila daemon mati (panggil jalur legacy).
pub fn set_profile(conn: &Connection, name: &str) -> Result<String, String> {
    let proxy =
        AxiooControlProxyBlocking::new(conn).map_err(|_| "axiood tidak jalan".to_string())?;
    proxy.set_profile(name).map_err(|e| {
        if format!("{e}").contains("ServiceUnknown") {
            "axiood tidak jalan".to_string()
        } else {
            format!("daemon menolak: {e}")
        }
    })
}

/// Minta daemon set toggle quiet-fan.
pub fn set_quiet_fan(conn: &Connection, quiet: bool) -> Result<String, String> {
    let proxy =
        AxiooControlProxyBlocking::new(conn).map_err(|_| "axiood tidak jalan".to_string())?;
    proxy.set_quiet_fan(quiet).map_err(|e| {
        if format!("{e}").contains("ServiceUnknown") {
            "axiood tidak jalan".to_string()
        } else {
            format!("daemon menolak: {e}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppd_display_mapping() {
        assert_eq!(ppd_display("power-saver"), ("Balanced", true));
        assert_eq!(ppd_display("balanced"), ("Balanced", false));
        assert_eq!(ppd_display("performance"), ("Performance", false));
        assert_eq!(ppd_display("alien"), ("Balanced", false));
    }
}
