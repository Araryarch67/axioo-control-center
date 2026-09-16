//! `axioo-ctl profile`: thin client over `com.axioo.Control` + PPD fallback.
//!
//! - Daemon jalan → `GetProfile`/`SetProfile` via `com.axioo.Control`
//!   (daemon yang teruskan ke PPD + apply RAPL + ganti kurva).
//! - Daemon mati → fallback langsung ke PPD (`ActiveProfile` get/set),
//!   supaya CLI tetap berguna.

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
}

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

fn ppd_new(conn: &Connection) -> Option<PpdNewProxyBlocking<'_>> {
    PpdNewProxyBlocking::builder(conn)
        .destination("org.freedesktop.UPower.PowerProfiles")
        .ok()?
        .build()
        .ok()
}

fn ppd_old(conn: &Connection) -> Option<PpdOldProxyBlocking<'_>> {
    PpdOldProxyBlocking::builder(conn)
        .destination("net.hadess.PowerProfiles")
        .ok()?
        .build()
        .ok()
}

fn ppd_get(conn: &Connection) -> Option<String> {
    if let Some(p) = ppd_new(conn) {
        if let Ok(v) = p.active_profile() {
            return Some(v);
        }
    }
    ppd_old(conn)?.active_profile().ok()
}

fn ppd_set(conn: &Connection, profile: &str) -> bool {
    if let Some(p) = ppd_new(conn) {
        if p.set_active_profile(profile).is_ok() {
            return true;
        }
    }
    ppd_old(conn)
        .map(|p| p.set_active_profile(profile).is_ok())
        .unwrap_or(false)
}

/// Map axioo 3-mode → PPD (sama seperti daemon).
/// Legacy `quiet`/`power-saver` → `power-saver` (daemon: Balanced+quiet).
fn to_ppd(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "balanced" => Some("balanced"),
        "entertainment" | "performance" | "perf" => Some("performance"),
        "quiet" | "power-saver" | "powersaver" => Some("power-saver"),
        _ => None,
    }
}

pub fn get() {
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            println!("profile: D-Bus system bus unreachable ({e})");
            return;
        }
    };
    // 1. Coba daemon dulu.
    if let Ok(proxy) = AxiooControlProxyBlocking::new(&conn) {
        if let Ok(p) = proxy.get_profile() {
            let ppd = proxy.get_ppd_profile().unwrap_or_else(|_| "-".to_string());
            let quiet = proxy.get_quiet_fan().unwrap_or(false);
            let q = if quiet {
                "quiet-fan ON"
            } else {
                "quiet-fan off"
            };
            println!("axioo profile: {p} [{q}] (PPD: {ppd})");
            return;
        }
        println!("profile: daemon not responding — falling back to PPD directly");
    }
    match ppd_get(&conn) {
        Some(p) => println!("PPD ActiveProfile: {p} (axiood not running)"),
        None => println!("profile: PPD unreachable (daemon off + PPD off?)"),
    }
}

pub fn set(name: &str) {
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            println!("profile: D-Bus system bus unreachable ({e})");
            return;
        }
    };
    if let Ok(proxy) = AxiooControlProxyBlocking::new(&conn) {
        match proxy.set_profile(name) {
            Ok(applied) => {
                println!("axioo profile -> {applied} (via axiood, PPD synced too)");
                return;
            }
            Err(e) => println!("profile: daemon refused ({e}) — falling back to PPD directly"),
        }
    }
    let Some(ppd) = to_ppd(name) else {
        println!("profile: '{name}' unknown (choose: Balanced/Entertainment/Performance)");
        return;
    };
    if ppd_set(&conn, ppd) {
        println!("PPD -> {ppd} (axiood not running; RAPL/curve not applied)");
    } else {
        println!("profile: failed to set PPD (needs polkit? try again)");
    }
}

/// `axioo-ctl profile quiet-fan on|off|toggle|status`
pub fn quiet_fan(action: &str) {
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            println!("profile: D-Bus system bus unreachable ({e})");
            return;
        }
    };
    let proxy = match AxiooControlProxyBlocking::new(&conn) {
        Ok(p) => p,
        Err(_) => {
            println!("profile: axiood not running (quiet-fan needs daemon)");
            return;
        }
    };
    // Daemon path absent → proxy exists but calls fail with ServiceUnknown.
    if proxy.get_profile().is_err() {
        println!("profile: axiood not running (quiet-fan needs daemon)");
        return;
    }
    let want = match action.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "enable" => Some(true),
        "off" | "false" | "0" | "disable" => Some(false),
        "toggle" => proxy.get_quiet_fan().ok().map(|q| !q),
        "status" => {
            match proxy.get_quiet_fan() {
                Ok(q) => println!("quiet-fan: {}", if q { "ON" } else { "off" }),
                Err(e) => println!("profile: failed to read quiet-fan ({e})"),
            }
            return;
        }
        _ => None,
    };
    match want {
        Some(q) => match proxy.set_quiet_fan(q) {
            Ok(label) => println!("axioo profile -> {label}"),
            Err(e) => println!("profile: failed to set quiet-fan ({e})"),
        },
        None => println!("profile: unknown action '{action}' (choose: on/off/toggle/status)"),
    }
}
