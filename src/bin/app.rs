#![windows_subsystem = "windows"]

//! Raw Win32/GDI shell for the Lancer engine.
//!
//! No UI framework: one window, a message loop, and GDI rectangles/text.
//! The whole binary is the engine plus ~600 lines of Win32.

use lancer::{
    apply_all, detect, load_saved, save_saved, set_startup, startup_enabled, Rgb, Smbus,
};
use std::ffi::c_void;
use std::ptr::{null, null_mut};

type Hwnd = *mut c_void;
type Hdc = *mut c_void;
type Hfont = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct PaintStruct {
    hdc: Hdc,
    erase: i32,
    rc: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[repr(C)]
struct WndClassW {
    style: u32,
    wnd_proc: usize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: *mut c_void,
    icon: *mut c_void,
    cursor: *mut c_void,
    background: *mut c_void,
    menu_name: *const u16,
    class_name: *const u16,
}

extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: Hwnd,
        menu: *mut c_void,
        instance: *mut c_void,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
    fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
    fn UpdateWindow(h: Hwnd) -> i32;
    fn GetMessageW(m: *mut Msg, h: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(m: *const Msg) -> i32;
    fn DispatchMessageW(m: *const Msg) -> isize;
    fn PostQuitMessage(code: i32);
    fn BeginPaint(h: Hwnd, ps: *mut PaintStruct) -> Hdc;
    fn EndPaint(h: Hwnd, ps: *const PaintStruct) -> i32;
    fn InvalidateRect(h: Hwnd, r: *const Rect, erase: i32) -> i32;
    fn GetClientRect(h: Hwnd, r: *mut Rect) -> i32;
    fn SetWindowLongPtrW(h: Hwnd, idx: i32, val: isize) -> isize;
    fn GetWindowLongPtrW(h: Hwnd, idx: i32) -> isize;
    fn LoadCursorW(inst: *mut c_void, name: *const u16) -> *mut c_void;
    fn SetProcessDPIAware() -> i32;
    fn AdjustWindowRectEx(r: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
}

extern "system" {
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn CreateCompatibleBitmap(dc: Hdc, w: i32, h: i32) -> *mut c_void;
    fn SelectObject(dc: Hdc, obj: *mut c_void) -> *mut c_void;
    fn DeleteObject(o: *mut c_void) -> i32;
    fn DeleteDC(dc: Hdc) -> i32;
    fn BitBlt(dst: Hdc, x: i32, y: i32, w: i32, h: i32, src: Hdc, sx: i32, sy: i32, rop: u32) -> i32;
    fn CreateSolidBrush(c: u32) -> *mut c_void;
    fn FillRect(dc: Hdc, r: *const Rect, b: *mut c_void) -> i32;
    fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        orient: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_prec: u32,
        clip_prec: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Hfont;
    fn SetBkMode(dc: Hdc, mode: i32) -> i32;
    fn SetTextColor(dc: Hdc, c: u32) -> u32;
    fn DrawTextW(dc: Hdc, s: *const u16, len: i32, r: *mut Rect, fmt: u32) -> i32;
    fn GetDC(h: Hwnd) -> Hdc;
    fn ReleaseDC(h: Hwnd, dc: Hdc) -> i32;
}

const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;

const GWLP_USERDATA: i32 = -21;
const DT_LEFT: u32 = 0x0000;
const DT_RIGHT: u32 = 0x0002;
const DT_CENTER: u32 = 0x0001;
const DT_VCENTER: u32 = 0x0004;
const DT_SINGLELINE: u32 = 0x0020;
const SRCCOPY: u32 = 0x00CC_0020;

const BG: u32 = 0x001d_1816;
const PANEL: u32 = 0x002a_221e;
const LINE: u32 = 0x0037_2e2a;
const TEXT: u32 = 0x00f3_ecE8;
const MUTED: u32 = 0x00a1_938b;
const ACCENT: u32 = 0x00ef_8d5b;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// COLORREF is 0x00BBGGRR.
fn bgr(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

struct App {
    r: u8,
    g: u8,
    b: u8,
    bright: u8,
    enabled: bool,
    status: String,
    devices: Vec<String>,
    drag: Option<usize>,
    swatch: Rect,
    sliders: [Rect; 4],
    presets: [Rect; 7],
    apply: Rect,
    apply_save: Rect,
    check: Rect,
}

const PRESETS: [(u8, u8, u8); 7] = [
    (255, 0, 0),
    (0, 255, 0),
    (0, 0, 255),
    (160, 60, 255),
    (0, 255, 255),
    (255, 170, 90),
    (0, 0, 0),
];

impl App {
    fn colour(&self) -> Rgb {
        let s = self.bright.clamp(5, 100) as u32;
        Rgb::new(
            (self.r as u32 * s / 100) as u8,
            (self.g as u32 * s / 100) as u8,
            (self.b as u32 * s / 100) as u8,
        )
    }

    fn layout(&mut self, w: i32) {
        let pad = 18;
        let inner = w - pad * 2;
        self.swatch = Rect { left: pad, top: 68, right: w - pad, bottom: 146 };
        let mut y = 176;
        for i in 0..4 {
            self.sliders[i] = Rect { left: pad + 26, top: y, right: w - pad - 46, bottom: y + 22 };
            y += 30;
        }
        let mut x = pad;
        for i in 0..7 {
            self.presets[i] = Rect { left: x, top: y + 6, right: x + 26, bottom: y + 32 };
            x += 34;
        }
        self.apply = Rect { left: pad, top: y + 46, right: pad + inner / 2 - 4, bottom: y + 80 };
        self.apply_save = Rect { left: pad + inner / 2 + 4, top: y + 46, right: w - pad, bottom: y + 80 };
        self.check = Rect { left: pad, top: y + 92, right: pad + 16, bottom: y + 108 };
    }

    fn hit(r: &Rect, x: i32, y: i32) -> bool {
        x >= r.left && x < r.right && y >= r.top && y < r.bottom
    }

    fn set_from(&mut self, idx: usize, x: i32) {
        let r = self.sliders[idx];
        let w = (r.right - r.left).max(1);
        let t = ((x - r.left) as f32 / w as f32).clamp(0.0, 1.0);
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
                    if save { " - saved" } else { "" }
                )
            }
            Err(e) => e.to_string(),
        };
    }
}

