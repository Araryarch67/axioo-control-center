//! D-Bus proxies (async, same bus as axiood).

// ---------- D-Bus proxies (async, same bus as axiood) ----------
#[zbus::proxy(
    interface = "com.axioo.Control",
    default_service = "com.axioo.Control",
    default_path = "/com/axioo/Control"
)]
pub(crate) trait AxiooControl {
    async fn get_profile(&self) -> zbus::Result<String>;
    async fn set_profile(&self, profile: &str) -> zbus::Result<String>;
    async fn get_ppd_profile(&self) -> zbus::Result<String>;
    async fn get_quiet_fan(&self) -> zbus::Result<bool>;
    async fn set_quiet_fan(&self, quiet: bool) -> zbus::Result<String>;
    async fn get_fan_duty(&self) -> zbus::Result<u8>;
    async fn get_power_watts(&self) -> zbus::Result<f64>;
    async fn get_curve(&self) -> zbus::Result<Vec<(i32, u8)>>;
    async fn get_fan_mode(&self) -> zbus::Result<String>;
    async fn set_fan_ec_auto(&self, auto: bool) -> zbus::Result<String>;
    async fn set_fan_manual(&self, duty: u8) -> zbus::Result<String>;
    async fn clear_fan_override(&self) -> zbus::Result<String>;
    #[allow(clippy::too_many_arguments)]
    async fn set_kbd(
        &self,
        brightness: u32,
        r: u8,
        g: u8,
        b: u8,
        effect: &str,
        rear: &str,
        speed: f32,
    ) -> zbus::Result<String>;
    async fn get_kbd(&self) -> zbus::Result<String>;
}
#[zbus::proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
pub(crate) trait PpdNew {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}
#[zbus::proxy(
    interface = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
pub(crate) trait PpdOld {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
}
pub(crate) async fn daemon_reachable() -> bool {
    if let Ok(conn) = zbus::Connection::system().await {
        if let Ok(proxy) = AxiooControlProxy::new(&conn).await {
            return proxy.get_profile().await.is_ok();
        }
    }
    false
}
pub(crate) async fn daemon_proxy() -> Result<AxiooControlProxy<'static>, String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    AxiooControlProxy::new(&conn).await.map_err(|_| {
        "axiood not running — run './install-system.sh' from tauri-app/ then restart the app"
            .to_string()
    })
}
