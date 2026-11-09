# Lancer RGB

> ## ⚠️ USE AT YOUR OWN RISK — THIS CAN KILL YOUR RAM's LEDs
>
> This is unofficial, reverse-engineered software. It writes directly to the
> RGB controller on the memory modules over the motherboard SMBus, using
> undocumented registers.
>
> **During development of this project, the LEDs on the author's own two XPG
> Lancer Blade DDR5 sticks stopped working and never came back.** XPG Prime
> could no longer light them either. A full PSU power cut (standby removed),
> cold boot and reseating did not restore them. The exact cause was never
> confirmed — but it happened while this code was being written against the
> modules, and you should assume the risk is real.
>
> What that means for you:
>
> - Writing RGB registers is **not** guaranteed safe on your modules.
> - A failed write, a latched controller or a corrupted saved profile may
>   leave your sticks permanently dark.
> - **No warranty. No liability. No support for damaged hardware.**
> - If the RGB on your RAM matters to you, or the kit is expensive and hard
>   to replace, **do not run this.**
>
> The author is not RMA-ing and is not claiming the software is the proven
> cause — only that you accept this risk yourself if you use it.

Minimal native Windows controller for **XPG Lancer Blade DDR5** RGB.
No ADATA account, no Electron, no background app.

Set a colour once; a 126 KB helper restores it at logon and exits.

| Binary | Size | Purpose |
| --- | --- | --- |
| `lancer.exe` | 4.7 MB | GPUI window: colour picker, apply/save |
| `lancer-apply.exe` | 126 KB | one-shot restore used by the logon entry |

The GUI deliberately uses GPUI rather than a hand-rolled Win32 window: GPUI
handles DPI scaling and input correctly on this machine, which a raw
win32/GDI implementation did not.

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
- Near-black colours are applied live but **never saved**, so a black profile
  cannot be persisted to the module.
- **Observed failure:** on the development kit the LEDs stopped lighting
  entirely and did not recover after a PSU power cut, cold boot or reseat.
  XPG Prime also failed to light them. Cause unconfirmed. See the warning at the
  top of this file.

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
src/bin/app.rs        GPUI window
src/bin/apply.rs      tiny logon restorer
examples/probe.rs     headless diagnostic: cargo run --example probe -- A03CFF
```

## Credits

Protocol knowledge derived from **[OpenRGB](https://openrgb.org)** by Adam Honse
(CalcProgrammer1), ENE SMBus controller (`Controllers/ENESMBusController`,
GPL-2.0-or-later).

- **GPUI / gpui-pre** - Rust UI framework; handles DPI and input.
- **ENE Technology** - authors of the `CtiIo`/`AacHal`/`MsIo` components the
  vendor pack installs; none of their binaries are included here.

XPG, ADATA and Lancer are trademarks of their owners. Unofficial project.

## License

GPL-2.0-or-later. See `LICENSE`.

This program is distributed in the hope that it will be useful, but **WITHOUT
ANY WARRANTY**; without even the implied warranty of MERCHANTABILITY or FITNESS
FOR A PARTICULAR PURPOSE. The authors accept no liability for damaged memory,
lost data or hardware that stops working. See the warning at the top of this
file and sections 15–16 of the GPL.
