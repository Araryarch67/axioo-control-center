//! `axiood` — privileged daemon: EC fan-curve loop + RAPL, two-way PPD sync.
//!
//! - Sumber kebenaran saat boot: PPD `ActiveProfile`.
//! - GUI/CLI set via `com.axioo.Control.SetProfile` / `SetQuietFan` → daemon
//!   set PPD balik + apply RAPL + ganti kurva kipas.
//! - EPP/governor TIDAK disentuh (milik PPD).
//! - `quiet_fan` murni opsi kipas per mode (tak sentuh PPD/RAPL).

mod ppd;
mod profile;
mod rapl_apply;
mod service;
mod state;

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use axioo_lib::{cpu, ec, fan, fan_ctrl};
use profile::AxiooProfile;
use service::{AxiooControl, AXIOO_PATH, AXIOO_SERVICE};
use state::DaemonState;

fn args() -> (f64, Option<String>, bool, bool, bool) {
    let mut interval = 2.0f64;
    let mut profile: Option<String> = None;
    let mut no_fan = false;
    let mut no_rapl = false;
    let mut no_ppd = false;
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--interval" | "-i" => {
                interval = it.next().and_then(|v| v.parse().ok()).unwrap_or(2.0);
            }
            "--profile" | "-p" => profile = it.next(),
            "--no-fan" => no_fan = true,
            "--no-rapl" => no_rapl = true,
            "--no-ppd" => no_ppd = true,
            "--help" | "-h" => {
                println!(
                    "axiood — Axioo privileged daemon\n\n\
                     Usage: axiood [--interval SECS] [--profile NAME] [--no-fan] [--no-rapl] [--no-ppd]\n\n\
                     NAME: Balanced|Entertainment|Performance (legacy Quiet = Balanced+quiet-fan).\n\
                     Two-way sync with power-profiles-daemon.\n\
                     Requires root to write EC + RAPL."
                );
                std::process::exit(0);
            }
            _ => eprintln!("axiood: unknown arg '{a}' (see --help)"),
        }
    }
    (interval, profile, no_fan, no_rapl, no_ppd)
}

/// Suhu max(CPU,GPU): EC `0x07`/`0xCD` bila ada, fallback coretemp.
fn current_temp_c() -> Option<i32> {
    if let Ok(map) = ec::read_map() {
        let snap = fan::snapshot(&map);
        return Some(snap.max_temp_c());
    }
    let s = cpu::sample();
    s.package_temp_c
        .or(s.max_core_temp_c)
        .map(|t| t.round() as i32)
}

