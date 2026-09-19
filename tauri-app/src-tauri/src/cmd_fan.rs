//! Fan one-shot commands (grade-gated, daemon owns the loop).

use axioo_lib::{
    devices::{self, FanWrite},
    fan, fan_ctrl,
};

use crate::dbus::{daemon_proxy, daemon_reachable};

/// Shared grade gate for both one-shot commands. Policy lives in
/// `axioo_lib::devices`; this only renders the refusal.
pub(crate) fn ensure_fan_writable() -> Result<devices::DeviceProfile, String> {
    let gate = devices::fan_gate();
    match gate.decision {
        FanWrite::Allowed | FanWrite::AllowedWithWarning => Ok(gate.profile),
        FanWrite::Locked => Err(format!(
            "fan writes locked for this model ({} — grade: {}). Validate the EC map first: `axioo-ctl fan dump`.",
            gate.display,
            gate.profile.grade.as_str()
        )),
    }
}
/// One-shot manual fan duty. REJECTED while axiood runs (daemon owns EC)
/// and on unvalidated models (dynamic device DB grade gate).
/// Clamp + verify enforced inside `fan_ctrl` (slider JS is untrusted).
#[tauri::command]
pub(crate) async fn fan_set_manual(duty: u8) -> Result<String, String> {
    if daemon_reachable().await {
        return Err("axiood active: direct control disabled — use mode + quiet-fan.".to_string());
    }
    ensure_fan_writable()?;
    let want = duty.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    match fan_ctrl::set_manual_duty(want) {
        Ok(r) => Ok(match (r.verified_pct, r.verify_skipped) {
            (Some(v), _) => format!("Manual {want}%: OK (EC {v}%)"),
            (None, true) => format!("Manual {want}%: OK (without ec_sys verification)"),
            _ => format!("Manual {want}%: OK"),
        }),
        Err(fan_ctrl::FanCtrlError::NotRoot) => Err(
            "Needs root for direct EC writes. Fix: enable axiood ('./install-system.sh') then use mode + quiet-fan.".to_string(),
        ),
        Err(e) => Err(format!("fan set failed: {e}")),
    }
}
#[tauri::command]
pub(crate) async fn fan_set_auto() -> Result<String, String> {
    if daemon_reachable().await {
        return Err("axiood active: direct control disabled — use mode + quiet-fan.".to_string());
    }
    ensure_fan_writable()?;
    match fan_ctrl::set_auto() {
        Ok(()) => Ok("EC auto: OK".to_string()),
        Err(fan_ctrl::FanCtrlError::NotRoot) => Err(
            "Needs root for direct EC writes. Fix: enable axiood ('./install-system.sh') then use mode + quiet-fan.".to_string(),
        ),
        Err(e) => Err(format!("fan auto failed: {e}")),
    }
}
/// Serahkan kipas ke firmware EC auto (via daemon; RAPL/PPD tetap ikut profil).
#[tauri::command]
pub(crate) async fn set_fan_ec_auto(auto: bool) -> Result<String, String> {
    let proxy = daemon_proxy().await?;
    proxy
        .set_fan_ec_auto(auto)
        .await
        .map_err(|e| format!("ec_auto failed: {e}"))
}
/// Kunci duty manual via loop daemon (clamp 40–100% di daemon).
#[tauri::command]
pub(crate) async fn set_fan_manual(duty: u8) -> Result<String, String> {
    let want = duty.clamp(fan::MIN_FAN_DUTY_PCT, fan::MAX_FAN_DUTY_PCT);
    let proxy = daemon_proxy().await?;
    proxy
        .set_fan_manual(want)
        .await
        .map_err(|e| format!("manual failed: {e}"))
}
/// Kembali ke kurva daemon (hapus override manual).
#[tauri::command]
pub(crate) async fn clear_fan_override() -> Result<String, String> {
    let proxy = daemon_proxy().await?;
    proxy
        .clear_fan_override()
        .await
        .map_err(|e| format!("return to curve failed: {e}"))
}
