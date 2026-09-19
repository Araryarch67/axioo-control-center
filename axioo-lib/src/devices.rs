//! Device database: grade the running machine from DMI at runtime.
//!
//! Table lives in `axioo-lib/devices.toml` (embedded fallback) and can be
//! overridden without rebuild via `$AXIOO_DEVICES`,
//! `/etc/axioo-control-center/devices.toml`, or
//! `/usr/share/axioo-control-center/devices.toml`.
//! Only [`current`]/[`fan_gate`] do I/O. Fan writes stay gated by
//! [`DeviceProfile::fan_write_allowed`].

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Support grade for the running machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    /// EC map validated on this exact model.
    Supported,
    /// Same barebone, untested. Writes allowed with a warning.
    Sibling,
    /// Axioo/Clevo without entry yet. Read-only until validated.
    UnknownClevo,
    /// Unlocked by local `validate --apply` on this machine.
    /// Same rights as Supported; upgrade to a named entry via issue.
    LocallyValidated,
    /// Not Axioo/Clevo. Generic read-only info only.
    Foreign,
}

impl Grade {
    pub fn as_str(self) -> &'static str {
        match self {
            Grade::Supported => "supported",
            Grade::Sibling => "sibling",
            Grade::UnknownClevo => "read-only (unvalidated Clevo/Axioo)",
            Grade::LocallyValidated => "locally validated",
            Grade::Foreign => "read-only (foreign hardware)",
        }
    }
}

impl<'de> serde::Deserialize<'de> for Grade {
    fn deserialize<D>(d: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match String::deserialize(d)?.as_str() {
            "supported" => Ok(Grade::Supported),
            "sibling" => Ok(Grade::Sibling),
            "unknown-clevo" | "unknown" => Ok(Grade::UnknownClevo),
            "foreign" => Ok(Grade::Foreign),
            other => Err(serde::de::Error::custom(format!(
                "unknown grade '{other}' (want supported|sibling|unknown-clevo|foreign)"
            ))),
        }
    }
}

/// Static profile for one device family.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct DeviceProfile {
    /// Stable id for `probe --json` `device.id`.
    pub id: String,
    /// Display override (`None` = compose from DMI at runtime).
    #[serde(default)]
    pub marketing: Option<String>,
    /// `board_name` substrings (case-insensitive).
    #[serde(default)]
    pub board_match: Vec<String>,
    /// `product_name`/`product_sku` substrings.
    #[serde(default)]
    pub product_match: Vec<String>,
    pub grade: Grade,
    /// Whether `fan set`/`auto` may write on this profile.
    #[serde(default)]
    pub fan_write_allowed: bool,
    #[serde(default)]
    pub kbd_zones: u8,
    #[serde(default)]
    pub notes: String,
}

impl DeviceProfile {
    /// Curated override, else `{sys_vendor} {product_name}`, else id.
    pub fn display_name(&self, dmi: &BTreeMap<String, String>) -> String {
        if let Some(m) = &self.marketing {
            return m.clone();
        }
        let vendor = dmi.get("sys_vendor").map_or("", String::as_str);
        let product = dmi.get("product_name").map_or("", String::as_str);
        let composed = format!("{vendor} {product}").trim().to_string();
        if composed.is_empty() {
            self.id.clone()
        } else {
            composed
        }
    }

    /// Single fan-write policy shared by CLI, daemon, and GUI.
    pub fn fan_write(&self) -> FanWrite {
        if !self.fan_write_allowed {
            FanWrite::Locked
        } else if self.grade == Grade::Sibling {
            FanWrite::AllowedWithWarning
        } else {
            FanWrite::Allowed
        }
    }
}

/// Outcome of [`DeviceProfile::fan_write`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanWrite {
    Allowed,
    AllowedWithWarning,
    Locked,
}

/// Resolved device + display name + fan-write decision in one DMI read.
pub struct FanGate {
    pub profile: DeviceProfile,
    pub display: String,
    pub decision: FanWrite,
}

