//! Standalone ENE SMBus RGB engine for XPG Lancer Blade DDR5.
//!
//! Talks straight to the signed CTIIO kernel driver (`\\.\CTIIO`) that the
//! vendor pack installs. No Electron, no vendor DLL, no login.
//!
//! Reverse-engineered register map (device name `AUDA0-E6K5-0101`,
//! second-generation ENE DRAM controller):
//!   0x1000  device name, 16 ASCII bytes
//!   0x1C00  64-byte config table (byte 2 = LED count, 0x13 = first channel)
//!   0x8100  direct colour registers (v2), R,B,G per LED
//!   0x8160  effect colour registers (v2), R,B,G per LED
//!   0x8021  mode (1 = static)
//!   0x8022  speed
//!   0x8023  direction
//!   0x80A0  apply (0x01) / save to module (0xAA)
//!
//! SMBus access is register-indirect: write the 16-bit register address to
//! command 0x00, then read/write command 0x01. The register pointer
//! auto-increments on byte writes and reads.

use std::ffi::c_void;

pub const IOCTL_READ_SMB_TYPE: u32 = 0x8050_2140;
pub const IOCTL_READ_QUICK: u32 = 0x8050_2144;
pub const IOCTL_READ_BYTE: u32 = 0x8050_2148;
pub const IOCTL_WRITE_BYTE: u32 = 0x8050_214C;
pub const IOCTL_READ_WORD: u32 = 0x8050_2150;
pub const IOCTL_WRITE_WORD: u32 = 0x8050_2154;
pub const IOCTL_READ_BLOCK: u32 = 0x8050_2158;
pub const IOCTL_WRITE_BLOCK: u32 = 0x8050_215C;

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const OPEN_EXISTING: u32 = 3;

#[repr(C)]
struct SecurityAttributes {
    length: u32,
    sd: *mut c_void,
    inherit: i32,
}

extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        sa: *mut SecurityAttributes,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn DeviceIoControl(
        device: *mut c_void,
        code: u32,
        inbuf: *mut c_void,
        insize: u32,
        outbuf: *mut c_void,
        outsize: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// One colour, as the user thinks of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Scale by a brightness percentage (clamped to 5..=100).
    pub fn scaled(self, pct: u8) -> Self {
        let s = pct.clamp(5, 100) as u32;
        Self {
            r: (self.r as u32 * s / 100) as u8,
            g: (self.g as u32 * s / 100) as u8,
            b: (self.b as u32 * s / 100) as u8,
        }
    }

    /// `false` for effectively black colours, which must never be persisted.
    pub fn savable(self) -> bool {
        self.r.max(self.g).max(self.b) >= 16
    }
}

/// A detected DIMM.
#[derive(Clone, Debug)]
pub struct Dim { pub address: u8, pub name: String, pub leds: u8 }

/// Handle to the CTIIO SMBus driver.
pub struct Smbus {
    handle: *mut c_void,
    pub bus: u8,
}

// The handle is owned by this struct and only used from the thread that
// created it; DeviceIoControl is synchronous.
unsafe impl Send for Smbus {}

