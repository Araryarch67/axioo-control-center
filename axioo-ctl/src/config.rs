//! `axioo-ctl config`: export/import all tunables as one JSON file.
//!
//! Sections (each best-effort, failures reported per section):
//! - `profile`/`quiet_fan`/`fan`/`kbd` via `com.axioo.Control` (needs axiood)
//! - `battery` thresholds via sysfs (read as user, write needs root)
//!
//! Format v1:
//! `{"version":1,"profile":"Performance","quiet_fan":false,`
//! ` "fan":{"mode":"curve","manual_duty":null},`
//! ` "kbd":{"brightness":255,"r":255,"g":95,"b":86,"effect":"static","rear":"follow","speed":1.0},`
//! ` "battery":{"start":95,"end":100}}`
//! Missing sections (daemon down) are omitted on export with a stderr note;
//! import skips absent sections silently.

use zbus::{blocking::Connection, proxy};

#[proxy(
    interface = "com.axioo.Control",
    default_service = "com.axioo.Control",
    default_path = "/com/axioo/Control"
)]
trait AxiooControl {
    async fn get_profile(&self) -> zbus::Result<String>;
    async fn set_profile(&self, profile: &str) -> zbus::Result<String>;
    async fn get_quiet_fan(&self) -> zbus::Result<bool>;
    async fn set_quiet_fan(&self, quiet: bool) -> zbus::Result<String>;
    async fn get_fan_mode(&self) -> zbus::Result<String>;
    async fn get_fan_duty(&self) -> zbus::Result<u8>;
    async fn set_fan_ec_auto(&self, auto: bool) -> zbus::Result<String>;
    async fn set_fan_manual(&self, duty: u8) -> zbus::Result<String>;
    async fn clear_fan_override(&self) -> zbus::Result<String>;
    async fn get_kbd(&self) -> zbus::Result<String>;
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
}

fn daemon() -> Option<AxiooControlProxyBlocking<'static>> {
    let conn = Connection::system().ok()?;
    let proxy = AxiooControlProxyBlocking::new(&conn).ok()?;
    // Verify the name is actually served (not just the bus).
    proxy.get_profile().ok()?;
    Some(proxy)
}

pub fn export(file: Option<String>) {
    let mut root = serde_json::Map::new();
    root.insert("version".into(), serde_json::json!(1));

    match daemon() {
        None => eprintln!("config export: axiood unreachable — daemon sections omitted"),
        Some(p) => {
            if let Ok(v) = p.get_profile() {
                root.insert("profile".into(), serde_json::json!(v));
            }
            if let Ok(v) = p.get_quiet_fan() {
                root.insert("quiet_fan".into(), serde_json::json!(v));
            }
            let mut fan = serde_json::Map::new();
            if let Ok(mode) = p.get_fan_mode() {
                fan.insert("mode".into(), serde_json::json!(mode));
                fan.insert(
                    "manual_duty".into(),
                    if mode == "manual" {
                        p.get_fan_duty().ok().into()
                    } else {
                        serde_json::Value::Null
                    },
                );
                root.insert("fan".into(), serde_json::Value::Object(fan));
            }
            match p.get_kbd() {
                Ok(s) if !s.is_empty() => match serde_json::from_str::<serde_json::Value>(&s) {
                    Ok(v) => {
                        root.insert("kbd".into(), v);
                    }
                    Err(e) => eprintln!("config export: kbd state unparsable ({e})"),
                },
                _ => eprintln!("config export: no saved keyboard state yet"),
            }
        }
    }

    let bats = axioo_lib::battery::batteries();
    if let Some(b) = bats.first() {
        let mut bat = serde_json::Map::new();
        bat.insert(
            "start".into(),
            b.charge_start_threshold
                .map_or(serde_json::Value::Null, |v| v.into()),
        );
        bat.insert(
            "end".into(),
            b.charge_end_threshold
                .map_or(serde_json::Value::Null, |v| v.into()),
        );
        root.insert("battery".into(), serde_json::Value::Object(bat));
    } else {
        eprintln!("config export: no battery found");
    }

    let text = serde_json::to_string_pretty(&serde_json::Value::Object(root)).unwrap();
    match file {
        Some(path) => match std::fs::write(&path, format!("{text}\n")) {
            Ok(()) => println!("config exported to {path}"),
            Err(e) => {
                eprintln!("error: cannot write {path} ({e})");
                std::process::exit(3);
            }
        },
        None => println!("{text}"),
    }
}

