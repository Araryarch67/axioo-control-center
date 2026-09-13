//! DMI / SMBIOS identification (who manufactured this machine).

use std::collections::BTreeMap;

use crate::read_trim_str;

const DMI_BASE: &str = "/sys/class/dmi/id";

const KEYS: &[&str] = &[
    "sys_vendor",
    "product_name",
    "product_version",
    "product_sku",
    "product_uuid",
    "board_vendor",
    "board_name",
    "board_version",
    "bios_vendor",
    "bios_version",
    "bios_date",
    "chassis_vendor",
    "chassis_type",
];

/// All readable DMI fields, keyed by field name.
pub fn read_dmi() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for key in KEYS {
        let path = format!("{DMI_BASE}/{key}");
        if let Some(v) = read_trim_str(&path) {
            if !v.is_empty() {
                out.insert(key.to_string(), v);
            }
        }
    }
    out
}