/// One call for all fan-write entry points (CLI, daemon, GUI).
pub fn fan_gate() -> FanGate {
    let dmi = crate::dmi::read_dmi();
    let profile = resolve(&dmi);
    let decision = profile.fan_write();
    let display = profile.display_name(&dmi);
    FanGate {
        profile,
        display,
        decision,
    }
}

#[derive(Debug, serde::Deserialize)]
struct DbFile {
    #[serde(default)]
    device: Vec<DeviceProfile>,
}

fn unknown_clevo() -> DeviceProfile {
    DeviceProfile {
        id: "unknown-clevo".to_string(),
        marketing: None,
        board_match: Vec::new(),
        product_match: Vec::new(),
        grade: Grade::UnknownClevo,
        fan_write_allowed: false,
        kbd_zones: 0,
        notes: "Read-only until EC map validated (fan dump + probe --json).".to_string(),
    }
}

fn foreign() -> DeviceProfile {
    DeviceProfile {
        id: "foreign".to_string(),
        marketing: None,
        board_match: Vec::new(),
        product_match: Vec::new(),
        grade: Grade::Foreign,
        fan_write_allowed: false,
        kbd_zones: 0,
        notes: "Generic read-only info only.".to_string(),
    }
}

fn attested_profile() -> DeviceProfile {
    DeviceProfile {
        id: "locally-validated".to_string(),
        marketing: None,
        board_match: Vec::new(),
        product_match: Vec::new(),
        grade: Grade::LocallyValidated,
        fan_write_allowed: true,
        kbd_zones: 0,
        notes: "Unlocked locally by validate --apply; upstream the report to get a named entry."
            .to_string(),
    }
}

/// Local unlock written by `validate --apply` after all-MATCH.
/// Proves THIS machine (exact DMI), so no app update is needed for a
/// new Pongo — the table stays for names/notes only.
pub const ATTEST_PATH: &str = "/var/lib/axiood/validated";

fn kv_get(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

fn attested_at(path: &str, dmi: &BTreeMap<String, String>) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    if kv_get(&text, "v").as_deref() != Some("1") || kv_get(&text, "ok").as_deref() != Some("1") {
        return false;
    }
    for key in ["board", "product", "sku"] {
        let want = kv_get(&text, key).unwrap_or_default();
        let got = match key {
            "board" => dmi.get("board_name"),
            "product" => dmi.get("product_name"),
            _ => dmi.get("product_sku"),
        };
        if want != got.map_or(String::new(), |s| s.clone()) {
            return false;
        }
    }
    kv_get(&text, "checks").is_some_and(|c| !c.is_empty() && !c.contains("MISMATCH"))
}

/// This machine passed local validation before (exact DMI match).
pub fn attested(dmi: &BTreeMap<String, String>) -> bool {
    attested_at(ATTEST_PATH, dmi)
}

/// Record a passing validation. Needs root (`/var/lib/axiood`).
/// `facts` holds measured machine facts (zones, stock RAPL, EC sample)
/// for transparency — same file, still local-only, still no UUID.
pub fn write_attestation(
    dmi: &BTreeMap<String, String>,
    checks: &str,
    facts: &str,
) -> std::io::Result<()> {
    let clean = |k: &str| {
        dmi.get(k)
            .map_or(String::new(), |s| s.replace(['\n', '='], " "))
            .trim()
            .to_string()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    let text = format!(
        "v=1\nok=1\nboard={}\nproduct={}\nsku={}\ndate={now}\nchecks={checks}\n{facts}",
        clean("board_name"),
        clean("product_name"),
        clean("product_sku"),
    );
    if let Some(parent) = std::path::Path::new(ATTEST_PATH).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(ATTEST_PATH, text)
}

/// Measured facts from a previous `--apply` (empty when never validated).
/// Local-only, root-owned file; unknown keys ignored by [`attested`].
pub fn attested_facts() -> BTreeMap<String, String> {
    match std::fs::read_to_string(ATTEST_PATH) {
        Ok(text) => facts_from(&text),
        Err(_) => BTreeMap::new(),
    }
}

fn facts_from(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') {
            let k = k.trim();
            if !matches!(
                k,
                "v" | "ok" | "board" | "product" | "sku" | "date" | "checks"
            ) {
                out.insert(k.to_string(), v.trim().to_string());
            }
        }
    }
    out
}