fn fill(dc: Hdc, r: &Rect, c: u32) {
    unsafe {
        let b = CreateSolidBrush(c);
        FillRect(dc, r, b);
        DeleteObject(b);
    }
}

fn text(dc: Hdc, s: &str, r: Rect, c: u32, size: i32, bold: bool, fmt: u32) {
    unsafe {
        let face = wide("Segoe UI");
        let f = CreateFontW(
            -size, 0, 0, 0,
            if bold { 700 } else { 400 },
            0, 0, 0, 1, 0, 0, 5, 0,
            face.as_ptr(),
        );
        let old = SelectObject(dc, f);
        SetBkMode(dc, 1);
        SetTextColor(dc, c);
        let w: Vec<u16> = s.encode_utf16().collect();
        let mut rr = r;
        DrawTextW(dc, w.as_ptr(), w.len() as i32, &mut rr, fmt | DT_SINGLELINE | DT_VCENTER);
        SelectObject(dc, old);
        DeleteObject(f);
    }
}

fn draw(hwnd: Hwnd, app: &App) {
    unsafe {
        let mut client = Rect { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut client);
        let w = client.right;
        let h = client.bottom;

        let screen = GetDC(hwnd);
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);

        fill(mem, &client, BG);

        let pad = 18;
        text(mem, "Lancer Blade RGB", Rect { left: pad, top: 14, right: w - pad, bottom: 40 }, TEXT, 20, true, DT_LEFT);
        text(mem, "ENE SMBus - no account - no background app", Rect { left: pad, top: 42, right: w - pad, bottom: 58 }, MUTED, 12, false, DT_LEFT);

        let pv = bgr(app.r, app.g, app.b);
        fill(mem, &app.swatch, pv);

        let c = app.colour();
        text(
            mem,
            &format!("#{:02X}{:02X}{:02X}  ->  #{:02X}{:02X}{:02X} (output)", app.r, app.g, app.b, c.r, c.g, c.b),
            Rect { left: pad, top: 148, right: w - pad, bottom: 166 },
            MUTED, 12, false, DT_LEFT,
        );

        let labels = ["R", "G", "B", "%"];
        let values = [app.r, app.g, app.b, app.bright];
        let accents = [bgr(224, 90, 90), bgr(87, 196, 106), bgr(91, 141, 239), bgr(201, 162, 39)];
        for i in 0..4 {
            let r = app.sliders[i];
            text(mem, labels[i], Rect { left: pad, top: r.top, right: pad + 20, bottom: r.bottom }, accents[i], 12, true, DT_LEFT);
            let track = Rect { left: r.left, top: r.top + 8, right: r.right, bottom: r.top + 14 };
            fill(mem, &track, LINE);
            let ratio = if i == 3 {
                (values[i] as f32 - 5.0) / 95.0
            } else {
                values[i] as f32 / 255.0
            }
            .clamp(0.0, 1.0);
            let fillw = ((r.right - r.left) as f32 * ratio) as i32;
            let filled = Rect { left: r.left, top: track.top, right: r.left + fillw, bottom: track.bottom };
            fill(mem, &filled, accents[i]);
            text(mem, &values[i].to_string(), Rect { left: r.right + 8, top: r.top, right: w - pad, bottom: r.bottom }, TEXT, 12, false, DT_RIGHT);
        }

        for i in 0..7 {
            let (r, g, b) = PRESETS[i];
            fill(mem, &app.presets[i], bgr(r, g, b));
        }

        fill(mem, &app.apply, ACCENT);
        text(mem, "Apply", app.apply, bgr(11, 13, 17), 13, true, DT_CENTER);

        fill(mem, &app.apply_save, PANEL);
        text(mem, "Apply + save to DIMM", app.apply_save, TEXT, 13, false, DT_CENTER);

        fill(mem, &app.check, if app.enabled { ACCENT } else { PANEL });
        text(
            mem,
            "Re-apply my colour when Windows starts",
            Rect { left: pad + 24, top: app.check.top - 2, right: w - pad, bottom: app.check.bottom + 2 },
            TEXT, 12, false, DT_LEFT,
        );

        let sep_y = app.check.bottom + 14;
        fill(mem, &Rect { left: pad, top: sep_y, right: w - pad, bottom: sep_y + 1 }, LINE);
        text(mem, "MODULES", Rect { left: pad, top: sep_y + 8, right: w - pad, bottom: sep_y + 24 }, MUTED, 11, true, DT_LEFT);

        let mut y = sep_y + 28;
        for d in &app.devices {
            text(mem, d, Rect { left: pad, top: y, right: w - pad, bottom: y + 16 }, TEXT, 12, false, DT_LEFT);
            y += 18;
        }
        text(mem, &app.status, Rect { left: pad, top: y + 6, right: w - pad, bottom: y + 22 }, MUTED, 12, false, DT_LEFT);

        BitBlt(screen, 0, 0, w, h, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(hwnd, screen);
    }
}