fn apply_rapl(profile: AxiooProfile, no_rapl: bool) {
    if no_rapl {
        return;
    }
    let (pl1, pl2) = profile.rapl_limits_w();
    match rapl_apply::apply_pl1_pl2(pl1, pl2) {
        Ok(paths) => println!(
            "axiood: RAPL {} -> PL1 {pl1}W PL2 {pl2}W ({})",
            profile.as_str(),
            paths.join(", ")
        ),
        Err(e) => eprintln!("axiood: RAPL apply failed ({e}) — continuing with fan only"),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (interval_s, profile_override, no_fan, no_rapl, no_ppd) = args();
    let interval = Duration::from_secs_f64(interval_s.clamp(0.5, 30.0));

    if !fan_ctrl::is_root() {
        eprintln!("axiood: requires root (EC + RAPL writes). Run via systemd or sudo.");
        // Tetap jalan dalam mode baca-saja bila dipaksa? Tidak — keluar agar
        // systemd me-restart dengan konteks yang benar.
        std::process::exit(1);
    }

    let conn = zbus::Connection::system().await?;
    let flavor = ppd::detect_flavor(&conn).await;
    let (ppd_current, _) = ppd::get_active_profile(&conn)
        .await
        .unwrap_or(("balanced".to_string(), flavor));
    println!("axiood: initial PPD '{ppd_current}' (flavor {flavor:?})");

    let (initial, initial_quiet) = match profile_override.as_deref() {
        Some(p) if p.eq_ignore_ascii_case("quiet") || p.eq_ignore_ascii_case("power-saver") => {
            (AxiooProfile::Balanced, true)
        }
        Some(p) => match AxiooProfile::parse(p) {
            Some(prof) => (prof, false),
            None => {
                eprintln!("axiood: --profile '{p}' unknown, using PPD mapping result");
                AxiooProfile::from_ppd(&ppd_current, AxiooProfile::Balanced, false)
            }
        },
        None => AxiooProfile::from_ppd(&ppd_current, AxiooProfile::Balanced, false),
    };

    let state = Arc::new(RwLock::new(DaemonState::new(
        initial,
        initial_quiet,
        ppd_current.clone(),
    )));
    println!("axiood: initial profile {}", state.read().await.label());

    // Serve com.axioo.Control
    let svc = AxiooControl {
        state: state.clone(),
    };
    conn.object_server().at(AXIOO_PATH, svc).await?;
    conn.request_name(AXIOO_SERVICE).await?;
    println!("axiood: D-Bus {AXIOO_SERVICE} @ {AXIOO_PATH}");

    // Apply awal: RAPL + selaraskan PPD bila override berbeda.
    {
        let st = state.read().await;
        apply_rapl(st.profile, no_rapl);
        if !no_ppd {
            let want = st.profile.ppd_profile();
            if want != ppd_current {
                match ppd::set_active_profile(&conn, flavor, want).await {
                    Ok(()) => println!("axiood: PPD synced -> '{want}'"),
                    Err(e) => eprintln!("axiood: failed to set PPD ({e})"),
                }
            }
        }
    }
    state.write().await.pending_apply = false;

    let mut tick = tokio::time::interval(interval);
    let mut ppd_tick = tokio::time::interval(Duration::from_secs(3));
    ppd_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // `true` = tick sebelumnya sudah menyerahkan kipas ke firmware
    // (set_auto satu-kali). Mencegah set_auto berulang tiap tick.
    let mut ec_auto_applied = false;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // 1. Terapkan perubahan dari D-Bus SetProfile/SetQuietFan.
                let (profile, quiet, do_apply) = {
                    let mut st = state.write().await;
                    let a = st.pending_apply;
                    st.pending_apply = false;
                    (st.profile, st.quiet_fan, a)
                };
                if do_apply {
                    apply_rapl(profile, no_rapl);
                    if !no_ppd {
                        let want = profile.ppd_profile();
                        let cur = state.read().await.ppd_profile.clone();
                        if want != cur {
                            match ppd::set_active_profile(&conn, flavor, want).await {
                                Ok(()) => {
                                    println!("axiood: PPD -> '{want}' (from {})", profile.as_str());
                                    state.write().await.ppd_profile = want.to_string();
                                }
                                Err(e) => eprintln!("axiood: failed to set PPD ({e})"),
                            }
                        }
                    }
                }
                // 2. Fan tick: tiga mode milik daemon (tak pernah dari klien langsung).
                //    - ec_auto : set_auto() SATU-KALI, lalu diam (firmware pegang).
                //    - manual  : kunci duty pilihan user.
                //    - curve   : EC-auto step (+hysteresis), quiet = pin 40.
                if !no_fan {
                    let (ec_auto, manual) = {
                        let st = state.read().await;
                        (st.fan_ec_auto, st.fan_manual)
                    };
                    if ec_auto {
                        if !ec_auto_applied {
                            match fan_ctrl::set_auto() {
                                Ok(()) => {
                                    println!("axiood: fan -> EC auto (firmware in charge)");
                                    state.write().await.last_duty = 0;
                                }
                                Err(e) => eprintln!("axiood: set EC auto failed ({e})"),
                            }
                            ec_auto_applied = true;
                        }
                    } else {
                        ec_auto_applied = false;
                        if let Some(temp) = current_temp_c() {
                            let last = state.read().await.last_duty;
                            let duty = match manual {
                                Some(d) => d,
                                None => profile.step_duty(quiet, temp, last),
                            };
                            if duty != last {
                                match fan_ctrl::set_manual_duty(duty) {
                                    Ok(rep) => {
                                        state.write().await.last_duty = duty;
                                        let tag = if manual.is_some() {
                                            "manual"
                                        } else if quiet {
                                            "quiet"
                                        } else {
                                            "normal"
                                        };
                                        println!(
                                            "axiood: {} [{tag}] {temp}C -> {duty}% (verify {:?})",
                                            profile.as_str(), rep.verified_pct
                                        );
                                    }
                                    Err(e) => eprintln!("axiood: EC write failed ({e})"),
                                }
                            }
                        } else {
                            eprintln!("axiood: temp unreadable (EC + coretemp failed)");
                        }
                    }
                }
            }
            _ = ppd_tick.tick(), if !no_ppd => {
                // 3. Follow PPD (polling 3 dtk; cukup responsif untuk slider GNOME).
                match ppd::get_active_profile(&conn).await {
                    Ok((cur, _)) => {
                        let known = state.read().await.ppd_profile.clone();
                        if cur != known {
                            println!("axiood: PPD berubah '{known}' -> '{cur}'");
                            let (mapped, mapped_quiet) = {
                                let st = state.read().await;
                                AxiooProfile::from_ppd(&cur, st.profile, st.quiet_fan)
                            };
                            {
                                let mut st = state.write().await;
                                st.ppd_profile = cur.clone();
                                if mapped != st.profile || mapped_quiet != st.quiet_fan {
                                    println!(
                                        "axiood: profil {} -> {}",
                                        st.label(),
                                        if mapped_quiet {
                                            format!("{} (quiet-fan)", mapped.as_str())
                                        } else {
                                            mapped.as_str().to_string()
                                        }
                                    );
                                    st.profile = mapped;
                                    st.quiet_fan = mapped_quiet;
                                    st.pending_apply = true;
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("axiood: PPD read failed ({e})"),
                }
            }
        }
    }
}