pub fn import(file: Option<String>) {
    let text = match file {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("error: cannot read {path} ({e})");
            std::process::exit(2);
        }),
        None => std::io::read_to_string(std::io::stdin()).unwrap_or_else(|e| {
            eprintln!("error: cannot read stdin ({e})");
            std::process::exit(2);
        }),
    };
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("error: invalid JSON ({e})");
        std::process::exit(2);
    });
    if v.get("version").and_then(|x| x.as_u64()) != Some(1) {
        eprintln!("error: unsupported config version (expected 1)");
        std::process::exit(2);
    }
    let mut failed = 0;

    if v.get("profile")
        .or(v.get("quiet_fan"))
        .or(v.get("fan"))
        .or(v.get("kbd"))
        .is_some()
    {
        match daemon() {
            None => {
                eprintln!("config import: axiood unreachable — daemon sections skipped");
                failed += 1;
            }
            Some(p) => {
                if let Some(name) = v.get("profile").and_then(|x| x.as_str()) {
                    match p.set_profile(name) {
                        Ok(label) => println!("config: profile -> {label}"),
                        Err(e) => {
                            eprintln!("config: profile FAILED ({e})");
                            failed += 1;
                        }
                    }
                }
                if let Some(q) = v.get("quiet_fan").and_then(|x| x.as_bool()) {
                    match p.set_quiet_fan(q) {
                        Ok(label) => println!("config: quiet-fan -> {label}"),
                        Err(e) => {
                            eprintln!("config: quiet-fan FAILED ({e})");
                            failed += 1;
                        }
                    }
                }
                if let Some(fan) = v.get("fan") {
                    let mode = fan.get("mode").and_then(|x| x.as_str()).unwrap_or("curve");
                    let r: Result<String, String> = match mode {
                        "ec_auto" => p
                            .set_fan_ec_auto(true)
                            .map(|_| "ec-auto".to_string())
                            .map_err(|e| e.to_string()),
                        "manual" => match fan.get("manual_duty").and_then(|x| x.as_u64()) {
                            Some(d) => p
                                .set_fan_manual(d.clamp(40, 100) as u8)
                                .map(|_| format!("manual {d}%"))
                                .map_err(|e| e.to_string()),
                            None => Err("manual mode without manual_duty".to_string()),
                        },
                        _ => p
                            .clear_fan_override()
                            .and_then(|_| p.set_fan_ec_auto(false))
                            .map(|_| "curve".to_string())
                            .map_err(|e| e.to_string()),
                    };
                    match r {
                        Ok(s) => println!("config: fan -> {s}"),
                        Err(e) => {
                            eprintln!("config: fan FAILED ({e})");
                            failed += 1;
                        }
                    }
                }
                if let Some(k) = v.get("kbd") {
                    let g = |key: &str| k.get(key);
                    let r = p.set_kbd(
                        g("brightness").and_then(|x| x.as_u64()).unwrap_or(255) as u32,
                        g("r").and_then(|x| x.as_u64()).unwrap_or(255) as u8,
                        g("g").and_then(|x| x.as_u64()).unwrap_or(255) as u8,
                        g("b").and_then(|x| x.as_u64()).unwrap_or(255) as u8,
                        g("effect").and_then(|x| x.as_str()).unwrap_or("static"),
                        g("rear").and_then(|x| x.as_str()).unwrap_or("follow"),
                        g("speed").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32,
                    );
                    match r {
                        Ok(s) => println!("config: {s}"),
                        Err(e) => {
                            eprintln!("config: kbd FAILED ({e})");
                            failed += 1;
                        }
                    }
                }
            }
        }
    }

    if let Some(b) = v.get("battery") {
        let start = b.get("start").and_then(|x| x.as_u64());
        let end = b.get("end").and_then(|x| x.as_u64());
        if start.is_some() || end.is_some() {
            let name = axioo_lib::battery::batteries()
                .first()
                .map(|x| x.name.clone())
                .unwrap_or_else(|| "BAT0".to_string());
            match axioo_lib::battery::set_charge_thresholds(&name, start, end) {
                Ok(()) => println!("config: battery thresholds saved (needs root)"),
                Err(e) => {
                    eprintln!("config: battery FAILED ({e})");
                    failed += 1;
                }
            }
        }
    }

    if failed > 0 {
        eprintln!("config import: {failed} section(s) failed");
        std::process::exit(3);
    }
    println!("config import: OK");
}
