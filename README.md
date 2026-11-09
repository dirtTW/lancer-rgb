# Lancer RGB

A small native Windows app to control **XPG Lancer Blade DDR5** RGB without
XPG Prime, without an ADATA account, and without an Electron runtime.

Set a colour once. It is restored automatically at every Windows logon.

```
Windows logon -> lancer.exe --apply-last -> write colour -> exit
```

No window, no tray icon, no background process, no login screen.

## Why this exists

XPG Prime requires an account, ships a ~700 MB Electron bundle, and keeps a
resident app to maintain the lighting. The RAM itself is a standard ENE SMBus
RGB controller, so all of that is unnecessary on Windows.

## Supported hardware

Tested with:

| Item | Value |
| --- | --- |
| Memory | XPG Lancer Blade DDR5 16 GB, `AX5U6000C4816G-DTLABRWH` (2 modules) |
| RGB controller | ENE `AUDA0-E6K5-0101` (second-generation DRAM controller) |
| Board | ASRock B650M-HDV/M.2, Ryzen 5 7600 |
| Driver | `CtiIo64.sys` (`\\.\CTIIO`), installed by the XPG/ENE DRAM RGB pack |

Other ENE DRAM modules using the same controller family should work; the
engine currently lists the second-generation register set (`0x8100`/`0x8160`)
and the first-generation one (`0x8000`/`0x8010`).

## How it works

```
lancer.exe
   |
   +-- src/lib.rs    SMBus transport + ENE register protocol
   |
   +-- \\.\CTIIO    signed kernel driver (already present on the system)
           |
           +-- SMBus --+-- DIMM 0x70  (AUDA0-E6K5-0101, 8 LEDs)
                       +-- DIMM 0x72  (AUDA0-E6K5-0101, 8 LEDs)
```

The vendor DLLs (`AacHal_x64.dll`, `MsIo64_ENE.dll`) and the vendor's COM
object are **not** used or redistributed. The engine issues `DeviceIoControl`
requests to the driver directly.

### Register map

| Register | Meaning |
| --- | --- |
| `0x1000` | device name, 16 ASCII bytes |
| `0x1C00` | 64-byte configuration table (byte 2 = LED count) |
| `0x8100` | direct colours, R,B,G per LED |
| `0x8160` | effect colours, R,B,G per LED |
| `0x8021` | mode (1 = static) |
| `0x8022` | speed |
| `0x8023` | direction |
| `0x80A0` | `0x01` apply, `0xAA` save to module |

SMBus access is register-indirect: write the 16-bit register address to
command `0x00`, then read/write command `0x01`. The register pointer
auto-increments, so a whole colour set streams as 24 byte writes.

IOCTLs used: read byte `0x80502148`, write byte `0x8050214C`,
read word `0x80502150`, write word `0x80502154`,
read block `0x80502158`, write block `0x8050215C`.
All input buffers are 12 bytes: `{ bus, addr, cmd, ... }`.

### About persistence

`0x80A0 = 0xAA` is the module's Save command. On the tested kit it only kept
the **first** colour ever saved and ignored subsequent saves, so hardware Save
is issued but not relied on.

The reliable path is the logon entry:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  LancerRGB = "lancer.exe" --apply-last
```

`--apply-last` reads `%APPDATA%\lancer-rgb\last.txt`, waits up to 10 seconds
for the modules to answer, applies the colour, saves, and exits. The GUI's
checkbox owns that registry value, so it can be disabled again.

## Build

Requires Rust (MSVC toolchain).

```
cargo build --release
```

Outputs:

- `target/release/lancer.exe` — the GUI app
- `target/release/lancer-probe.exe` — headless diagnostic harness

## Install

```
copy target\release\lancer.exe %LOCALAPPDATA%\LancerRGB\lancer.exe
```

Then create a shortcut to it, or tick **Re-apply my colour when Windows starts**
in the app to register the logon task.

## Using it

- Pick a colour with the sliders or a preset.
- **Apply** — sets the colour and remembers it.
- **Apply + save to DIMM** — also issues the module Save command.

## Credits

Protocol knowledge and the ENE register map were derived from
**[OpenRGB](https://openrgb.org)** by Adam Honse (CalcProgrammer1), specifically
its ENE SMBus controller implementation (`Controllers/ENESMBusController`,
GPL-2.0-or-later). Thanks to that project for documenting a bus that vendors
leave undocumented.

- **Slint** — the Rust UI toolkit used for the window.
- **ENE Technology** — authors of the `CtiIo`/`AacHal`/`MsIo` components that
  the vendor pack installs. None of their binaries are included here.
- The ASUS AURA HAL ecosystem documentation, which clarified the relationship
  between the AURA, ENE HAL and DRAM controller layers.

XPG, ADATA and Lancer are trademarks of their respective owners. This project
is unofficial and not affiliated with or endorsed by them.

## License

GPL-2.0-or-later, to stay compatible with the OpenRGB work this builds on.
See `LICENSE`.
