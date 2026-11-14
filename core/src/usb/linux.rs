//! Linux / SteamOS implementation. Runs as root (pkexec). Never writes outside
//! /tmp and the selected stick – nothing in the (read-only) SteamOS root fs.

use super::*;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom};
use std::process::Command;
use std::time::Duration;

const CRITICAL: &[&str] = &["/", "/home", "/var", "/usr", "/boot", "/efi", "/etc", "/opt", "/srv", "/root", "/var/home", "/run/media/root", "/home/deck"];

fn read_trim(p: impl AsRef<Path>) -> String {
    std::fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn mounts() -> Vec<(String, String)> {
    read_trim("/proc/mounts")
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.to_string(), it.next()?.replace("\\040", " ")))
        })
        .collect()
}

fn disk_name(dev: &str) -> String {
    dev.trim_start_matches("/dev/").to_string()
}

fn belongs_to(source: &str, disk: &str) -> bool {
    let s = source.trim_start_matches("/dev/");
    s == disk || (s.starts_with(disk) && s[disk.len()..].trim_start_matches('p').chars().all(|c| c.is_ascii_digit()) && s.len() > disk.len())
}

fn is_usb(name: &str) -> bool {
    std::fs::canonicalize(format!("/sys/block/{name}")).map(|p| p.to_string_lossy().contains("/usb")).unwrap_or(false)
}