fn upper(s: &str) -> String {
    s.to_uppercase()
}

fn matches_any(hay: &str, needles: &[String]) -> bool {
    let h = upper(hay);
    needles.iter().any(|n| h.contains(&n.to_uppercase()))
}

/// Override chain (first existing file wins): env, /etc, /usr/share.
fn candidates() -> Vec<String> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("AXIOO_DEVICES") {
        if !p.is_empty() {
            v.push(p);
        }
    }
    v.push("/etc/axioo-control-center/devices.toml".to_string());
    v.push("/usr/share/axioo-control-center/devices.toml".to_string());
    v
}

fn parse_db(text: &str) -> Result<Vec<DeviceProfile>, String> {
    toml::from_str::<DbFile>(text)
        .map(|db| db.device)
        .map_err(|e| e.to_string())
}

static DB: OnceLock<(Vec<DeviceProfile>, String, Option<String>)> = OnceLock::new();

fn load_from(paths: &[String], embedded: &str) -> (Vec<DeviceProfile>, String, Option<String>) {
    let mut warn = None;
    for p in paths {
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        match parse_db(&text) {
            Ok(table) => return (table, format!("file:{p}"), None),
            Err(e) => {
                warn = Some(format!("{p}: {e}; using embedded table"));
            }
        }
    }
    match parse_db(embedded) {
        Ok(table) => (table, "embedded".to_string(), warn),
        Err(e) => (
            Vec::new(),
            "embedded".to_string(),
            Some(format!("embedded table broken: {e}")),
        ),
    }
}

fn load() -> &'static (Vec<DeviceProfile>, String, Option<String>) {
    DB.get_or_init(|| load_from(&candidates(), include_str!("../devices.toml")))
}

/// Named table (override-aware). Empty only if the embedded table itself
/// fails to parse (a bug — surfaced via [`table_warning`]).
pub fn table() -> &'static [DeviceProfile] {
    &load().0
}

/// Where the active table came from (`file:<path>` or `embedded`).
pub fn table_source() -> &'static str {
    &load().1
}

/// Parse warning, if an override file existed but was rejected.
pub fn table_warning() -> Option<&'static str> {
    load().2.as_deref()
}

pub fn match_dmi(dmi: &BTreeMap<String, String>) -> DeviceProfile {
    match_in(dmi, table())
}

/// Pure table match (no I/O, no attestation) — unit-test seam.
pub fn match_in(dmi: &BTreeMap<String, String>, table: &[DeviceProfile]) -> DeviceProfile {
    let board = dmi.get("board_name").map_or("", String::as_str);
    let product = dmi.get("product_name").map_or("", String::as_str);
    let sku = dmi.get("product_sku").map_or("", String::as_str);
    let product_blob = format!("{product} {sku}");

    for p in table {
        if matches_any(board, &p.board_match) || matches_any(&product_blob, &p.product_match) {
            return p.clone();
        }
    }

    // Generic Axioo/Clevo fallback.
    let vendor = dmi.get("sys_vendor").map_or("", String::as_str);
    let board_vendor = dmi.get("board_vendor").map_or("", String::as_str);
    let blob = format!("{vendor} {board_vendor} {board} {product} {sku}");
    let b = upper(&blob);
    if b.contains("AXIOO") || b.contains("CLEVO") || b.contains("PONGO") {
        return unknown_clevo();
    }
    foreign()
}

fn resolve(dmi: &BTreeMap<String, String>) -> DeviceProfile {
    let p = match_dmi(dmi);
    if !p.fan_write_allowed && attested(dmi) {
        return attested_profile();
    }
    p
}

