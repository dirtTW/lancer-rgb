use lancer::{detect, Dim, Rgb, Smbus};
use slint::{ModelRc, SharedString, VecModel};
use std::rc::Rc;

slint::slint! {
    import { Button, Slider, VerticalBox, HorizontalBox, CheckBox } from "std-widgets.slint";

    export component AppWindow inherits Window {
        in-out property <int> red: 255;
        in-out property <int> green: 0;
        in-out property <int> blue: 0;
        in-out property <string> status: "ready";
        in-out property <[string]> devices: [];
        in-out property <bool> remember: false;

        callback apply();
        callback apply_save();
        callback preset(int, int, int);
        callback remember_changed(bool);

        title: "Lancer RGB";
        width: 520px;
        height: 620px;
        background: #16181d;

        VerticalBox {
            padding: 18px;
            spacing: 12px;

            Text { text: "Lancer Blade RGB"; color: #f2f4f8; font-size: 20px; font-weight: 800; }
            Text {
                text: "Direct ENE SMBus control · no account · no background app";
                color: #8b93a1; font-size: 12px;
            }

            Rectangle {
                height: 84px; border-radius: 10px;
                background: rgb(root.red, root.green, root.blue);
                border-width: 1px; border-color: #2a2e37;
            }

            HorizontalBox {
                spacing: 10px;
                Text { text: "R"; color: #e05a5a; width: 18px; vertical-alignment: center; }
                Slider { minimum: 0; maximum: 255; value: root.red; changed(v) => { root.red = round(v); } }
                Text { text: root.red; color: #cdd4df; width: 34px; horizontal-alignment: right; vertical-alignment: center; }
            }
            HorizontalBox {
                spacing: 10px;
                Text { text: "G"; color: #57c46a; width: 18px; vertical-alignment: center; }
                Slider { minimum: 0; maximum: 255; value: root.green; changed(v) => { root.green = round(v); } }
                Text { text: root.green; color: #cdd4df; width: 34px; horizontal-alignment: right; vertical-alignment: center; }
            }
            HorizontalBox {
                spacing: 10px;
                Text { text: "B"; color: #5b8def; width: 18px; vertical-alignment: center; }
                Slider { minimum: 0; maximum: 255; value: root.blue; changed(v) => { root.blue = round(v); } }
                Text { text: root.blue; color: #cdd4df; width: 34px; horizontal-alignment: right; vertical-alignment: center; }
            }

            HorizontalBox {
                spacing: 8px;
                Button { text: "Red"; clicked => { root.preset(255, 0, 0); } }
                Button { text: "Green"; clicked => { root.preset(0, 255, 0); } }
                Button { text: "Blue"; clicked => { root.preset(0, 0, 255); } }
                Button { text: "Purple"; clicked => { root.preset(160, 60, 255); } }
                Button { text: "Cyan"; clicked => { root.preset(0, 255, 255); } }
                Button { text: "Warm"; clicked => { root.preset(255, 170, 90); } }
                Button { text: "Off"; clicked => { root.preset(0, 0, 0); } }
            }

            HorizontalBox {
                spacing: 8px;
                Button { text: "Apply"; clicked => { root.apply(); } }
                Button { text: "Apply + save to DIMM"; clicked => { root.apply_save(); } }
            }

            CheckBox {
                text: "Re-apply my colour when Windows starts";
                checked: root.remember;
                toggled => { root.remember_changed(self.checked); }
            }

            Rectangle { height: 1px; background: #262a33; }

            Text { text: "MODULES"; color: #6f7787; font-size: 11px; font-weight: 700; }
            VerticalBox {
                spacing: 2px;
                for d in root.devices: Text { text: d; color: #b9c1cd; font-size: 12px; }
            }

            Text { text: root.status; color: #7f8a9c; font-size: 12px; wrap: word-wrap; }
        }
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_NAME: &str = "LancerRGB";

fn config_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&base).join("lancer-rgb").join("last.txt")
}

fn save_config(c: Rgb) {
    let p = config_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(&p, format!("{} {} {}", c.r, c.g, c.b));
}

fn load_config() -> Rgb {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| {
            let v: Vec<u8> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() == 3).then(|| Rgb::new(v[0], v[1], v[2]))
        })
        .unwrap_or(Rgb::new(255, 0, 0))
}

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

fn apply_last() {
    let c = load_config();
    // At logon the SMBus/DIMMs may not answer for a moment, so retry.
    for _ in 0..20 {
        if let Ok(bus) = Smbus::open() {
            let dims = detect(&bus);
            if !dims.is_empty() {
                for d in &dims {
                    let _ = bus.set_solid(d, c);
                    let _ = bus.save(d);
                }
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

fn main() -> Result<(), slint::PlatformError> {
    if std::env::args().any(|a| a == "--apply-last") {
        apply_last();
        return Ok(());
    }

    let ui = AppWindow::new()?;
    ui.set_remember(startup_enabled());

    let bus = Rc::new(Smbus::open());
    let dims: Vec<Dim> = match bus.as_ref() {
        Ok(b) => detect(b),
        Err(_) => Vec::new(),
    };

    match bus.as_ref() {
        Err(e) => ui.set_status(format!("{e}").into()),
        Ok(_) if dims.is_empty() => {
            ui.set_status("No ENE module found on SMBus 0x70-0x77.".into())
        }
        Ok(_) => {
            let lines: Vec<SharedString> = dims
                .iter()
                .map(|d| format!("0x{:02X}   {}   {} LEDs", d.address, d.name, d.leds).into())
                .collect();
            ui.set_devices(ModelRc::new(VecModel::from(lines)));
            let saved = load_config();
            ui.set_red(saved.r as i32);
            ui.set_green(saved.g as i32);
            ui.set_blue(saved.b as i32);
            ui.set_status(format!("{} module(s) ready", dims.len()).into());
        }
    }

    let current = {
        let bus = bus.clone();
        let dims = dims.clone();
        let weak = ui.as_weak();
        Rc::new(move |save: bool| {
            let Some(ui) = weak.upgrade() else { return };
            let c = Rgb::new(ui.get_red() as u8, ui.get_green() as u8, ui.get_blue() as u8);
            let Ok(bus) = bus.as_ref() else { return };
            let mut ok = 0;
            for d in &dims {
                if bus.set_solid(d, c).is_ok() {
                    if save {
                        let _ = bus.save(d);
                    }
                    ok += 1;
                }
            }
            save_config(c);
            let hex = format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b);
            let tail = if save { " · saved into the module" } else { "" };
            ui.set_status(format!("applied {hex} to {ok}/{} module(s){tail}", dims.len()).into());
        })
    };

    {
        let f = current.clone();
        ui.on_apply(move || f(false));
    }
    {
        let f = current.clone();
        ui.on_apply_save(move || f(true));
    }
    {
        let weak = ui.as_weak();
        ui.on_preset(move |r, g, b| {
            if let Some(ui) = weak.upgrade() {
                ui.set_red(r);
                ui.set_green(g);
                ui.set_blue(b);
            }
        });
    }
    ui.on_remember_changed(|v| set_startup(v));

    ui.run()
}
