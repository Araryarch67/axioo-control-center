//! Persistensi warna keyboard terakhir untuk restore saat boot.
//!
//! Masalah: firmware/EC me-reset backlight ke putih tiap reboot, dan GUI
//! (`axioo-center.service`, user unit) baru jalan setelah login — SDDM
//! selalu putih. Daemon (`axiood`, system service, root) jalan sebelum
//! display-manager, jadi ia yang restore warna tersimpan ke sysfs.
//!
//! Format `/var/lib/axiood/kbd.json`:
//! `{"brightness":255,"r":255,"g":95,"b":86,"effect":"static","rear":"follow","speed":1.0}`
//! Ditulis tiap `SetKbd` via D-Bus (GUI/CLI sebagai user tak bisa tulis
//! `/var/lib` langsung) dan tiap `axioo-ctl kbd set` sebagai root.
//! Restore hanya menerapkan warna dasar statis (cukup untuk SDDM/login;
//! animasi efek jalan lagi setelah GUI login via thread userspace-nya).

use axioo_lib::{kbd, kbd_effect::KbdEffect};

/// Lokasi state (root-owned; GUI user hanya tulis via D-Bus `SetKbd`).
pub const KBD_STATE_PATH: &str = "/var/lib/axiood/kbd.json";

/// State tersimpan (satu warna dasar + efek terakhir).
#[derive(Debug, Clone)]
pub struct KbdPersist {
    pub brightness: u32,
    pub rgb: (u8, u8, u8),
    pub effect: String,
    pub rear: String,
    pub speed: f32,
}

impl Default for KbdPersist {
    fn default() -> Self {
        Self {
            brightness: 255,
            rgb: (255, 255, 255),
            effect: "static".to_string(),
            rear: "follow".to_string(),
            speed: 1.0,
        }
    }
}

/// Validasi + normalisasi input D-Bus/CLI (JS/argv untrusted).
pub fn sanitize(
    brightness: u32,
    r: u8,
    g: u8,
    b: u8,
    effect: &str,
    rear: &str,
    speed: f32,
) -> Result<KbdPersist, String> {
    let effect_clean: String = effect.chars().take(16).collect();
    let rear_clean: String = rear.chars().take(16).collect();
    if KbdEffect::parse(&effect_clean).is_none() {
        return Err(format!("unknown effect: '{effect_clean}'"));
    }
    if !rear_clean.is_empty()
        && !rear_clean.eq_ignore_ascii_case("follow")
        && KbdEffect::parse(&rear_clean).is_none()
    {
        return Err(format!("unknown rear effect: '{rear_clean}'"));
    }
    Ok(KbdPersist {
        brightness: brightness.min(255),
        rgb: (r, g, b),
        effect: effect_clean,
        rear: if rear_clean.is_empty() {
            "follow".to_string()
        } else {
            rear_clean
        },
        speed: speed.clamp(0.1, 4.0),
    })
}

/// Muat state dari disk (`None` = belum pernah disimpan / rusak).
pub fn load() -> Option<KbdPersist> {
    let raw = std::fs::read_to_string(KBD_STATE_PATH).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let brightness = v.get("brightness")?.as_u64()? as u32;
    let r = v.get("r")?.as_u64()? as u8;
    let g = v.get("g")?.as_u64()? as u8;
    let b = v.get("b")?.as_u64()? as u8;
    let effect = v.get("effect").and_then(|e| e.as_str()).unwrap_or("static");
    let rear = v.get("rear").and_then(|e| e.as_str()).unwrap_or("follow");
    let speed = v.get("speed").and_then(|s| s.as_f64()).unwrap_or(1.0) as f32;
    sanitize(brightness, r, g, b, effect, rear, speed).ok()
}

/// Simpan state ke disk (buat dir bila belum ada).
pub fn save(p: &KbdPersist) -> Result<(), String> {
    if let Some(dir) = std::path::Path::new(KBD_STATE_PATH).parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("kbd state dir: {e}"))?;
    }
    let obj = serde_json::json!({
        "brightness": p.brightness,
        "r": p.rgb.0,
        "g": p.rgb.1,
        "b": p.rgb.2,
        "effect": p.effect,
        "rear": p.rear,
        "speed": p.speed,
    });
    std::fs::write(KBD_STATE_PATH, obj.to_string()).map_err(|e| format!("kbd state write: {e}"))?;
    Ok(())
}

/// Terapkan warna dasar ke SEMUA zona (clamp per-device max).
/// Dipakai boot-restore + `SetKbd` daemon (efek animasi milik GUI/CLI).
pub fn apply_static(brightness: u32, rgb: (u8, u8, u8)) -> Result<usize, String> {
    let devs = kbd::discover();
    if devs.is_empty() {
        return Err("no keyboard-backlight LED found".to_string());
    }
    let mut n = 0;
    let mut first_err: Option<String> = None;
    for d in &devs {
        let br = brightness.min(d.max_brightness);
        if let Err(e) = kbd::set(d, br, rgb) {
            first_err = first_err.or_else(|| Some(format!("{e}")));
        } else {
            n += 1;
        }
    }
    first_err.map_or(Ok(n), |e| if n == 0 { Err(e) } else { Ok(n) })
}

/// Restore boot: muat file lalu tulis warna dasar ke hardware.
/// Best-effort — caller cukup log hasilnya, jangan gagalkan daemon.
pub fn restore() -> Result<String, String> {
    let p = load().ok_or_else(|| "no saved keyboard state".to_string())?;
    let n = apply_static(p.brightness, p.rgb)?;
    Ok(format!(
        "keyboard restored: brightness {} rgb({},{},{}) effect={} rear={} ({} zones)",
        p.brightness, p.rgb.0, p.rgb.1, p.rgb.2, p.effect, p.rear, n,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_unknown_effects() {
        assert!(sanitize(255, 1, 2, 3, "static", "follow", 1.0).is_ok());
        assert!(sanitize(255, 1, 2, 3, "breathing", "rainbow", 2.0).is_ok());
        assert!(sanitize(255, 1, 2, 3, "nope", "follow", 1.0).is_err());
        assert!(sanitize(255, 1, 2, 3, "static", "nope", 1.0).is_err());
        // Clamp, bukan tolak.
        let p = sanitize(999, 1, 2, 3, "static", "", 99.0).unwrap();
        assert_eq!(p.brightness, 255);
        assert_eq!(p.rear, "follow");
        assert_eq!(p.speed, 4.0);
    }
}
