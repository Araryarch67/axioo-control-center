//! axioo-control-center: GPUI desktop frontend with live sensors.
//!
//! A background thread samples hardware through `axioo-lib` once per
//! second (read-only) and sends snapshots over an `mpsc` channel.
//! The UI thread polls the channel from a foreground task
//! (`cx.spawn` + `Timer::after`) and updates the view. Neither
//! `AsyncApp` nor `Entity` ever crosses the thread boundary (both
//! are `!Send`), so only `SensorData` (plain `Send` data) moves
//! between threads.

use std::time::{Duration, Instant};

use axioo_lib::{battery, cpu, dmi, hwmon, nvidia, rapl, rapl::RaplDomain};
use gpui::{
    App, AppContext, Application, AsyncApp, Context, IntoElement, ParentElement, Render, Styled,
    Timer, Window, WindowOptions, div, rgb, white,
};

#[derive(Clone, Default)]
struct SensorData {
    rows: Vec<(String, String)>,
    stamp: String,
}

struct RootView {
    product: String,
    data: SensorData,
}

impl RootView {
    fn new(product: String) -> Self {
        Self { product, data: SensorData::default() }
    }
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.data.rows.iter().map(|(tag, val)| {
            div()
                .flex()
                .flex_row()
                .justify_between()
                .w_full()
                .child(div().text_color(rgb(0x89b4fa)).child(tag.clone()))
                .child(div().child(val.clone()))
        });
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .bg(rgb(0x1e1e2e))
            .text_color(white())
            .text_sm()
            .child(div().text_2xl().child("axioo-control-center"))
            .child(
                div()
                    .text_color(rgb(0xa6adc4))
                    .child(format!("{} · {}", self.product, self.data.stamp)),
            )
            .children(rows)
    }
}

fn f1(v: Option<f64>, unit: &str) -> String {
    v.map_or_else(|| "-".to_string(), |x| format!("{x:.1}{unit}"))
}

fn sample(prev_rapl: &[RaplDomain], prev_t: Instant, n: u64) -> (SensorData, Vec<RaplDomain>, Instant) {
    let now = Instant::now();
    let dt = now.duration_since(prev_t).as_secs_f64().max(0.001);
    let mut rows: Vec<(String, String)> = Vec::new();

    let c = cpu::sample();
    rows.push((
        "CPU".to_string(),
        format!(
            "pkg {} · max {} · {} / {} · {}",
            f1(c.package_temp_c, "C"),
            f1(c.max_core_temp_c, "C"),
            f1(c.avg_mhz, "MHz"),
            f1(c.max_mhz, "MHz"),
            c.governor.as_deref().unwrap_or("?"),
        ),
    ));

    let cur = rapl::domains();
    for d in &cur {
        let w = prev_rapl
            .iter()
            .find(|p| p.id == d.id)
            .and_then(|p| rapl::watts(p, d, dt))
            .map_or("-".to_string(), |x| format!("{x:.1}W"));
        rows.push(("PWR".to_string(), format!("{} {}", d.name, w)));
    }

    for g in nvidia::gpus().unwrap_or_default() {
        rows.push((
            "GPU".to_string(),
            format!(
                "{} · {} · {} · {}",
                f1(g.temp_c, "C"),
                f1(g.power_w, "W"),
                g.gr_clock_mhz.map_or("-".to_string(), |x| format!("{x}MHz")),
                g.mem_clock_mhz.map_or("-".to_string(), |x| format!("{x}MHz")),
            ),
        ));
    }

    let fans = hwmon::fans();
    if fans.is_empty() {
        rows.push(("FAN".to_string(), "(no hwmon fan nodes)".to_string()));
    }
    for f in &fans {
        rows.push(("FAN".to_string(), format!("{} {} RPM", f.label, f.rpm)));
    }

    for b in battery::batteries() {
        rows.push((
            "BAT".to_string(),
            format!(
                "{} {}% {} {}",
                b.name,
                b.capacity_pct.map_or("-".to_string(), |x| format!("{x:.0}")),
                b.status.as_deref().unwrap_or("?"),
                f1(b.power_w, "W"),
            ),
        ));
    }

    let data = SensorData { rows, stamp: format!("live · update #{n}") };
    (data, cur, now)
}

fn main() {
    Application::new().run(|app: &mut App| {
        let product = dmi::read_dmi()
            .get("product_name")
            .cloned()
            .unwrap_or_else(|| "Axioo".to_string());

        // Only `SensorData` (all `Send`) crosses threads. `Entity` and
        // `AsyncApp` stay on the UI thread inside the foreground task below.
        let (tx, rx) = std::sync::mpsc::channel::<SensorData>();
        std::thread::spawn(move || {
            let mut prev_rapl = rapl::domains();
            let mut prev_t = Instant::now();
            let mut n = 1u64;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let (data, cur, now) = sample(&prev_rapl, prev_t, n);
                prev_rapl = cur;
                prev_t = now;
                n += 1;
                if tx.send(data).is_err() {
                    break;
                }
            }
        });

        let view = app.new(|_| RootView::new(product));
        app.open_window(WindowOptions::default(), {
            let view = view.clone();
            move |_, _| view
        })
        .unwrap();

        // Foreground poll loop: drain to latest snapshot, then update view.
        // NOTE: async-closure form (`async move |cx|`) so the future owns
        // its captures; `|cx| async move {}` would borrow `cx` and break
        // the `R: 'static` bound on `App::spawn`.
        app.spawn(async move |cx: &mut AsyncApp| {
            loop {
                Timer::after(Duration::from_millis(250)).await;
                let mut latest: Option<SensorData> = None;
                loop {
                    match rx.try_recv() {
                        Ok(data) => latest = Some(data),
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                if let Some(data) = latest {
                    if view
                        .update(cx, |v, cx| {
                            v.data = data;
                            cx.notify();
                        })
                        .is_err()
                    {
                        return;
                    }
                }
            }
        })
        .detach();
    });
}