pub fn list_disks() -> R<Vec<Disk>> {
    let mut out = Vec::new();
    let rd = std::fs::read_dir("/sys/block").map_err(|e| e.to_string())?;
    let mnt = mounts();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if ["loop", "ram", "zram", "dm-", "sr", "md", "nbd", "fd"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let removable = read_trim(format!("/sys/block/{name}/removable")) == "1";
        let usb = is_usb(&name);
        if !removable && !usb {
            continue;
        }
        let size = read_trim(format!("/sys/block/{name}/size")).parse::<u64>().unwrap_or(0) * 512;
        if size == 0 {
            continue; // empty card reader
        }
        let mounts: Vec<String> = mnt.iter().filter(|(s, _)| belongs_to(s, &name)).map(|(_, m)| m.clone()).collect();
        if mounts.iter().any(|m| CRITICAL.contains(&m.as_str())) {
            continue; // never offer the running system
        }
        let vendor = read_trim(format!("/sys/block/{name}/device/vendor"));
        let model = read_trim(format!("/sys/block/{name}/device/model"));
        let mut label = format!("{vendor} {model}").trim().to_string();
        if label.is_empty() {
            label = if name.starts_with("mmcblk") { "SD-Karte / SD card".into() } else { "USB".into() };
        }
        out.push(Disk { id: format!("/dev/{name}"), name: label, size, removable, usb, mounts });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn sh(cmd: &mut Command) -> R<String> {
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    if !out.status.success() {
        return Err(format!(
            "{:?} {}: {}{}",
            cmd.get_program(),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn tool(name: &str, alts: &[&str]) -> R<PathBuf> {
    find_tool(name).or_else(|| alts.iter().find_map(|a| find_tool(a))).ok_or_else(|| format!("'{name}' fehlt / missing"))
}

fn validate(wa: &WorkerArgs, ctx: &mut Ctx) -> R<(String, u64)> {
    let dev = &wa.device;
    if !dev.starts_with("/dev/") || dev.contains("..") {
        return Err(format!("invalid device {dev}"));
    }
    let name = disk_name(dev);
    if !Path::new(&format!("/sys/block/{name}")).exists() {
        return Err(ctx.t(
            &format!("{dev} ist kein ganzes Laufwerk (bitte z. B. /dev/sdc statt /dev/sdc1)."),
            &format!("{dev} is not a whole disk (use e.g. /dev/sdc, not /dev/sdc1)."),
        ));
    }
    let removable = read_trim(format!("/sys/block/{name}/removable")) == "1";
    if !wa.force_any && !removable && !is_usb(&name) {
        return Err(ctx.t(&format!("{dev} ist kein USB-/Wechseldatenträger – aus Sicherheitsgründen abgebrochen."), &format!("{dev} is not a USB/removable drive – aborted for safety.")));
    }
    for (src, m) in mounts() {
        if belongs_to(&src, &name) && CRITICAL.contains(&m.as_str()) {
            return Err(ctx.t(&format!("{dev} enthält das laufende System ({m}). Abbruch!"), &format!("{dev} contains the running system ({m}). Aborting!")));
        }
    }
    // LVM / RAID / LUKS on top of it?
    if let Ok(rd) = std::fs::read_dir(format!("/sys/block/{name}")) {
        for e in rd.flatten() {
            let h = e.path().join("holders");
            if h.is_dir() && std::fs::read_dir(&h).map(|mut d| d.next().is_some()).unwrap_or(false) {
                return Err(ctx.t("Der Stick wird von LVM/RAID/Verschlüsselung benutzt. Abbruch!", "The stick is used by LVM/RAID/encryption. Aborting!"));
            }
        }
    }
    let size = read_trim(format!("/sys/block/{name}/size")).parse::<u64>().unwrap_or(0) * 512;
    if size < 1024 * 1024 * 1024 && !wa.force_any {
        return Err(ctx.t("Der Stick ist zu klein (unter 1 GB).", "The stick is too small (under 1 GB)."));
    }
    Ok((name, size))
}

fn unmount_disk(name: &str, ctx: &mut Ctx) {
    let mut ms: Vec<(String, String)> = mounts().into_iter().filter(|(s, _)| belongs_to(s, name)).collect();
    ms.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (src, m) in ms {
        ctx.log(format!("umount {src} ({m})"));
        if sh(Command::new("umount").arg(&m)).is_err() {
            let _ = sh(Command::new("umount").arg("-l").arg(&m));
        }
    }
    // swap on the stick?
    let _ = Command::new("swapoff").arg(format!("/dev/{name}")).output();
}

fn partition_path(dev: &str) -> String {
    if dev.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("{dev}p1")
    } else {
        format!("{dev}1")
    }
}

fn reread(dev: &str) {
    let _ = Command::new("blockdev").arg("--rereadpt").arg(dev).output();
    let _ = Command::new("partprobe").arg(dev).output();
    if let Some(u) = find_tool("udevadm") {
        let _ = Command::new(u).arg("settle").arg("--timeout=10").output();
    }
}

fn drop_caches() {
    unsafe_sync();
    let _ = std::fs::write("/proc/sys/vm/drop_caches", "3");
}

fn unsafe_sync() {
    let _ = Command::new("sync").output();
}

/// MBR with one FAT32 (LBA) partition from 1 MiB to the end, marked active.
/// No boot code on purpose: the stick boots in UEFI mode (required by Windows 11),
/// which also prevents setup from running in legacy BIOS mode by accident.
fn write_mbr(dev: &str, size: u64) -> R<()> {
    let mut f = OpenOptions::new().read(true).write(true).open(dev).map_err(|e| format!("{dev}: {e}"))?;
    let zero = vec![0u8; 1024 * 1024];
    f.write_all(&zero).map_err(|e| e.to_string())?;
    // wipe a backup GPT at the end of the disk, it would confuse firmware/Windows
    f.seek(SeekFrom::Start(size - zero.len() as u64)).map_err(|e| e.to_string())?;
    f.write_all(&zero).map_err(|e| e.to_string())?;
    let sectors = size / 512;
    let start: u64 = 2048;
    let count = (sectors - start) & !7; // keep 4K alignment
    let count = count.min(u32::MAX as u64) as u32;
    let mut mbr = vec![0u8; 512];
    let sig = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1) as u32) | 1;
    mbr[440..444].copy_from_slice(&sig.to_le_bytes());
    let e = 446;
    mbr[e] = 0x80; // active
    mbr[e + 1..e + 4].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
    mbr[e + 4] = 0x0C; // FAT32 LBA
    mbr[e + 5..e + 8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
    mbr[e + 8..e + 12].copy_from_slice(&(start as u32).to_le_bytes());
    mbr[e + 12..e + 16].copy_from_slice(&count.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    f.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    f.write_all(&mbr).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    Ok(())
}

struct TmpDir(PathBuf);
impl TmpDir {
    fn new(tag: &str) -> R<Self> {
        let p = std::env::temp_dir().join(format!("wie-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|e| e.to_string())?;
        Ok(TmpDir(p))
    }
}

struct Mounted(PathBuf);
impl Drop for Mounted {
    fn drop(&mut self) {
        unsafe_sync();
        if sh(Command::new("umount").arg(&self.0)).is_err() {
            std::thread::sleep(Duration::from_secs(1));
            let _ = sh(Command::new("umount").arg("-l").arg(&self.0));
        }
        let _ = std::fs::remove_dir(&self.0);
    }
}

pub fn run(wa: &WorkerArgs, ctx: &mut Ctx) -> R<String> {
    if read_trim("/proc/self/status").lines().find(|l| l.starts_with("Uid:")).map(|l| l.split_whitespace().nth(1) != Some("0")).unwrap_or(true) {
        return Err(ctx.t("Root-Rechte benötigt.", "Root privileges required."));
    }
    let (name, size) = validate(wa, ctx)?;
    ctx.progress(1.0, &ctx.t("Prüfe Stick", "Checking stick"));
    unmount_disk(&name, ctx);
    match wa.mode {
        Mode::Windows => windows_stick(wa, ctx, &name, size),
        Mode::Raw => raw_write(wa, ctx, &name, size),
    }
}

fn windows_stick(wa: &WorkerArgs, ctx: &mut Ctx, name: &str, size: u64) -> R<String> {
    let dev = format!("/dev/{name}");
    let plan = plan_windows_copy(&wa.image, ctx)?;
    if plan.total + 256 * 1024 * 1024 > size {
        return Err(ctx.t(
            &format!("Der Stick ist zu klein: benötigt {:.1} GB.", plan.total as f64 / 1e9),
            &format!("The stick is too small: {:.1} GB needed.", plan.total as f64 / 1e9),
        ));
    }
    let mkfs = tool("mkfs.fat", &["mkfs.vfat", "mkdosfs"])?;
    let wimlib = if plan.split.is_some() { Some(tool("wimlib-imagex", &[])?) } else { None };

    ctx.log(ctx.t("Schreibe Partitionstabelle (MBR, FAT32)", "Writing partition table (MBR, FAT32)"));
    ctx.progress(2.0, &ctx.t("Bereite Stick vor", "Preparing stick"));
    write_mbr(&dev, size)?;
    reread(&dev);
    let part = partition_path(&dev);
    let mut ok = false;
    for _ in 0..40 {
        if Path::new(&part).exists() {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
        reread(&dev);
    }
    if !ok {
        return Err(ctx.t("Die neue Partition ist nicht erschienen. Stick abziehen, neu einstecken und nochmal versuchen.", "The new partition did not appear. Unplug the stick, plug it in again and retry."));
    }
    std::thread::sleep(Duration::from_millis(500));
    // desktop automount might grab the empty partition – release it
    unmount_disk(name, ctx);
    ctx.log(ctx.t("Formatiere FAT32", "Formatting FAT32"));
    sh(Command::new(&mkfs).args(["-F", "32", "-n", LABEL]).arg(&part))?;
    unmount_disk(name, ctx);

    let usb = TmpDir::new("usb")?;
    sh(Command::new("mount").args(["-t", "vfat", "-o", "rw,noatime,flush"]).arg(&part).arg(&usb.0))?;
    let usb_m = Mounted(usb.0.clone());

    let split_share = plan.split.as_ref().map(|(_, s)| *s as f64 / (plan.total as f64).max(1.0)).unwrap_or(0.0);
    let copy_to = 5.0 + 75.0 * (1.0 - split_share);
    let sums = copy_iso_files(&wa.image, &usb_m.0, &plan, ctx, 5.0, copy_to)?;

    if let (Some((wim_path, wim_size)), Some(wimlib)) = (plan.split.clone(), wimlib) {
        ctx.log(ctx.t("install.wim ist größer als 4 GB → wird in install.swm-Teile aufgeteilt", "install.wim is larger than 4 GB → splitting into install.swm parts"));
        let isodir = TmpDir::new("iso")?;
        sh(Command::new("mount").args(["-o", "loop,ro", "-t", "udf"]).arg(&wa.image).arg(&isodir.0))
            .map_err(|e| ctx.t("ISO konnte nicht eingehängt werden: ", "Could not mount the ISO: ") + &e)?;
        let iso_m = Mounted(isodir.0.clone());
        let src = iso_m.0.join(&wim_path);
        let dst_dir = usb_m.0.join(Path::new(&wim_path).parent().unwrap_or(Path::new("sources")));
        std::fs::create_dir_all(&dst_dir).map_err(|e| e.to_string())?;
        let target = dst_dir.join("install.swm");
        let mut child = Command::new(&wimlib)
            .arg("split")
            .arg(&src)
            .arg(&target)
            .arg(split_mb().to_string())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("wimlib-imagex: {e}"))?;
        let stage = ctx.t("Teile install.wim auf", "Splitting install.wim");
        loop {
            if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
                let mut errtxt = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut errtxt);
                }
                if !st.success() {
                    return Err(format!("wimlib-imagex split: {errtxt}"));
                }
                break;
            }
            let written: u64 = std::fs::read_dir(&dst_dir)
                .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("install")).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum())
                .unwrap_or(0);
            ctx.progress(copy_to + (80.0 - copy_to) * (written as f64 / wim_size as f64).min(1.0), &stage);
            std::thread::sleep(Duration::from_millis(700));
        }
        drop(iso_m);
        let _ = std::fs::remove_dir(&isodir.0);
    }

    ctx.progress(80.0, &ctx.t("Schreibe Daten auf den Stick", "Flushing data to the stick"));
    unsafe_sync();
    drop_caches();
    verify_files(&usb_m.0, &sums, ctx, 80.0, 99.0)?;
    drop(usb_m);
    let _ = std::fs::remove_dir(&usb.0);
    unsafe_sync();
    Ok(ctx.t("Fertig! Der Stick ist bootfähig und geprüft – du kannst ihn jetzt abziehen.", "Done! The stick is bootable and verified – you can remove it now."))
}

fn raw_write(wa: &WorkerArgs, ctx: &mut Ctx, name: &str, size: u64) -> R<String> {
    let dev = format!("/dev/{name}");
    let src_size = std::fs::metadata(&wa.image).map_err(|e| e.to_string())?.len();
    let compressed = compression_of(&wa.image).is_some();
    if !compressed && src_size > size {
        return Err(ctx.t(
            &format!("Der Stick ist zu klein: Image {:.2} GB, Stick {:.2} GB.", src_size as f64 / 1e9, size as f64 / 1e9),
            &format!("The stick is too small: image {:.2} GB, stick {:.2} GB.", src_size as f64 / 1e9, size as f64 / 1e9),
        ));
    }
    let mut src = open_raw(&wa.image, ctx)?;
    let mut out = OpenOptions::new().write(true).open(&dev).map_err(|e| format!("{dev}: {e}"))?;
    let stage = ctx.t("Schreibe Image", "Writing image");
    let verify_to = if compressed { 99.0 } else { 70.0 };
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut written = 0u64;
    let mut since_sync = 0u64;
    let mut crc = Crc32::new();
    loop {
        let n = read_full(&mut src.reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        if written + n as u64 > size {
            return Err(ctx.t("Der Stick ist zu klein für dieses Image.", "The stick is too small for this image."));
        }
        out.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", ctx.t("Schreibfehler", "write error")))?;
        crc.update(&buf[..n]);
        written += n as u64;
        since_sync += n as u64;
        if since_sync >= 64 * 1024 * 1024 {
            out.sync_data().map_err(|e| e.to_string())?;
            since_sync = 0;
        }
        let consumed = src.consumed.load(std::sync::atomic::Ordering::Relaxed) as f64;
        let frac = if compressed { consumed / src.total_input.max(1) as f64 } else { written as f64 / src_size.max(1) as f64 };
        ctx.progress(2.0 + (verify_to - 3.0) * frac.min(1.0), &stage);
    }
    if let Some(mut c) = src.child.take() {
        let st = c.wait().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err(ctx.t("Entpacken fehlgeschlagen – Datei beschädigt?", "Unpacking failed – file damaged?"));
        }
    }
    out.sync_all().map_err(|e| e.to_string())?;
    drop(out);
    ctx.log(format!("{:.2} GB {}", written as f64 / 1e9, ctx.t("geschrieben", "written")));
    let _ = Command::new("blockdev").arg("--flushbufs").arg(&dev).output();
    drop_caches();

    // verify by reading the stick back
    let stage = ctx.t("Prüfe Stick", "Verifying stick");
    let mut f = File::open(&dev).map_err(|e| e.to_string())?;
    let mut c2 = Crc32::new();
    let mut left = written;
    while left > 0 {
        let n = left.min(buf.len() as u64) as usize;
        f.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
        c2.update(&buf[..n]);
        left -= n as u64;
        ctx.progress(verify_to + (99.0 - verify_to) * ((written - left) as f64 / written.max(1) as f64), &stage);
    }
    if c2.finish() != crc.finish() {
        return Err(ctx.t(
            "Prüfung fehlgeschlagen: Die Daten auf dem Stick stimmen nicht. Stick defekt oder gefälscht? Anderen Stick/USB-Port versuchen.",
            "Verification failed: data on the stick does not match. Faulty or fake stick? Try another stick/USB port.",
        ));
    }
    reread(&dev);
    Ok(ctx.t("Fertig! Image geschrieben und geprüft – du kannst den Stick jetzt abziehen.", "Done! Image written and verified – you can remove the stick now."))
}
