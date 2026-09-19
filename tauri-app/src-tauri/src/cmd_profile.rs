//! Profile + quiet-fan commands (via axiood).

use tauri::Manager;

use crate::dbus::AxiooControlProxy;
use crate::{sync_tray_checks, TrayProfiles};

#[tauri::command]
pub(crate) async fn set_profile(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let clean: String = name.chars().take(32).collect();
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    let proxy = AxiooControlProxy::new(&conn).await.map_err(|_| {
        "axiood not running — run './install-system.sh' from tauri-app/ then restart the app"
            .to_string()
    })?;
    let label = proxy.set_profile(&clean).await.map_err(|e| {
        if format!("{e}").contains("ServiceUnknown") {
            "axiood not running — run './install-system.sh' from tauri-app/ then restart the app"
                .to_string()
        } else {
            format!("daemon refused: {e}")
        }
    })?;
    // Jaga centang tray tetap sinkron walau ganti via jendela.
    if let Some(items) = app.try_state::<TrayProfiles>() {
        sync_tray_checks(&items, &label);
    }
    Ok(label)
}
#[tauri::command]
pub(crate) async fn set_quiet_fan(quiet: bool) -> Result<String, String> {
    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("D-Bus unreachable: {e}"))?;
    let proxy = AxiooControlProxy::new(&conn).await.map_err(|_| {
        "axiood not running — run './install-system.sh' from tauri-app/ then restart the app"
            .to_string()
    })?;
    proxy
        .set_quiet_fan(quiet)
        .await
        .map_err(|e| format!("quiet-fan failed: {e}"))
}
