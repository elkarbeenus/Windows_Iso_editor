//! Windows implementation. The worker runs elevated (UAC) and uses the
//! built-in tools diskpart, PowerShell (Storage module) and DISM.

use super::*;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::Duration;

const NO_WINDOW: u32 = 0x0800_0000;

fn ps(script: &str) -> R<String> {
    let full = format!("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'Stop'; {script}");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("powershell: {e}"))?;
    if !out.status.success() {
        return Err(format!("{}{}", String::from_utf8_lossy(&out.stderr).trim(), String::from_utf8_lossy(&out.stdout).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn list_disks() -> R<Vec<Disk>> {
    let script = r#"
$sys = -1
try { $sys = (Get-Partition -DriveLetter ($env:SystemDrive.Substring(0,1)) -ErrorAction Stop).DiskNumber } catch {}
Get-CimInstance Win32_DiskDrive | Where-Object { $_.InterfaceType -eq 'USB' -or "$($_.MediaType)" -like '*Removable*' -or "$($_.PNPDeviceID)" -like 'USBSTOR*' } | ForEach-Object {
  if ($_.Index -ne $sys) {
    $letters = @()
    try { $letters = @(Get-CimAssociatedInstance -InputObject $_ -ResultClassName Win32_DiskPartition | ForEach-Object { Get-CimAssociatedInstance -InputObject $_ -ResultClassName Win32_LogicalDisk } | ForEach-Object { $_.DeviceID }) } catch {}
    "{0}|{1}|{2}|{3}|{4}" -f $_.Index, $_.Model, $_.Size, $_.MediaType, ($letters -join ',')
  }
}
"#;
    let out = ps(script)?;
    let mut v = Vec::new();
    for line in out.lines() {
        let p: Vec<&str> = line.trim().split('|').collect();
        if p.len() < 5 {
            continue;
        }
        let size = p[2].trim().parse::<u64>().unwrap_or(0);
        if size == 0 {
            continue;
        }
        v.push(Disk {
            id: p[0].trim().to_string(),
            name: p[1].trim().to_string(),
            size,
            removable: p[3].contains("Removable"),
            usb: true,
            mounts: p[4].split(',').filter(|s| !s.is_empty()).map(|s| format!("{s}\\")).collect(),
        });
    }
    Ok(v)
}

fn diskpart(lines: &[String]) -> R<String> {
    let path = std::env::temp_dir().join(format!("wie-diskpart-{}.txt", std::process::id()));
    std::fs::write(&path, lines.join("\r\n") + "\r\n").map_err(|e| e.to_string())?;
    let out = Command::new("diskpart.exe").arg("/s").arg(&path).creation_flags(NO_WINDOW).output().map_err(|e| format!("diskpart: {e}"))?;
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        return Err(format!("diskpart ({}): {}", out.status, text.lines().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" / ")));
    }
    Ok(text)
}

struct DiskInfo {
    size: u64,
    bus: String,
}

fn validate(wa: &WorkerArgs, ctx: &mut Ctx) -> R<(u32, DiskInfo)> {
    let n: u32 = wa.device.trim().parse().map_err(|_| format!("invalid disk number {}", wa.device))?;
    let out = ps(&format!("$d = Get-Disk -Number {n}; \"{{0}}|{{1}}|{{2}}|{{3}}\" -f $d.BusType, $d.IsSystem, $d.IsBoot, $d.Size"))?;
    let p: Vec<&str> = out.trim().split('|').collect();
    if p.len() < 4 {
        return Err(format!("Get-Disk: {out}"));
    }
    let info = DiskInfo { bus: p[0].to_string(), size: p[3].trim().parse().unwrap_or(0) };
    if p[1].eq_ignore_ascii_case("true") || p[2].eq_ignore_ascii_case("true") {
        return Err(ctx.t("Das ist das Systemlaufwerk – Abbruch!", "This is the system drive – aborting!"));
    }
    if !wa.force_any && !["USB", "SD", "MMC"].iter().any(|b| info.bus.eq_ignore_ascii_case(b)) {
        return Err(ctx.t(
            &format!("Laufwerk {n} ist kein USB-Laufwerk ({}) – aus Sicherheitsgründen abgebrochen.", info.bus),
            &format!("Disk {n} is not a USB drive ({}) – aborted for safety.", info.bus),
        ));
    }
    if info.size < 1024 * 1024 * 1024 && !wa.force_any {
        return Err(ctx.t("Der Stick ist zu klein (unter 1 GB).", "The stick is too small (under 1 GB)."));
    }
    Ok((n, info))
}

pub fn run(wa: &WorkerArgs, ctx: &mut Ctx) -> R<String> {
    let (n, info) = validate(wa, ctx)?;
    ctx.progress(1.0, &ctx.t("Prüfe Stick", "Checking stick"));
    match wa.mode {
        Mode::Windows => windows_stick(wa, ctx, n, &info),
        Mode::Raw => raw_write(wa, ctx, n, &info),
    }
}

fn windows_stick(wa: &WorkerArgs, ctx: &mut Ctx, n: u32, info: &DiskInfo) -> R<String> {
    let plan = plan_windows_copy(&wa.image, ctx)?;
    if plan.total + 256 * 1024 * 1024 > info.size {
        return Err(ctx.t(
            &format!("Der Stick ist zu klein: benötigt {:.1} GB.", plan.total as f64 / 1e9),
            &format!("The stick is too small: {:.1} GB needed.", plan.total as f64 / 1e9),
        ));
    }
    ctx.log(ctx.t("Partitioniere und formatiere (MBR, FAT32)", "Partitioning and formatting (MBR, FAT32)"));
    ctx.progress(2.0, &ctx.t("Bereite Stick vor", "Preparing stick"));
    // Windows formats FAT32 only up to 32 GB – more is never needed for setup files
    let disk_mb = info.size / (1024 * 1024);
    let size_line = if disk_mb > 32000 { "create partition primary size=32000".to_string() } else { "create partition primary".to_string() };
    let script = vec![
        format!("select disk {n}"),
        "clean".into(),
        "convert mbr".into(),
        size_line,
        "select partition 1".into(),
        "active".into(),
        format!("format fs=fat32 quick label={LABEL}"),
        "assign".into(),
        "exit".into(),
    ];
    if let Err(e) = diskpart(&script) {
        ctx.log(format!("diskpart: {e} – {}", ctx.t("zweiter Versuch", "retrying")));
        std::thread::sleep(Duration::from_secs(3));
        diskpart(&script)?;
    }
    let mut letter = String::new();
    for _ in 0..30 {
        if let Ok(o) = ps(&format!("(Get-Partition -DiskNumber {n} -PartitionNumber 1).DriveLetter")) {
            let l = o.trim().to_string();
            if l.len() == 1 && l.chars().all(|c| c.is_ascii_alphabetic()) {
                letter = l;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    if letter.is_empty() {
        return Err(ctx.t("Der Stick hat keinen Laufwerksbuchstaben bekommen. Bitte neu einstecken und nochmal versuchen.", "The stick got no drive letter. Please replug it and try again."));
    }
    let root = PathBuf::from(format!("{letter}:\\"));
    ctx.log(format!("USB: {}", root.display()));

    let split_share = plan.split.as_ref().map(|(_, s)| *s as f64 / (plan.total as f64).max(1.0)).unwrap_or(0.0);
    let copy_to = 5.0 + 75.0 * (1.0 - split_share);
    let sums = copy_iso_files(&wa.image, &root, &plan, ctx, 5.0, copy_to)?;

    if let Some((wim_path, wim_size)) = plan.split.clone() {
        ctx.log(ctx.t("install.wim ist größer als 4 GB → wird mit DISM aufgeteilt", "install.wim is larger than 4 GB → splitting with DISM"));
        let iso = wa.image.to_string_lossy().to_string();
        let out = ps(&format!("$m = Mount-DiskImage -ImagePath {} -PassThru; ($m | Get-Volume).DriveLetter", ps_quote(&iso)))
            .map_err(|e| ctx.t("ISO konnte nicht eingebunden werden: ", "Could not mount the ISO: ") + &e)?;
        let iso_letter = out.lines().map(|l| l.trim()).find(|l| l.len() == 1).unwrap_or("").to_string();
        let result = (|| -> R<()> {
            if iso_letter.is_empty() {
                return Err("Mount-DiskImage: no drive letter".into());
            }
            let src = format!("{iso_letter}:\\{}", wim_path.replace('/', "\\"));
            let dst_dir = root.join("sources");
            std::fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
            let swm = dst_dir.join("install.swm");
            let mut child = Command::new("dism.exe")
                .arg("/Split-Image")
                .arg(format!("/ImageFile:{src}"))
                .arg(format!("/SWMFile:{}", swm.display()))
                .arg(format!("/FileSize:{}", split_mb()))
                .creation_flags(NO_WINDOW)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("dism: {e}"))?;
            let stage = ctx.t("Teile install.wim auf", "Splitting install.wim");
            loop {
                if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
                    if !st.success() {
                        let mut t = String::new();
                        if let Some(mut o) = child.stdout.take() {
                            let _ = o.read_to_string(&mut t);
                        }
                        return Err(format!("DISM: {}", t.lines().rev().take(6).collect::<Vec<_>>().join(" / ")));
                    }
                    break;
                }
                let written: u64 = std::fs::read_dir(&dst_dir)
                    .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".swm")).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
                    .unwrap_or(0);
                ctx.progress(copy_to + (80.0 - copy_to) * (written as f64 / wim_size as f64).min(1.0), &stage);
                std::thread::sleep(Duration::from_millis(700));
            }
            Ok(())
        })();
        let _ = ps(&format!("Dismount-DiskImage -ImagePath {} | Out-Null", ps_quote(&iso)));
        result?;
    }

    verify_files(&root, &sums, ctx, 80.0, 99.0)?;
    Ok(ctx.t("Fertig! Der Stick ist bootfähig und geprüft – du kannst ihn jetzt abziehen.", "Done! The stick is bootable and verified – you can remove it now."))
}

