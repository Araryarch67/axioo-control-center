//! Battery threshold command.

use axioo_lib::battery;

#[tauri::command]
pub(crate) async fn battery_set(start: Option<u64>, end: Option<u64>) -> Result<String, String> {
    // Re-validate ranges server-side (JS untrusted).
    if let Some(s) = start {
        if !(40..=95).contains(&s) {
            return Err("start must be 40-95.".to_string());
        }
    }
    if let Some(e) = end {
        if !(60..=100).contains(&e) {
            return Err("end must be 60-100.".to_string());
        }
    }
    if let (Some(s), Some(e)) = (start, end) {
        if s >= e {
            return Err("start must be < end.".to_string());
        }
    }
    battery::set_charge_thresholds("BAT0", start, end).map_err(|e| {
        let msg = format!("{e}");
        if msg.contains("permission denied") {
            "battery: permission denied — run './install-system.sh' (udev rule) then restart the app".to_string()
        } else {
            format!("battery set failed: {e}")
        }
    })?;
    Ok("Charge threshold saved.".to_string())
}
