#![windows_subsystem = "windows"]

//! GPUI shell for the Lancer RGB engine.
//!
//! GPUI has no stock widgets, so the sliders and buttons here are drawn from
//! divs. Slider geometry comes from a `canvas` prepaint so mouse positions can
//! be converted to values.

use gpui::*;
use lancer::{apply_all, detect, load_saved, save_saved, Rgb, Smbus};
use std::cell::Cell;
use std::rc::Rc;

const BG: u32 = 0x1618_1d;
const PANEL: u32 = 0x1e_222a;
const LINE: u32 = 0x2a_2e37;
const TEXT: u32 = 0xe8_ecf3;
const MUTED: u32 = 0x8b_93a1;
const ACCENT: u32 = 0x5b_8def;

fn hex(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "LancerRGB";

fn startup_enabled() -> bool {
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn set_startup(enabled: bool) {
    let mut cmd = std::process::Command::new("reg");
    if enabled {
        let exe = std::env::current_exe().unwrap_or_default();
        let value = format!("\"{}\" --apply-last", exe.display());
        cmd.args(["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &value, "/f"]);
    } else {
        cmd.args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"]);
    }
    let _ = cmd.output();
}

#[derive(Clone)]
struct Track {
    bounds: Rc<Cell<Bounds<Pixels>>>,
    dragging: Rc<Cell<bool>>,
}

impl Track {
    fn new() -> Self {
        Self {
            bounds: Rc::new(Cell::new(Bounds {
                origin: point(px(0.), px(0.)),
                size: size(px(1.), px(1.)),
            })),
            dragging: Rc::new(Cell::new(false)),
        }
    }
}

struct Root {
    r: u8,
    g: u8,
    b: u8,
    bright: u8,
    status: SharedString,
    devices: Vec<SharedString>,
    enabled: bool,
    tracks: [Track; 4],
}

impl Root {
    fn colour(&self) -> Rgb {
        let s = self.bright.clamp(5, 100) as u32;
        Rgb::new(
            (self.r as u32 * s / 100) as u8,
            (self.g as u32 * s / 100) as u8,
            (self.b as u32 * s / 100) as u8,
        )
    }

    fn set_from_x(&mut self, idx: usize, x: Pixels) {
        let b = self.tracks[idx].bounds.get();
        let w = b.size.width.as_f32().max(1.0);
        let t = ((x.as_f32() - b.origin.x.as_f32()) / w).clamp(0.0, 1.0);
        match idx {
            0 => self.r = (t * 255.0).round() as u8,
            1 => self.g = (t * 255.0).round() as u8,
            2 => self.b = (t * 255.0).round() as u8,
            _ => self.bright = (5.0 + t * 95.0).round() as u8,
        }
    }

    fn apply(&mut self, save: bool) {
        let c = self.colour();
        self.status = match apply_all(c, save) {
            Ok((ok, total)) => {
                save_saved(Rgb::new(self.r, self.g, self.b), self.bright);
                format!(
                    "applied #{:02X}{:02X}{:02X} at {}% to {}/{} module(s){}",
                    c.r,
                    c.g,
                    c.b,
                    self.bright,
                    ok,
                    total,
                    if save { " - saved" } else { "" },
                )
                .into()
            }
            Err(e) => format!("{e}").into(),
        };
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = hex(self.r, self.g, self.b);
        let c = self.colour();

        let mut sliders = Vec::new();
        for (idx, (label, value, accent)) in [
            ("R", self.r, 0xe0_5a5a),
            ("G", self.g, 0x57_c46a),
            ("B", self.b, 0x5b_8def),
            ("%", self.bright, 0xc9_a227),
        ]
        .into_iter()
        .enumerate()
        {
            let t = self.tracks[idx].clone();
            let tb = t.bounds.clone();
            let td = t.dragging.clone();
            let ratio = if idx == 3 {
                (value as f32 - 5.0) / 95.0
            } else {
                value as f32 / 255.0
            }
            .clamp(0.0, 1.0);

            let el = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.))
                .child(div().w(px(16.)).text_color(rgb(accent)).child(label))
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h(px(22.))
                        .child(
                            canvas(
                                move |bounds, _w, _cx| {
                                    tb.set(bounds);
                                },
                                |_b, _t, _w, _cx| {},
                            )
                            .absolute()
                            .left(px(0.))
                            .top(px(0.))
                            .size_full(),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(8.))
                                .w_full()
                                .h(px(6.))
                                .rounded_full()
                                .bg(rgb(LINE)),
                        )
                        .child(
                            div()
                                .absolute()
                                .left(px(0.))
                                .top(px(8.))
                                .w(DefiniteLength::Fraction(ratio))
                                .h(px(6.))
                                .rounded_full()
                                .bg(rgb(accent)),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this: &mut Root, ev: &MouseDownEvent, _w, cx| {
                                td.set(true);
                                this.set_from_x(idx, ev.position.x);
                                cx.notify();
                            }),
                        )
                        .on_mouse_move(cx.listener(
                            move |this: &mut Root, ev: &MouseMoveEvent, _w, cx| {
                                if this.tracks[idx].dragging.get() {
                                    this.set_from_x(idx, ev.position.x);
                                    cx.notify();
                                }
                            },
                        ))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |this: &mut Root, _ev, _w, _cx| {
                                this.tracks[idx].dragging.set(false);
                            }),
                        ),
                )
                .child(
                    div()
                        .w(px(34.))
                        .text_color(rgb(0xcd_d4df))
                        .child(format!("{value}")),
                );
            sliders.push(el);
        }

        let mut preset_row = div().flex().flex_row().gap(px(8.));
        for (name, r, g, b) in [
            ("Red", 255u8, 0u8, 0u8),
            ("Green", 0, 255, 0),
            ("Blue", 0, 0, 255),
            ("Purple", 160, 60, 255),
            ("Cyan", 0, 255, 255),
            ("Warm", 255, 170, 90),
            ("Off", 0, 0, 0),
        ] {
            preset_row = preset_row.child(
                div()
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(px(6.))
                    .bg(rgb(0x24_2831))
                    .text_color(rgb(TEXT))
                    .hover(|s| s.bg(rgb(0x30_3642)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this: &mut Root, _ev, _w, cx| {
                            this.r = r;
                            this.g = g;
                            this.b = b;
                            cx.notify();
                        }),
                    )
                    .child(name),
            );
        }

        let enabled = self.enabled;
        let check_bg = if enabled { ACCENT } else { 0x24_2831 };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG))
            .p(px(18.))
            .gap(px(10.))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child("Lancer Blade RGB"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("ENE SMBus - no account - no background app"),
            )
            .child(
                div()
                    .h(px(78.))
                    .w_full()
                    .rounded(px(10.))
                    .bg(rgb(preview))
                    .border_1()
                    .border_color(rgb(LINE)),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "#{:02X}{:02X}{:02X}  ->  #{:02X}{:02X}{:02X} (output)",
                        self.r, self.g, self.b, c.r, c.g, c.b
                    )),
            )
            .children(sliders)
            .child(preset_row)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .py(px(9.))
                            .rounded(px(7.))
                            .bg(rgb(ACCENT))
                            .text_color(rgb(0x0b_0d11))
                            .text_size(px(13.))
                            .flex()
                            .justify_center()
                            .hover(|s| s.bg(rgb(0x74_9ff5)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this: &mut Root, _ev, _w, cx| {
                                    this.apply(false);
                                    cx.notify();
                                }),
                            )
                            .child("Apply"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .py(px(9.))
                            .rounded(px(7.))
                            .bg(rgb(PANEL))
                            .border_1()
                            .border_color(rgb(LINE))
                            .text_size(px(13.))
                            .flex()
                            .justify_center()
                            .hover(|s| s.bg(rgb(0x28_2d37)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this: &mut Root, _ev, _w, cx| {
                                    this.apply(true);
                                    cx.notify();
                                }),
                            )
                            .child("Apply + save to DIMM"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(16.))
                            .rounded(px(4.))
                            .bg(rgb(check_bg))
                            .border_1()
                            .border_color(rgb(LINE))
                            .flex()
                            .justify_center()
                            .text_size(px(11.))
                            .text_color(rgb(0x0b_0d11))
                            .child(if enabled { "x" } else { "" }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0xcd_d4df))
                            .child("Re-apply my colour when Windows starts"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this: &mut Root, _ev, _w, cx| {
                            this.enabled = !this.enabled;
                            set_startup(this.enabled);
                            cx.notify();
                        }),
                    ),
            )
            .child(div().h(px(1.)).w_full().bg(rgb(0x26_2a33)))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(0x6f_7787))
                    .child("MODULES"),
            )
            .children(self.devices.iter().cloned().map(|d| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xb9_c1cd))
                    .child(d)
            }))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0x7f_8a9c))
                    .child(self.status.clone()),
            )
    }
}

fn main() {
    if std::env::args().any(|a| a == "--apply-last") {
        lancer::apply_saved();
        return;
    }

    gpui_platform::application().run(|cx: &mut App| {
        let (c, bright) = load_saved();
        let enabled = startup_enabled();

        let mut devices = Vec::new();
        let status: SharedString = match Smbus::open() {
            Ok(bus) => {
                let found = detect(&bus);
                for d in &found {
                    devices.push(format!("0x{:02X}   {}   {} LEDs", d.address, d.name, d.leds).into());
                }
                if found.is_empty() {
                    "No ENE module found on SMBus 0x70-0x77.".into()
                } else {
                    format!("{} module(s) ready", found.len()).into()
                }
            }
            Err(e) => format!("{e}").into(),
        };

        let root = Root {
            r: c.r,
            g: c.g,
            b: c.b,
            bright,
            status,
            devices,
            enabled,
            tracks: [Track::new(), Track::new(), Track::new(), Track::new()],
        };

        let bounds = Bounds {
            origin: point(px(240.), px(160.)),
            size: size(px(540.), px(660.)),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Lancer RGB".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_window, cx| cx.new(|_| root),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}