impl Smbus {
    pub fn open() -> Result<Self> {
        let path = wide(r"\\.\CTIIO");
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null_mut(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if handle.is_null() || handle as isize == -1 {
            return Err(Error(
                "cannot open \\\\.\\CTIIO (is the ENE DRAM RGB driver pack installed?)".into(),
            ));
        }
        Ok(Self { handle, bus: 0 })
    }

    fn ioctl(&self, code: u32, input: &[u8; 12]) -> Result<[u8; 8]> {
        let mut last = String::new();
        // SMBus transactions occasionally fail for a single frame (the chip
        // is busy); retrying makes writes atomic from the caller's view.
        for attempt in 0..4 {
            let mut out = [0u8; 8];
            let mut returned = 0u32;
            let ok = unsafe {
                DeviceIoControl(
                    self.handle,
                    code,
                    input.as_ptr() as *mut c_void,
                    12,
                    out.as_mut_ptr() as *mut c_void,
                    8,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(out);
            }
            last = format!("ioctl {code:#010X} failed (attempt {})", attempt + 1);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Err(Error(last))
    }

    fn request(bus: u8, addr: u8, cmd: u8) -> [u8; 12] {
        let mut b = [0u8; 12];
        b[0] = bus;
        b[1] = addr;
        b[2] = cmd;
        b
    }

    pub fn read_byte(&self, addr: u8, cmd: u8) -> Result<u8> {
        Ok(self.ioctl(IOCTL_READ_BYTE, &Self::request(self.bus, addr, cmd))?[0])
    }

    pub fn write_byte(&self, addr: u8, cmd: u8, value: u8) -> Result<()> {
        let mut b = Self::request(self.bus, addr, cmd);
        b[3] = value;
        self.ioctl(IOCTL_WRITE_BYTE, &b).map(|_| ())
    }

    pub fn write_word(&self, addr: u8, cmd: u8, value: u16) -> Result<()> {
        let mut b = Self::request(self.bus, addr, cmd);
        let v = value.to_le_bytes();
        b[3] = v[0];
        b[4] = v[1];
        self.ioctl(IOCTL_WRITE_WORD, &b).map(|_| ())
    }

    /// Point the ENE register window at `reg`.
    pub fn set_register(&self, addr: u8, reg: u16) -> Result<()> {
        self.write_word(addr, 0x00, reg)?;
        // The controller needs a moment before it accepts the follow-up
        // access; without this the register pointer occasionally desyncs.
        std::thread::sleep(std::time::Duration::from_micros(150));
        Ok(())
    }

    pub fn register_read(&self, addr: u8, reg: u16) -> Result<u8> {
        self.set_register(addr, reg)?;
        match self.read_byte(addr, 0x81) {
            Ok(v) => Ok(v),
            Err(_) => {
                self.set_register(addr, reg)?;
                self.read_byte(addr, 0x81)
            }
        }
    }

    pub fn register_write(&self, addr: u8, reg: u16, value: u8) -> Result<()> {
        self.set_register(addr, reg)?;
        self.write_byte(addr, 0x01, value)?;
        std::thread::sleep(std::time::Duration::from_micros(150));
        Ok(())
    }
}

impl Drop for Smbus {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Register map for the second-generation DRAM controllers.
pub struct Profile {
    pub colour_reg: u16,
}

pub fn profile_for(name: &str) -> Option<Profile> {
    match name {
        // Second generation DRAM (Geil Super Luce, XPG Lancer Blade DDR5)
        "AUDA0-E6K5-0101" => Some(Profile { colour_reg: 0x8160 }),
        // First generation DRAM
        "DIMM_LED-0102" | "DIMM_LED-0103" => Some(Profile { colour_reg: 0x8010 }),
        _ => None,
    }
}

/// Scan the ENE DRAM address range and return every module found.
pub fn detect(bus: &Smbus) -> Vec<Dim> {
    let mut found = Vec::new();
    for addr in [0x70u8, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77] {
        if bus.read_byte(addr, 0xA0).is_err() {
            continue;
        }
        let mut name = String::new();
        for i in 0..16u16 {
            match bus.register_read(addr, 0x1000 + i) {
                Ok(b) if b != 0 && b.is_ascii_graphic() => name.push(b as char),
                _ => break,
            }
        }
        if name.is_empty() {
            continue;
        }
        let leds = bus.register_read(addr, 0x1C02).unwrap_or(0);
        if leds == 0 {
            continue;
        }
        found.push(Dim { address: addr, name, leds });
    }
    found
}

impl Smbus {
    /// Set a solid colour on one module (effect/static mode).
    pub fn set_solid(&self, dim: &Dim, colour: Rgb) -> Result<()> {
        let profile = profile_for(&dim.name)
            .ok_or_else(|| Error(format!("unsupported controller {}", dim.name)))?;

        // Point at the colour registers, then stream R,B,G per LED.
        // The register pointer auto-increments on every byte write.
        self.set_register(dim.address, profile.colour_reg)?;
        for _ in 0..dim.leds {
            self.write_byte(dim.address, 0x01, colour.r)?;
            self.write_byte(dim.address, 0x01, colour.b)?;
            self.write_byte(dim.address, 0x01, colour.g)?;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));

        self.register_write(dim.address, 0x8021, 0x01)?; // static
        self.register_write(dim.address, 0x8022, 0x02)?; // normal speed
        self.register_write(dim.address, 0x8023, 0x00)?; // forward
        self.register_write(dim.address, 0x80A0, 0x01)?; // apply now
        Ok(())
    }

    /// Persist the current effect into the module's own controller so it
    /// survives power loss.
    pub fn save(&self, dim: &Dim) -> Result<()> {
        self.register_write(dim.address, 0x80A0, 0xAA)
    }

    /// Read back the currently programmed effect colours (R,B,G per LED).
    pub fn read_colours(&self, dim: &Dim, count: u8) -> Result<Vec<u8>> {
        let profile = profile_for(&dim.name)
            .ok_or_else(|| Error(format!("unsupported controller {}", dim.name)))?;
        self.set_register(dim.address, profile.colour_reg)?;
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            out.push(self.read_byte(dim.address, 0x81)?);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- config

fn config_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&base).join("lancer-rgb").join("last.txt")
}

/// Last colour chosen by the user, plus brightness percentage.
pub fn load_saved() -> (Rgb, u8) {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| {
            let v: Vec<u8> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
            (v.len() >= 3).then(|| {
                (Rgb::new(v[0], v[1], v[2]), if v.len() > 3 { v[3].clamp(5, 100) } else { 100 })
            })
        })
        .unwrap_or((Rgb::new(160, 60, 255), 70))
}

pub fn save_saved(c: Rgb, bright: u8) {
    let p = config_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(&p, format!("{} {} {} {}", c.r, c.g, c.b, bright));
}

/// Result of pushing a colour to every module.
pub struct Applied {
    pub ok: usize,
    pub total: usize,
    /// Whether the colour was also written into at least one module's flash.
    pub saved: bool,
}

/// Apply a colour to every detected module.
///
/// Near-black colours are applied live but never written to the module's
/// flash: a saved black profile would make the sticks boot dark, which is
/// indistinguishable from a dead LED rail.
pub fn apply_all(c: Rgb, save: bool) -> Result<Applied> {
    let bus = Smbus::open()?;
    let dims = detect(&bus);
    let mut ok = 0;
    let mut saved = false;
    for d in &dims {
        if bus.set_solid(d, c).is_ok() {
            if save && c.savable() && bus.save(d).is_ok() {
                saved = true;
            }
            ok += 1;
        }
    }
    Ok(Applied { ok, total: dims.len(), saved })
}

/// One-shot restore used by the logon entry (`lancer-apply.exe`).
pub fn apply_saved() {
    let (c, bright) = load_saved();
    let c = c.scaled(bright);
    // At logon the modules may not answer for a moment.
    for _ in 0..20 {
        match apply_all(c, true) {
            Ok(a) if a.ok > 0 => return,
            _ => std::thread::sleep(std::time::Duration::from_millis(500)),
        }
    }
}

// ------------------------------------------------------------ autostart

pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const RUN_NAME: &str = "LancerRGB";

pub fn startup_enabled() -> bool {
    std::process::Command::new("reg")
        .args(["query", RUN_KEY, "/v", RUN_NAME])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Point the Run key at the tiny restorer next to the GUI.
pub fn set_startup(enabled: bool) {
    let mut cmd = std::process::Command::new("reg");
    if enabled {
        let exe = std::env::current_exe().unwrap_or_default();
        let dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let apply = dir.join("lancer-apply.exe");
        let target = if apply.exists() { apply } else { exe };
        let value = format!("\"{}\"", target.display());
        cmd.args(["add", RUN_KEY, "/v", RUN_NAME, "/t", "REG_SZ", "/d", &value, "/f"]);
    } else {
        cmd.args(["delete", RUN_KEY, "/v", RUN_NAME, "/f"]);
    }
    let _ = cmd.output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_profiles_use_the_right_colour_registers() {
        let v2 = profile_for("AUDA0-E6K5-0101").expect("v2 profile");
        assert_eq!(v2.colour_reg, 0x8160);
        let v1 = profile_for("DIMM_LED-0102").expect("v1 profile");
        assert_eq!(v1.colour_reg, 0x8010);
        assert!(profile_for("not-ene").is_none());
    }

    #[test]
    fn brightness_scales_every_channel() {
        let c = Rgb::new(200, 100, 50);
        assert_eq!(c.scaled(100), c);
        assert_eq!(c.scaled(50), Rgb::new(100, 50, 25));
        // brightness has a 5% floor
        assert_eq!(Rgb::new(100, 0, 0).scaled(0), Rgb::new(5, 0, 0));
    }

    #[test]
    fn black_profiles_are_never_saved() {
        assert!(!Rgb::new(0, 0, 0).savable());
        assert!(!Rgb::new(15, 15, 15).savable());
        assert!(Rgb::new(16, 0, 0).savable());
    }
}
