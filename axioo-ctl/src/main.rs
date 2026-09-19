mod battery;
mod completion;
mod config;
mod fan;
mod gpu;
mod kbd;
mod monitor;
mod probe;
mod profile;
mod validate;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "axioo-ctl",
    version,
    about = "Linux control utility for Axioo laptops (Clevo-based)"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Dump hardware capabilities: DMI, sensors, RAPL, NVIDIA, LEDs, ACPI/WMI, EC.
    Probe {
        /// Machine-readable JSON on stdout (same data, for scripts/issues).
        #[arg(long)]
        json: bool,
    },
    /// Live monitor: temps, clocks, power, fans, GPU, battery.
    Monitor {
        /// Refresh interval in seconds.
        #[arg(short, long, default_value_t = 1.0)]
        interval: f64,
        /// Stop after N refreshes (default: run until Ctrl-C).
        #[arg(short, long)]
        count: Option<u64>,
    },
    /// Fan: EC dump + curve preview + one-shot manual duty / restore EC-auto (writes need root).
    Fan {
        #[command(subcommand)]
        cmd: FanCmd,
    },
    /// Power profile via axiood (two-way sync with PPD).
    Profile {
        #[command(subcommand)]
        cmd: ProfileCmd,
    },
    /// Keyboard backlight: status / get / set (needs kbd LED driver).
    Kbd {
        #[command(subcommand)]
        cmd: KbdCmd,
    },
    /// Battery: status / get / set thresholds (FlexiCharger via charge_control_*).
    Battery {
        #[command(subcommand)]
        cmd: BatteryCmd,
    },
    /// dGPU: power state (RTD3-aware) + processes holding it (read-only).
    Gpu {
        #[command(subcommand)]
        cmd: GpuCmd,
    },
    /// Print shell completions to stdout (bash|fish|zsh|powershell|elvish).
    Completion {
        /// Shell name.
        shell: String,
    },
    /// Backup/restore all tunables as one JSON file (profile, fan, kbd, battery).
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Guided self-validation for a new model (EC read-only + kbd walk + scrubbed report).
    Validate {
        /// CPU load seconds for the hot sample (0 = skip load phase).
        #[arg(long, default_value_t = 20)]
        load_secs: u64,
        /// Skip the interactive keyboard zone walk.
        #[arg(long)]
        skip_kbd: bool,
        /// Write JSON report to file instead of stdout.
        #[arg(long)]
        file: Option<String>,
        /// Unlock this machine locally when all checks MATCH
        /// (writes /var/lib/axiood/validated, needs root; no app update).
        #[arg(long)]
        apply: bool,
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
enum ProfileCmd {
    /// Show current axioo profile (+ quiet-fan + PPD).
    Get,
    /// Set profile: Balanced|Entertainment|Performance.
    Set {
        /// Profile name.
        name: String,
    },
    /// Per-mode quiet-fan toggle: on|off|toggle|status.
    QuietFan {
        /// Action.
        action: String,
    },
}

#[derive(Subcommand)]
enum KbdCmd {
    /// Show driver/LED presence and current state, with fix hints.
    Status,
    /// Print current brightness + RGB.
    Get,
    /// Raise brightness one step (for Fn-key bindings).
    Brighter,
    /// Lower brightness one step (for Fn-key bindings).
    Dimmer,
    /// Re-apply last saved color from /var/lib/axiood/kbd.json (udev +
    /// manual; earliest restore point, long before SDDM).
    Restore,
    /// Set brightness and/or color (mode static).
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
    /// RGB effect: static|breathing|wave|rainbow|cycle|aurora|twinkle|pulse|gradient|music|spectrum|reactive (Ctrl-C to stop).
    Effect {
        /// Effect name.
        name: String,
        /// Base color R,G,B (for breathing/static).
        #[arg(long)]
        rgb: Option<String>,
        /// Preset color (alt to --rgb).
        #[arg(long)]
        preset: Option<String>,
        /// Independent rear-exhaust animation (needs 5 nodes); "follow" = follow main.
        #[arg(long)]
        rear: Option<String>,
    },
}

#[derive(Subcommand)]
enum BatteryCmd {
    /// Show battery + thresholds + available steps.
    Status,
    /// Print current thresholds (start/end).
    Get,
    /// Set thresholds (requires root). Validated against the firmware available list.
    Set {
        /// Start threshold (charging starts when below).
        #[arg(long)]
        start: Option<u64>,
        /// End threshold (charging stops when above).
        #[arg(long)]
        end: Option<u64>,
    },
}

#[derive(Subcommand)]
enum GpuCmd {
    /// dGPU power state + holders (never wakes a suspended GPU to ask).
    Status,
}

#[derive(Subcommand)]
enum ConfigCmd {
    /// Export current settings as JSON (stdout or --file).
    Export {
        /// Write to file instead of stdout.
        #[arg(long)]
        file: Option<String>,
    },
    /// Apply settings from JSON (--file or stdin).
    Import {
        /// Read from file instead of stdin.
        #[arg(long)]
        file: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Probe { json } => {
            if json {
                probe::run_json();
            } else {
                probe::run();
            }
        }
        Cmd::Monitor { interval, count } => monitor::run(interval, count),
        Cmd::Fan { cmd } => match cmd {
            FanCmd::Dump => fan::dump(),
            FanCmd::Watch { interval, count } => fan::watch(interval, count),
            FanCmd::Curve { temp, duty } => fan::curve(temp, duty),
            FanCmd::Set { pct } => fan::set(pct),
            FanCmd::Auto => fan::auto(),
            FanCmd::Ping => fan::ping(),
        },
        Cmd::Profile { cmd } => match cmd {
            ProfileCmd::Get => profile::get(),
            ProfileCmd::Set { name } => profile::set(&name),
            ProfileCmd::QuietFan { action } => profile::quiet_fan(&action),
        },
        Cmd::Kbd { cmd } => match cmd {
            KbdCmd::Status => kbd::status(),
            KbdCmd::Get => kbd::get(),
            KbdCmd::Brighter => kbd::brighter(),
            KbdCmd::Dimmer => kbd::dimmer(),
            KbdCmd::Restore => kbd::restore(),
            KbdCmd::Set {
                brightness,
                rgb,
                preset,
                dry_run,
            } => kbd::set(brightness, rgb, preset, dry_run),
            KbdCmd::Effect {
                name,
                rgb,
                preset,
                rear,
            } => kbd::effect(&name, rgb, preset, rear),
        },
        Cmd::Battery { cmd } => match cmd {
            BatteryCmd::Status => battery::status(),
            BatteryCmd::Get => battery::get(),
            BatteryCmd::Set { start, end } => battery::set(start, end),
        },
        Cmd::Gpu { cmd } => match cmd {
            GpuCmd::Status => gpu::status(),
        },
        Cmd::Completion { shell } => completion::run(&shell),
        Cmd::Config { cmd } => match cmd {
            ConfigCmd::Export { file } => config::export(file),
            ConfigCmd::Import { file } => config::import(file),
        },
        Cmd::Validate {
            load_secs,
            skip_kbd,
            file,
            apply,
        } => validate::run(load_secs, skip_kbd, file, apply),
    }
}
