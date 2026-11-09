# Lancer RGB

Minimal native Windows controller for **XPG Lancer Blade DDR5** RGB.
No ADATA account, no Electron, no background app.

Set a colour once; a 126 KB helper restores it at logon and exits.

| Binary | Size | Purpose |
| --- | --- | --- |
| `lancer.exe` | 234 KB | raw Win32/GDI window: colour picker, apply/save |
| `lancer-apply.exe` | 126 KB | one-shot restore used by the logon entry |

No UI framework: the window is user32 + gdi32, linked straight from Rust.

## Hardware

Tested with XPG Lancer Blade DDR5 16 GB (`AX5U6000C4816G-DTLABRWH`) on an
ASRock B650M-HDV/M.2. The strips are an **ENE SMBus RGB controller**
(`AUDA0-E6K5-0101`, 8 LEDs) at I2C `0x70` and `0x72`, not USB/HID.

## How it works

```
lancer.exe / lancer-apply.exe
        |
        +-- src/lib.rs         SMBus transport + ENE protocol
        +-- \\.\CTIIO          signed driver already installed by the vendor pack
                |
                +-- SMBus ---+-- DIMM 0x70
                             +-- DIMM 0x72
```

The vendor DLLs (`AacHal_x64.dll`, `MsIo64_ENE.dll`) and the vendor COM object
are not used or redistributed. The engine talks to the driver directly.

### Register map

| Register | Meaning |
| --- | --- |
| `0x1000` | device name, 16 ASCII bytes |
| `0x1C00` | config table (byte 2 = LED count) |
| `0x8100` | direct colours, R,B,G per LED |
| `0x8160` | effect colours, R,B,G per LED |
| `0x8021` | mode (1 = static) |
| `0x8022` | speed |
| `0x8023` | direction |
| `0x80A0` | `0x01` apply, `0xAA` save to module |

Register access is indirect: write the 16-bit register to command `0x00`, then
read/write command `0x01`; the pointer auto-increments. IOCTLs:
`0x80502148` read byte, `0x8050214C` write byte, `0x80502150` read word,
`0x80502154` write word. All input buffers are 12 bytes
(`bus, addr, cmd, ...`).

### Reliability notes

- Single SMBus transactions occasionally fail for one frame; the engine retries
  up to four times so a colour write is never left half-applied.
- 150 us settling delay after selecting a register, otherwise the pointer can
  desync.
- Hardware Save (`0x80A0 = 0xAA`) only kept the first colour on the tested kit,
  so it is issued but not relied on.

## Persistence

`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` runs
`lancer-apply.exe`, which reads `%APPDATA%\lancer-rgb\last.txt`, waits up to
10 s for the modules, applies the colour, saves, and exits. No window, no tray,
no resident process. The checkbox in the GUI owns that registry value.

## Build

```
cargo build --release
```

Release profile is tuned for size: `opt-level = "z"`, fat LTO, one codegen unit,
`panic = "abort"`, stripped symbols.

## Layout

```
src/lib.rs            engine (transport, protocol, config, one-shot restore)
src/bin/app.rs        Win32/GDI window
src/bin/apply.rs      tiny logon restorer
examples/probe.rs     headless diagnostic: cargo run --example probe -- A03CFF
build.rs              links user32 + gdi32
```

## Credits

Protocol knowledge derived from **[OpenRGB](https://openrgb.org)** by Adam Honse
(CalcProgrammer1), ENE SMBus controller (`Controllers/ENESMBusController`,
GPL-2.0-or-later).

- **Win32/GDI** - the entire UI is user32 + gdi32 called from Rust; no toolkit.
- **ENE Technology** - authors of the `CtiIo`/`AacHal`/`MsIo` components the
  vendor pack installs; none of their binaries are included here.

XPG, ADATA and Lancer are trademarks of their owners. Unofficial project.

## License

GPL-2.0-or-later. See `LICENSE`.