pub fn current() -> DeviceProfile {
    resolve(&crate::dmi::read_dmi())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dmi(board: &str, product: &str, vendor: &str) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("board_name".to_string(), board.to_string()),
            ("product_name".to_string(), product.to_string()),
            ("sys_vendor".to_string(), vendor.to_string()),
        ])
    }

    fn test_table() -> Vec<DeviceProfile> {
        parse_db(include_str!("../devices.toml")).expect("embedded table parses")
    }

    fn entry<'a>(table: &'a [DeviceProfile], id: &str) -> &'a DeviceProfile {
        table.iter().find(|p| p.id == id).expect("entry present")
    }

    #[test]
    fn embedded_table_parses_with_all_entries() {
        let t = test_table();
        assert!(t.len() >= 11);
        assert!(entry(&t, "x560wnr-su9").fan_write_allowed);
        assert!(!entry(&t, "pongo725").fan_write_allowed);
    }

    #[test]
    fn sibling_beats_prefix_in_table_order() {
        let t = test_table();
        let p = match_in(&dmi("X560WNR1-G", "NP9561R", "Sager"), &t);
        assert_eq!(p.id, "x560wnr-sibling");
        assert_eq!(p.grade, Grade::Sibling);
        assert!(p.fan_write_allowed);
        let s = match_in(&dmi("X560WNR-SU9", "Pongo Studio X", "Axioo"), &t);
        assert_eq!(s.id, "x560wnr-su9");
        assert_eq!(s.grade, Grade::Supported);
    }

    #[test]
    fn unknown_axioo_is_read_only() {
        let p = match_in(&dmi("Pongo Board", "Pongo Future", "Axioo"), &test_table());
        assert_eq!(p.id, "unknown-clevo");
        assert_eq!(p.grade, Grade::UnknownClevo);
        assert!(!p.fan_write_allowed);
    }

    #[test]
    fn all_named_pongos_resolve_and_stay_locked() {
        let t = test_table();
        for (product, id) in [
            ("Pongo 535", "pongo535"),
            ("Pongo 725", "pongo725"),
            ("Pongo 725 V2", "pongo725"),
            ("Pongo 735", "pongo735"),
            ("Pongo 750", "pongo750"),
            ("Pongo 755", "pongo755"),
            ("Pongo 755 V2", "pongo755"),
            ("Pongo 760 V2", "pongo760"),
            ("Pongo 765", "pongo765"),
            ("Pongo 765V2", "pongo765"),
            ("Pongo 775", "pongo775"),
            ("Pongo 960", "pongo960"),
        ] {
            let p = match_in(&dmi("Unknown Board", product, "Axioo"), &t);
            assert_eq!(p.id, id, "product {product}");
            assert_eq!(p.grade, Grade::UnknownClevo, "product {product}");
            assert!(!p.fan_write_allowed, "product {product}");
        }
        let by_board = match_in(&dmi("NP50RNA", "Pongo", "Axioo"), &t);
        assert_eq!(by_board.id, "pongo725");
        assert_eq!(by_board.kbd_zones, 1);
    }

    #[test]
    fn display_name_prefers_override_then_dmi_then_id() {
        let t = test_table();
        let d = dmi("X560WNR-SU9", "Pongo Studio X", "Axioo");
        assert_eq!(
            entry(&t, "x560wnr-su9").display_name(&d),
            "Axioo Pongo Studio X 2025 (Clevo X560WNR-SU9)"
        );
        let p = match_in(&dmi("Unknown Board", "Pongo 735", "Axioo"), &t);
        assert_eq!(
            p.display_name(&dmi("Unknown Board", "Pongo 735", "Axioo")),
            "Axioo Pongo 735"
        );
        assert_eq!(foreign().display_name(&BTreeMap::new()), "foreign");
    }

    #[test]
    fn fan_write_decision_matrix() {
        let t = test_table();
        assert_eq!(entry(&t, "x560wnr-su9").fan_write(), FanWrite::Allowed);
        assert_eq!(
            entry(&t, "x560wnr-sibling").fan_write(),
            FanWrite::AllowedWithWarning
        );
        assert_eq!(entry(&t, "pongo725").fan_write(), FanWrite::Locked);
        assert_eq!(attested_profile().fan_write(), FanWrite::Allowed);
        assert_eq!(foreign().fan_write(), FanWrite::Locked);
    }

    #[test]
    fn bad_grade_rejected() {
        let bad = "[[device]]\nid = \"x\"\nproduct_match = [\"X\"]\ngrade = \"super\"\n";
        assert!(parse_db(bad).is_err());
    }

    #[test]
    fn override_file_wins_over_embedded() {
        let dir = std::env::temp_dir().join(format!("axioo-db-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.toml").to_string_lossy().into_owned();
        std::fs::write(
            &path,
            "[[device]]\nid = \"custom\"\nproduct_match = [\"Pongo 735\"]\ngrade = \"sibling\"\nfan_write_allowed = true\n",
        )
        .unwrap();
        let expected = format!("file:{path}");
        let paths = [path];
        let (table, source, warn) = { load_from(&paths, include_str!("../devices.toml")) };
        assert_eq!(source, expected);
        assert!(warn.is_none());
        let p = match_in(&dmi("Unknown Board", "Pongo 735", "Axioo"), &table);
        assert_eq!(p.id, "custom");
        assert_eq!(p.grade, Grade::Sibling);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn broken_override_falls_back_with_warning() {
        let dir = std::env::temp_dir().join(format!("axioo-db-test-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("devices.toml").to_string_lossy().into_owned();
        std::fs::write(&path, "not = [valid").unwrap();
        let (table, source, warn) = load_from(
            &[path, "/nonexistent".to_string()],
            include_str!("../devices.toml"),
        );
        assert_eq!(source, "embedded");
        assert!(warn.is_some());
        assert!(!table.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn foreign_hardware_is_read_only() {
        let p = match_in(&dmi("440FX", "VMware", "VMware, Inc."), &test_table());
        assert_eq!(p.grade, Grade::Foreign);
        assert!(!p.fan_write_allowed);
    }

    #[test]
    fn attestation_unlocks_matching_machine_only() {
        let dir = std::env::temp_dir().join(format!("axioo-attest-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("validated").to_string_lossy().into_owned();
        let good = "v=1\nok=1\nboard=B\nproduct=P\nsku=S\ndate=1\nchecks=a:MATCH\n";
        std::fs::write(&path, good).unwrap();
        let d = BTreeMap::from([
            ("board_name".to_string(), "B".to_string()),
            ("product_name".to_string(), "P".to_string()),
            ("product_sku".to_string(), "S".to_string()),
        ]);
        assert!(attested_at(&path, &d));
        let other = BTreeMap::from([
            ("board_name".to_string(), "OTHER".to_string()),
            ("product_name".to_string(), "P".to_string()),
            ("product_sku".to_string(), "S".to_string()),
        ]);
        assert!(!attested_at(&path, &other));
        std::fs::write(
            &path,
            "v=1\nok=1\nboard=B\nproduct=P\nsku=S\nchecks=a:MISMATCH\n",
        )
        .unwrap();
        assert!(!attested_at(&path, &d));
        std::fs::write(
            &path,
            "v=1\nok=0\nboard=B\nproduct=P\nsku=S\nchecks=a:MATCH\n",
        )
        .unwrap();
        assert!(!attested_at(&path, &d));
        assert!(!attested_at("/nonexistent-axioo-test-path", &d));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attested_facts_exposes_only_fact_keys() {
        let out = facts_from(
            "v=1\nok=1\nboard=B\nproduct=P\nsku=S\nchecks=a:MATCH\nzones=5\nstock_rapl=pkg:44W\n",
        );
        assert_eq!(out.get("zones").map(String::as_str), Some("5"));
        assert_eq!(out.get("stock_rapl").map(String::as_str), Some("pkg:44W"));
        assert!(!out.contains_key("board"));
        assert!(!out.contains_key("checks"));
        assert!(facts_from("").is_empty());
    }

    #[test]
    fn empty_dmi_is_foreign() {
        let p = match_in(&BTreeMap::new(), &test_table());
        assert_eq!(p.grade, Grade::Foreign);
    }
}
