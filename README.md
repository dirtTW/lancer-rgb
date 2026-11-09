# Lancer RGB

Standalone RGB control for **XPG Lancer Blade DDR5** (`AX5U6000C4816G-DTLABRWH`)
on Windows. No ADATA account, no Electron, no background service.

Replaces the XPG Prime workflow: set a colour, optionally save it into the
DIMM, and the module keeps it with nothing running.

## What the hardware actually is

The strips are not USB or HID devices — they are an **ENE DRAM RGB controller
on the motherboard SMBus**, reachable at I2C addresses `0x70`/`0x72` here.

```
XPG-Prime.exe (Electron)  ->  my_module / ene_hal_wrapper.node  -> AacHal_x64.dll
                                                                  (COM: ENE_DRAM_AIO.Hal.1,
                                                                   CLSID {84D8717B-4E1F-4607-9C1A-14E1BDB7B675})
                                                                        |
                                                              MsIo64_ENE.dll
                                                                        |
                                                          CtiIo64.sys  \\.\CTIIO
                                                                        |
                                                                    SMBus -> DIMM
```

Detected controller: **`AUDA0-E6K5-0101`**, 8 LEDs per module, 2 modules.
This is a second-generation ENE DRAM controller, the same family OpenRGB
calls "ENE SMBus".

## Register map used

| Register | Meaning |
| --- | --- |
| `0x1000` | device name, 16 ASCII bytes |
| `0x1C00` | 64-byte config table (byte 2 = LED count) |
| `0x8100` | direct colours, R,B,G per LED |
| `0x8160` | effect colours, R,B,G per LED |
| `0x8021` | mode (1 = static) |
| `0x8022` | speed |
| `0x8023` | direction |
| `0x80A0` | `0x01` apply, `0xAA` **save into module** |

SMBus access is register-indirect: write the 16-bit register to command `0x00`,
then read/write command `0x01`. The register pointer auto-increments, so a
whole colour set streams as 24 byte writes.

## Persistence

`0x80A0 = 0xAA` ("SaveMode") looks like it commits the effect to the module.
On this kit it only ever kept the **first** colour written and ignored later
saves: after a cold boot the sticks came back red (the first save), regardless
of what had been saved since. So hardware Save is issued, but is not relied on.

The reliable mechanism is a one-shot task at logon:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  LancerRGB = "lancer.exe" --apply-last
```

`--apply-last` reads the last colour from `%APPDATA%\lancer-rgb\last.txt`,
waits up to 10 s for the modules to answer, applies the colour, saves, and
exits. No window, no tray, no process left running, no login. This is the
gap XPG Prime filled by keeping a whole Electron app resident.

The GUI's "Re-apply my colour when Windows starts" checkbox owns that Run key,
so it can be turned off again.

## Layout

```
src/lib.rs         engine: CTIIO transport, ENE protocol, detect/set/save
src/bin/app.rs     GUI (Slint) + --apply-last startup mode
src/bin/probe.rs   headless dev harness used to verify the hardware
```

The engine talks to `\\.\CTIIO` directly with `DeviceIoControl`; it does not
load `MsIo64_ENE.dll` or `AacHal_x64.dll`. The only requirement is the signed
`CtiIo64.sys` driver that the vendor pack installs.

IOCTLs: read byte `0x80502148`, write byte `0x8050214C`, read word `0x80502150`,
write word `0x80502154`, read block `0x80502158`, write block `0x8050215C`.
All input buffers are 12 bytes: `{ bus, addr, cmd, ... }`.

## Build

```
cargo build --release
```

`lancer.exe` is the app; `lancer-probe.exe` is the diagnostic harness.
