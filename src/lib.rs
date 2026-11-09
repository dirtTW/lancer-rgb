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
        if ok == 0 {
            return Err(Error(format!("ioctl {code:#010X} failed")));
        }
        Ok(out)
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
        self.write_word(addr, 0x00, reg)
    }

    pub fn register_read(&self, addr: u8, reg: u16) -> Result<u8> {
        self.set_register(addr, reg)?;
        self.read_byte(addr, 0x81)
    }

    pub fn register_write(&self, addr: u8, reg: u16, value: u8) -> Result<()> {
        self.set_register(addr, reg)?;
        self.write_byte(addr, 0x01, value)
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
