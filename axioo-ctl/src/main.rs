mod fan;
mod kbd;
mod monitor;
mod probe;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "axioo-ctl",
    version,
    about = "Linux control utility for Axioo laptops (read-only MVP)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dump hardware capabilities: DMI, sensors, RAPL, NVIDIA, LEDs, ACPI/WMI, EC.
    Probe,
    /// Live monitor: temps, clocks, power, fans, GPU, battery.
    Monitor {
        /// Refresh interval in seconds.
        #[arg(short, long, default_value_t = 1.0)]
        interval: f64,
        /// Stop after N refreshes (default: run until Ctrl-C).
        #[arg(short, long)]
        count: Option<u64>,
    },
    /// Fan inspection (read-only): EC dump + curve preview. No EC writes.
    Fan {
        #[command(subcommand)]
        cmd: FanCmd,
    },
    /// Keyboard backlight: status / get / set (needs kbd LED driver).
    Kbd {
        #[command(subcommand)]
        cmd: KbdCmd,
    },
}

#[derive(Subcommand)]
enum FanCmd {
    /// Dump EC fan registers + cross-check against hwmon (needs root + ec_sys).
    Dump,
    /// Poll the EC fan map over time (read-only log for idle-vs-load).
    Watch {
        /// Refresh interval in seconds.
        #[arg(short, long, default_value_t = 1.0)]
        interval: f64,
        /// Stop after N samples (default: run until Ctrl-C).
        #[arg(short, long)]
        count: Option<u64>,
    },
    /// Preview the reference auto-curve step (pure, no hardware I/O).
    Curve {
        /// Max(CPU, GPU) temperature in °C.
        #[arg(long)]
        temp: i32,
        /// Current duty in percent.
        #[arg(long)]
        duty: u8,
    },
    /// One-shot manual duty on both fans (needs root — via pkexec/sudo).
    Set {
        /// Duty in percent (clamped to safe 40–100%).
        pct: u8,
    },
    /// Restore EC auto control on both fans (needs root — via pkexec/sudo).
    Auto,
    /// No-op root probe for GUI pre-auth at startup (never touches EC).
    Ping,
}

#[derive(Subcommand)]
enum KbdCmd {
    /// Show driver/LED presence and current state, with fix hints.
    Status,
    /// Print current brightness + RGB.
    Get,
    /// Set brightness and/or color.
    Set {
        /// Raw brightness value (clamped to the driver's max_brightness).
        #[arg(short, long)]
        brightness: Option<u32>,
        /// Color as R,G,B (0-255 each), e.g. 255,0,0.
        #[arg(long)]
        rgb: Option<String>,
        /// Named preset: red|yellow|green|cyan|blue|white|off.
        #[arg(long)]
        preset: Option<String>,
        /// Print what would be written without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Probe => probe::run(),
        Cmd::Monitor { interval, count } => monitor::run(interval, count),
        Cmd::Fan { cmd } => match cmd {
            FanCmd::Dump => fan::dump(),
            FanCmd::Watch { interval, count } => fan::watch(interval, count),
            FanCmd::Curve { temp, duty } => fan::curve(temp, duty),
            FanCmd::Set { pct } => fan::set(pct),
            FanCmd::Auto => fan::auto(),
            FanCmd::Ping => fan::ping(),
        },
        Cmd::Kbd { cmd } => match cmd {
            KbdCmd::Status => kbd::status(),
            KbdCmd::Get => kbd::get(),
            KbdCmd::Set { brightness, rgb, preset, dry_run } => {
                kbd::set(brightness, rgb, preset, dry_run)
            }
        },
    }
}
