<p align="center"><img src="src-tauri/icons/128x128.png" width="96" alt="logo"></p>

<h1 align="center">Windows ISO Editor</h1>
<p align="center"><b>Debloat Windows 11 installation ISOs – safely.</b><br>
Remove Copilot, Recall, Cortana, ads and bloatware before Windows is even installed –<br>
and write the result (or any Windows / SteamOS / Linux image) to a bootable USB stick.<br>
Linux AppImage (SteamOS) · Windows exe · x64 & ARM64 ISOs</p>
<p align="center"><i>Created by Dominic Lesina × Claude</i></p>

---

## What it does

1. **Choose ISO**: pick an original Windows 11 ISO (x64 or ARM64) or drag & drop it.
2. **Customize**: start with *Gentle*, *Recommended* or *Maximum* and toggle cards. Every card explains what it does, and *“What exactly happens?”* shows the exact registry keys and packages.
3. **Create**: the app writes a new ISO next to the original and checks the result.

Then click **“Write to USB stick now”** (or use Rufus/Ventoy) and install Windows as usual.

## Bootable USB sticks

The second mode on the start page creates boot sticks from any image:

| Image | How it is written |
|---|---|
| Windows 10/11 ISO (x64, ARM64) | MBR + FAT32 (UEFI), like Microsoft's Media Creation Tool. `install.wim` > 4 GB is split into `install.swm` (Linux: bundled wimlib, Windows: DISM) |
| Linux hybrid ISOs | 1:1 (like dd / Etcher) |
| SteamOS recovery `.img` / `.img.bz2` | 1:1, decompressed on the fly (Linux also `.gz`, `.xz`, `.zst`, `.zip`) |

Everything written is read back and compared, so faulty or fake sticks are caught right away. Only USB/removable drives are offered, never the system drive. Root rights come from pkexec on Linux/SteamOS (the user needs a password, `passwd`) and from a UAC prompt on Windows.

## Why the installation can't break

Unlike tools that cut components out of `install.wim`, Windows ISO Editor **does not modify the installer**:

- The original ISO is copied **bit for bit**. Only an `autounattend.xml` is appended to the UDF file system (new data at the end of the image plus 5 metadata sectors). Boot catalog, `boot.wim`, `install.wim`, `efisys.bin` and `bootmgr` stay byte-identical, and the app verifies this after writing.
- The answer file contains **no disk, product key, edition or account settings**. Setup asks you everything exactly as it would without the file.
- Clean-up runs inside the freshly installed Windows (specialize pass, as SYSTEM). **Every step is isolated** (`try/catch`, logged), and the script always exits successfully, so a failing step can never stop setup.
- A **hard-coded protection list** in the script refuses to remove Xbox / Game Bar / Gaming Services, Microsoft Store & winget, Windows Security, runtimes (.NET, VC++, WebView2), codecs and core apps. Bluetooth, drivers, Windows Update and Edge are never touched (Edge only gets its ads switched off).

CI proves on every build that **Windows itself** (Mount-DiskImage) and the **Linux kernel** UDF driver read the patched ISO file-for-file identical to the original.

Log files: `C:\Windows\Setup\Scripts\WinIsoEditor\specialize.log`, plus `%TEMP%\WinIsoEditor-firstlogon.log` for the first user.

## Options (excerpt)

| Area | Examples |
|---|---|
| AI & surveillance | Copilot (app + policy + Edge sidebar), Recall & Click to Do, Cortana, AI in Paint/Notepad |
| Privacy | minimal telemetry, advertising ID, activity history, Bing web search in Start, telemetry services |
| Ads & suggestions | sponsored apps (Candy Crush & co.), Start/lock screen ads, widgets, Edge clutter, Teams, new Outlook / Dev Home (incl. re-install prevention) |
| Preinstalled apps | News, Weather, Bing, Tips, M365 promo app, Solitaire, Clipchamp, To Do, Feedback Hub, Maps, Phone Link, Media Player, OneDrive … |
| Installation | local account without internet, bypass TPM/Secure Boot/CPU/RAM check, no automatic BitLocker |
| Comfort | file extensions, classic context menu, Start on the left, fast startup off (dual boot) |

## Downloads

See [Releases](../../releases):

- `Windows-ISO-Editor_<ver>_x86_64.AppImage`: Linux / SteamOS (`chmod +x`, then run)
- `Windows-ISO-Editor_<ver>_x64-setup.exe`: Windows installer
- `Windows-ISO-Editor_<ver>_x64-portable.exe`: Windows, no installation

## Building

```bash
npm install
npx tauri build            # Tauri v1, Rust stable
cargo test --manifest-path core/Cargo.toml   # UDF patcher + answer-file tests (needs python3)
```

`core/` contains the dependency-free engine (UDF reader/patcher, catalog, answer-file generator) and a CLI:
`cargo run --example wie-cli -- build in.iso out.iso recommended`.

## Deutsch (kurz)

Wähle eine originale Windows-11-ISO, klicke an, was raus soll (Copilot, Recall, Werbung, Apps …), und erstelle eine neue ISO. Der Windows-Installer bleibt unverändert. Es wird nur eine Antwortdatei ergänzt, deine Auswahl wird erst im frisch installierten Windows angewendet, und Gaming, Bluetooth, Store und Updates sind geschützt.

## License

MIT
