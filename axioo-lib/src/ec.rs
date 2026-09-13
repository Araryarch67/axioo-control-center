//! Embedded Controller access *availability* probe + read-only map dump.
//!
//! MVP rule: report + read only. Opening `/sys/kernel/debug/ec/ec0/io`
//! for writes is out of scope until the exact EC map of this model is
//! validated (see `docs/ec-fan-protocol.md` section F). Everything here
//! opens the io file with read-only access and never writes.

use std::fmt;
use std::fs;
use std::io::{self, Read};

use crate::read_trim_str;

/// Debugfs path exposing the 256 EC registers (needs `modprobe ec_sys` + root).
pub const EC_IO_PATH: &str = "/sys/kernel/debug/ec/ec0/io";
/// Size of the EC register map exposed by `ec_sys`.
pub const EC_MAP_LEN: usize = 256;

/// Is the `ec_sys` kernel module available for this kernel?
pub fn ec_sys_available() -> bool {
    let release = read_trim_str("/proc/sys/kernel/osrelease").unwrap_or_default();
    let pattern = format!("/lib/modules/{release}/kernel/drivers/acpi/ec_sys.ko");
    [&format!("{pattern}.zst"), &format!("{pattern}.xz"), &pattern]
        .iter()
        .any(|p| fs::metadata(p).is_ok())
}

/// Value of the `ec_sys.write_support` module parameter when loaded.
pub fn ec_write_support() -> Option<String> {
    read_trim_str("/sys/module/ec_sys/parameters/write_support")
}

/// Can we open the EC io map (needs root + debugfs)? Read-only check.
pub fn ec_io_readable() -> bool {
    fs::File::open(EC_IO_PATH).is_ok()
}

/// Read-only failure modes for [`read_map`], with actionable explanations.
#[derive(Debug)]
pub enum EcReadError {
    /// `ec_sys` not loaded or debugfs not mounted: the io file is absent.
    NotPresent { detail: String },
    /// Needs root (or debugfs mounted with access).
    Permission { detail: String },
    /// File shorter than 256 bytes: unexpected kernel layout.
    ShortRead { got: usize },
    /// Any other I/O failure.
    Io(io::Error),
}

impl fmt::Display for EcReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EcReadError::NotPresent { detail } => write!(
                f,
                "EC io map not present ({detail}). Try: sudo modprobe ec_sys && \
                 ls /sys/kernel/debug/ec/ec0/io — see docs/ec-fan-protocol.md section F"
            ),
            EcReadError::Permission { detail } => write!(
                f,
                "permission denied reading EC io map ({detail}). Re-run with sudo; \
                 reads via ec_sys need root + debugfs"
            ),
            EcReadError::ShortRead { got } => {
                write!(f, "EC io map too short: got {got} bytes, expected 256")
            }
            EcReadError::Io(e) => write!(f, "EC io read failed: {e}"),
        }
    }
}

impl std::error::Error for EcReadError {}

/// Read the full 256-byte EC register map via `ec_sys` (read-only).
/// Never writes; the file is opened read-only and read once.
pub fn read_map() -> Result<[u8; EC_MAP_LEN], EcReadError> {
    let file = fs::File::open(EC_IO_PATH).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => EcReadError::NotPresent { detail: e.to_string() },
        io::ErrorKind::PermissionDenied => EcReadError::Permission { detail: e.to_string() },
        _ => EcReadError::Io(e),
    })?;
    let mut buf = Vec::with_capacity(EC_MAP_LEN);
    file.take(EC_MAP_LEN as u64).read_to_end(&mut buf).map_err(|e| {
        if e.kind() == io::ErrorKind::PermissionDenied {
            EcReadError::Permission { detail: e.to_string() }
        } else {
            EcReadError::Io(e)
        }
    })?;
    if buf.len() < EC_MAP_LEN {
        return Err(EcReadError::ShortRead { got: buf.len() });
    }
    let mut map = [0u8; EC_MAP_LEN];
    map.copy_from_slice(&buf[..EC_MAP_LEN]);
    Ok(map)
}

/// Read a single EC register (convenience over [`read_map`]).
pub fn read_byte(reg: u8) -> Result<u8, EcReadError> {
    Ok(read_map()?[reg as usize])
}

/// Clevo/TUXEDO/ITE kernel modules currently loaded (`/proc/modules`).
pub fn loaded_vendor_modules() -> Vec<String> {
    let mut out = Vec::new();
    let text = read_trim_str("/proc/modules").unwrap_or_default();
    for line in text.lines() {
        let name = line.split_whitespace().next().unwrap_or("");
        let n = name.to_lowercase();
        if n.contains("clevo") || n.contains("tuxedo") || n.contains("ite_") || n == "ec_sys" {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}
