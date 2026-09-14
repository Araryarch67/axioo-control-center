//! power-profiles-daemon client (two-way sync).
//!
//! Mendukung dua nama bus (PPD lama 0.30 pakai `net.hadess`, versi baru
//! pakai `org.freedesktop.UPower`):
//! - new: `org.freedesktop.UPower.PowerProfiles` @ `/org/freedesktop/UPower/PowerProfiles`
//! - old: `net.hadess.PowerProfiles` @ `/net/hadess/PowerProfiles`

use zbus::{proxy, Connection};

pub const PPD_NEW_SERVICE: &str = "org.freedesktop.UPower.PowerProfiles";
pub const PPD_NEW_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
pub const PPD_OLD_SERVICE: &str = "net.hadess.PowerProfiles";
pub const PPD_OLD_PATH: &str = "/net/hadess/PowerProfiles";

#[proxy(
    interface = "net.hadess.PowerProfiles",
    default_path = "/net/hadess/PowerProfiles"
)]
trait PpdOld {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PpdNew {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;
}

/// Which PPD bus identity answered.
#[derive(Debug, Clone, Copy)]
pub enum PpdFlavor {
    New,
    Old,
}

/// Read `ActiveProfile`, trying the new bus name first.
pub async fn get_active_profile(conn: &Connection) -> zbus::Result<(String, PpdFlavor)> {
    // New name first.
    if let Ok(proxy) = PpdNewProxy::builder(conn)
        .destination(PPD_NEW_SERVICE)
        .unwrap()
        .path(PPD_NEW_PATH)
        .unwrap()
        .build()
        .await
    {
        if let Ok(p) = proxy.active_profile().await {
            return Ok((p, PpdFlavor::New));
        }
    }
    let proxy = PpdOldProxy::builder(conn)
        .destination(PPD_OLD_SERVICE)
        .unwrap()
        .path(PPD_OLD_PATH)
        .unwrap()
        .build()
        .await?;
    Ok((proxy.active_profile().await?, PpdFlavor::Old))
}

/// Set `ActiveProfile` via `org.freedesktop.DBus.Properties.Set`
/// (cara yang dipakai `powerprofilesctl set`).
pub async fn set_active_profile(
    conn: &Connection,
    flavor: PpdFlavor,
    profile: &str,
) -> zbus::Result<()> {
    match flavor {
        PpdFlavor::New => {
            let proxy = PpdNewProxy::builder(conn)
                .destination(PPD_NEW_SERVICE)
                .unwrap()
                .path(PPD_NEW_PATH)
                .unwrap()
                .build()
                .await?;
            proxy.set_active_profile(profile).await
        }
        PpdFlavor::Old => {
            let proxy = PpdOldProxy::builder(conn)
                .destination(PPD_OLD_SERVICE)
                .unwrap()
                .path(PPD_OLD_PATH)
                .unwrap()
                .build()
                .await?;
            proxy.set_active_profile(profile).await
        }
    }
}

/// Detect flavor once at startup (new preferred).
pub async fn detect_flavor(conn: &Connection) -> PpdFlavor {
    get_active_profile(conn)
        .await
        .map(|(_, f)| f)
        .unwrap_or(PpdFlavor::Old)
}