fn raw_write(wa: &WorkerArgs, ctx: &mut Ctx, n: u32, info: &DiskInfo) -> R<String> {
    if compression_of(&wa.image).is_some() {
        return Err(ctx.t(
            "Gepackte Images (.bz2, .gz, .xz, .zip) bitte unter Windows zuerst entpacken (z. B. mit 7-Zip) und dann die .img/.iso wählen.",
            "Please unpack compressed images (.bz2, .gz, .xz, .zip) first on Windows (e.g. with 7-Zip) and select the .img/.iso.",
        ));
    }
    let src_size = std::fs::metadata(&wa.image).map_err(|e| e.to_string())?.len();
    if src_size > info.size {
        return Err(ctx.t(
            &format!("Der Stick ist zu klein: Image {:.2} GB, Stick {:.2} GB.", src_size as f64 / 1e9, info.size as f64 / 1e9),
            &format!("The stick is too small: image {:.2} GB, stick {:.2} GB.", src_size as f64 / 1e9, info.size as f64 / 1e9),
        ));
    }
    ctx.progress(2.0, &ctx.t("Bereite Stick vor", "Preparing stick"));
    // remove all partitions so Windows has no volume mounted on the stick
    diskpart(&[format!("select disk {n}"), "clean".into(), "exit".into()])?;
    std::thread::sleep(Duration::from_secs(1));

    let dev = format!("\\\\.\\PhysicalDrive{n}");
    let mut src = File::open(&wa.image).map_err(|e| e.to_string())?;
    let mut out = OpenOptions::new().read(true).write(true).open(&dev).map_err(|e| format!("{dev}: {e}"))?;
    const HEAD: usize = 1024 * 1024;
    let mut head = vec![0u8; HEAD];
    let head_n = read_full(&mut src, &mut head).map_err(|e| e.to_string())?;
    let stage = ctx.t("Schreibe Image", "Writing image");
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut crc = Crc32::new();
    crc.update(&head[..head_n]);
    let mut written = head_n as u64;
    // The partition table (first MiB) is written last, so Windows does not
    // mount the new partitions while we are still writing.
    out.seek(SeekFrom::Start(HEAD as u64)).map_err(|e| e.to_string())?;
    if head_n == HEAD {
        loop {
            let got = read_full(&mut src, &mut buf).map_err(|e| e.to_string())?;
            if got == 0 {
                break;
            }
            crc.update(&buf[..got]);
            let padded = got.div_ceil(512) * 512;
            buf[got..padded].fill(0);
            out.write_all(&buf[..padded]).map_err(|e| format!("{}: {e}", ctx.t("Schreibfehler", "write error")))?;
            written += got as u64;
            ctx.progress(2.0 + 66.0 * (written as f64 / src_size.max(1) as f64), &stage);
        }
    }
    let head_padded = head_n.div_ceil(512) * 512;
    head[head_n..head_padded.max(head_n)].fill(0);
    out.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    out.write_all(&head[..head_padded]).map_err(|e| e.to_string())?;
    out.sync_all().map_err(|e| e.to_string())?;

    // verify
    let stage = ctx.t("Prüfe Stick", "Verifying stick");
    out.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut c2 = Crc32::new();
    let mut left = written;
    while left > 0 {
        let want = (left.min(buf.len() as u64)) as usize;
        let rd = want.div_ceil(512) * 512;
        out.read_exact(&mut buf[..rd]).map_err(|e| e.to_string())?;
        c2.update(&buf[..want]);
        left -= want as u64;
        ctx.progress(70.0 + 29.0 * ((written - left) as f64 / written.max(1) as f64), &stage);
    }
    drop(out);
    let _ = ps(&format!("Update-Disk -Number {n}"));
    if c2.finish() != crc.finish() {
        return Err(ctx.t(
            "Prüfung fehlgeschlagen: Die Daten auf dem Stick stimmen nicht. Stick defekt oder gefälscht? Anderen Stick/USB-Port versuchen.",
            "Verification failed: data on the stick does not match. Faulty or fake stick? Try another stick/USB port.",
        ));
    }
    Ok(ctx.t("Fertig! Image geschrieben und geprüft – du kannst den Stick jetzt abziehen.", "Done! Image written and verified – you can remove the stick now."))
}