unsafe fn app_ptr(hwnd: Hwnd) -> *mut App {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App
}

extern "system" fn wnd_proc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut ps = std::mem::zeroed::<PaintStruct>();
                let dc = BeginPaint(hwnd, &mut ps);
                let app = app_ptr(hwnd);
                if !app.is_null() {
                    draw(hwnd, &*app);
                }
                let _ = dc;
                EndPaint(hwnd, &ps);
                0
            }
            WM_ERASEBKGND => 1,
            WM_LBUTTONDOWN => {
                let app = &mut *app_ptr(hwnd);
                let x = (lparam & 0xFFFF) as i16 as i32;
                let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
                for i in 0..7 {
                    if App::hit(&app.presets[i], x, y) {
                        let (r, g, b) = PRESETS[i];
                        app.r = r;
                        app.g = g;
                        app.b = b;
                        break;
                    }
                }
                for i in 0..4 {
                    if App::hit(&app.sliders[i], x, y) {
                        app.drag = Some(i);
                        app.set_from(i, x);
                    }
                }
                if App::hit(&app.apply, x, y) {
                    app.apply(false);
                } else if App::hit(&app.apply_save, x, y) {
                    app.apply(true);
                } else if App::hit(&app.check, x, y) {
                    app.enabled = !app.enabled;
                    set_startup(app.enabled);
                }
                InvalidateRect(hwnd, null(), 0);
                0
            }
            WM_MOUSEMOVE => {
                let app = &mut *app_ptr(hwnd);
                if let Some(i) = app.drag {
                    let x = (lparam & 0xFFFF) as i16 as i32;
                    app.set_from(i, x);
                    InvalidateRect(hwnd, null(), 0);
                }
                0
            }
            WM_LBUTTONUP => {
                let app = &mut *app_ptr(hwnd);
                app.drag = None;
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--apply-last") {
        lancer::apply_saved();
        return;
    }

    unsafe {
        SetProcessDPIAware();

        let (c, bright) = load_saved();
        let mut devices = Vec::new();
        let status;
        match Smbus::open() {
            Ok(bus) => {
                let found = detect(&bus);
                for d in &found {
                    devices.push(format!("0x{:02X}   {}   {} LEDs", d.address, d.name, d.leds));
                }
                status = if found.is_empty() {
                    "No ENE module found on SMBus 0x70-0x77.".to_string()
                } else {
                    format!("{} module(s) ready", found.len())
                };
            }
            Err(_) => {
                status = "cannot open \\\\.\\CTIIO (is the ENE driver installed?)".to_string();
            }
        }

        let app = Box::into_raw(Box::new(App {
            r: c.r,
            g: c.g,
            b: c.b,
            bright,
            enabled: startup_enabled(),
            status,
            devices,
            drag: None,
            swatch: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            sliders: [Rect { left: 0, top: 0, right: 0, bottom: 0 }; 4],
            presets: [Rect { left: 0, top: 0, right: 0, bottom: 0 }; 7],
            apply: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            apply_save: Rect { left: 0, top: 0, right: 0, bottom: 0 },
            check: Rect { left: 0, top: 0, right: 0, bottom: 0 },
        }));
        (*app).layout(520);

        let class = wide("LancerRgbWindow");
        let wc = WndClassW {
            style: 0,
            wnd_proc: wnd_proc as *const () as usize,
            cls_extra: 0,
            wnd_extra: 0,
            instance: null_mut(),
            icon: null_mut(),
            cursor: LoadCursorW(null_mut(), 32512 as *const u16),
            background: null_mut(),
            menu_name: null(),
            class_name: class.as_ptr(),
        };
        RegisterClassW(&wc);

        let style = 0x00CF_0000u32 & !0x0004_0000 & !0x0001_0000;
        let mut frame = Rect { left: 0, top: 0, right: 520, bottom: 520 };
        AdjustWindowRectEx(&mut frame, style, 0, 0);

        let title = wide("Lancer RGB");
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            style,
            260,
            140,
            frame.right - frame.left,
            frame.bottom - frame.top,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        );
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
        ShowWindow(hwnd, 1);
        UpdateWindow(hwnd);

        let mut msg = std::mem::zeroed::<Msg>();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
