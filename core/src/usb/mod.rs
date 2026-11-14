//! Bootable USB sticks: Windows installation ISOs (FAT32, install.wim split if
//! needed – same layout as Microsoft's Media Creation Tool) and raw images
//! (Linux hybrid ISOs, SteamOS recovery images .img/.img.bz2 …).
//!
//! The writing part runs in a separate, elevated "worker" process
//! (`--usb-worker`, started via pkexec on Linux / UAC on Windows) and reports
//! progress as JSON lines.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::crc32::Crc32;
use crate::udf::Udf;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

pub const FAT32_MAX: u64 = 4 * 1024 * 1024 * 1024 - 1;
pub const SPLIT_MB: u64 = 3800;

pub fn split_mb() -> u64 {
    std::env::var("WIE_SPLIT_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(SPLIT_MB)
}
pub const LABEL: &str = "WIN_INSTALL";

#[derive(Debug, Clone)]
pub struct Disk {
    /// "/dev/sdc" on Linux, disk number ("2") on Windows
    pub id: String,
    pub name: String,
    pub size: u64,
    pub removable: bool,
    pub usb: bool,
    /// mount points currently in use (informational)
    pub mounts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Windows,
    Raw,
}

#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub mode: Mode,
    /// "windows" | "linux" | "steamos" | "image"
    pub kind: String,
    pub label: String,
    pub size: u64,
    pub compressed: Option<String>,
    /// space needed on the stick (0 = unknown, compressed)
    pub needed: u64,
}

#[derive(Debug, Clone)]
pub enum Event {
    Log(String),
    Progress { pct: f64, stage: String },
    Done(String),
    Error(String),
}

fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => {}
            '\t' => o.push(' '),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl Event {
    pub fn to_json(&self) -> String {
        match self {
            Event::Log(m) => format!("{{\"t\":\"log\",\"msg\":{}}}", js(m)),
            Event::Progress { pct, stage } => format!("{{\"t\":\"progress\",\"pct\":{:.2},\"stage\":{}}}", pct, js(stage)),
            Event::Done(m) => format!("{{\"t\":\"done\",\"msg\":{}}}", js(m)),
            Event::Error(m) => format!("{{\"t\":\"error\",\"msg\":{}}}", js(m)),
        }
    }
}

pub struct Ctx<'a> {
    pub de: bool,
    pub emit: &'a mut dyn FnMut(Event),
    last_pct: f64,
    last_t: std::time::Instant,
}

impl<'a> Ctx<'a> {
    pub fn new(de: bool, emit: &'a mut dyn FnMut(Event)) -> Self {
        Ctx { de, emit, last_pct: -1.0, last_t: std::time::Instant::now() }
    }
    pub fn t(&self, de: &str, en: &str) -> String {
        if self.de { de.to_string() } else { en.to_string() }
    }
    pub fn log(&mut self, m: impl Into<String>) {
        (self.emit)(Event::Log(m.into()));
    }
    pub fn progress(&mut self, pct: f64, stage: &str) {
        let pct = pct.clamp(0.0, 100.0);
        if (pct - self.last_pct).abs() >= 0.2 || self.last_t.elapsed().as_millis() > 700 || pct >= 100.0 {
            self.last_pct = pct;
            self.last_t = std::time::Instant::now();
            (self.emit)(Event::Progress { pct, stage: stage.to_string() });
        }
    }
}

pub type R<T> = Result<T, String>;

// ------------------------------------------------------------------ detection

fn magic(path: &Path) -> Vec<u8> {
    let mut b = vec![0u8; 8];
    if let Ok(mut f) = File::open(path) {
        let n = f.read(&mut b).unwrap_or(0);
        b.truncate(n);
    }
    b
}

pub fn compression_of(path: &Path) -> Option<&'static str> {
    let m = magic(path);
    if m.starts_with(b"BZh") {
        Some("bz2")
    } else if m.starts_with(&[0x1f, 0x8b]) {
        Some("gz")
    } else if m.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
        Some("xz")
    } else if m.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        Some("zst")
    } else if m.starts_with(b"PK\x03\x04") {
        Some("zip")
    } else {
        None
    }
}

/// ISO9660 volume label (sector 16), used to name Linux ISOs.
fn iso9660_label(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut b = vec![0u8; 2048];
    use std::io::Seek;
    f.seek(std::io::SeekFrom::Start(16 * 2048)).ok()?;
    f.read_exact(&mut b).ok()?;
    if &b[1..6] != b"CD001" {
        return None;
    }
    Some(String::from_utf8_lossy(&b[40..72]).trim().to_string())
}

pub fn detect_image(path: &Path) -> R<ImageInfo> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size < 32 * 1024 {
        return Err("Datei ist zu klein / file too small".into());
    }
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if let Some(c) = compression_of(path) {
        let kind = if name.contains("steam") { "steamos" } else { "image" };
        return Ok(ImageInfo { mode: Mode::Raw, kind: kind.into(), label: String::new(), size, compressed: Some(c.into()), needed: 0 });
    }
    // Windows setup ISO?
    if let Ok(info) = crate::inspect(path) {
        let needed = windows_needed(path).unwrap_or(size);
        return Ok(ImageInfo { mode: Mode::Windows, kind: "windows".into(), label: info.volume_id, size, compressed: None, needed });
    }
    let label = iso9660_label(path).unwrap_or_default();
    let low = format!("{name} {}", label.to_ascii_lowercase());
    let kind = if low.contains("steam") || low.contains("holo") {
        "steamos"
    } else if label.is_empty() && !name.ends_with(".iso") {
        "image"
    } else {
        "linux"
    };
    Ok(ImageInfo { mode: Mode::Raw, kind: kind.into(), label, size, compressed: None, needed: size })
}

fn windows_needed(path: &Path) -> R<u64> {
    let mut u = Udf::open(path).map_err(|e| e.to_string())?;
    let files = u.walk().map_err(|e| e.to_string())?;
    // file data + FAT overhead (cluster slack, directories)
    Ok(files.iter().map(|f| f.size + 32 * 1024).sum::<u64>() + 64 * 1024 * 1024)
}

pub fn is_install_image(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p == "sources/install.wim" || p == "sources/install.esd"
}

// ------------------------------------------------------------------ Windows ISO -> folder

pub struct CopyPlan {
    pub files: Vec<crate::udf::WalkEntry>,
    pub total: u64,
    pub split: Option<(String, u64)>,
}

pub fn plan_windows_copy(iso: &Path, ctx: &mut Ctx) -> R<CopyPlan> {
    let mut u = Udf::open(iso).map_err(|e| e.to_string())?;
    let files = u.walk().map_err(|e| e.to_string())?;
    if !files.iter().any(|f| is_install_image(&f.path)) && !files.iter().any(|f| f.path.eq_ignore_ascii_case("sources/install.swm")) {
        return Err(ctx.t("Das ist keine Windows-Installations-ISO.", "This is not a Windows setup ISO."));
    }
    if !files.iter().any(|f| f.path.eq_ignore_ascii_case("efi/boot/bootx64.efi") || f.path.eq_ignore_ascii_case("efi/boot/bootaa64.efi")) {
        ctx.log(ctx.t("Warnung: kein UEFI-Bootloader in der ISO gefunden.", "Warning: no UEFI boot loader found in the ISO."));
    }
    let mut split = None;
    // test hook: lower threshold to exercise the split path with small images
    let threshold = std::env::var("WIE_SPLIT_THRESHOLD").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(FAT32_MAX);
    for f in files.iter().filter(|f| !f.is_dir && f.size > threshold) {
        if is_install_image(&f.path) {
            split = Some((f.path.clone(), f.size));
        } else {
            return Err(format!("{} > 4 GB – {}", f.path, ctx.t("kann nicht auf FAT32 gespeichert werden", "cannot be stored on FAT32")));
        }
    }
    let total = files.iter().filter(|f| !f.is_dir).map(|f| f.size).sum();
    Ok(CopyPlan { files, total, split })
}

/// Copies every file except a split candidate from the ISO into `dst`
/// (a mounted FAT32 volume). Returns (path, crc) for verification.
pub fn copy_iso_files(iso: &Path, dst: &Path, plan: &CopyPlan, ctx: &mut Ctx, pct_from: f64, pct_to: f64) -> R<Vec<(String, u64, u32)>> {
    let mut u = Udf::open(iso).map_err(|e| e.to_string())?;
    let mut done_total = 0u64;
    let mut sums = Vec::new();
    let stage = ctx.t("Kopiere Dateien", "Copying files");
    let total = plan.total.max(1);
    for f in &plan.files {
        let target = dst.join(f.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if f.is_dir {
            std::fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            continue;
        }
        if plan.split.as_ref().map(|(p, _)| p == &f.path).unwrap_or(false) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let out = File::create(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let mut w = CrcWriter { inner: out, crc: Crc32::new(), since_sync: 0 };
        let base = done_total;
        {
            let mut cb = |n: u64| -> crate::udf::Result<()> {
                let frac = (base + n) as f64 / total as f64;
                ctx.progress(pct_from + (pct_to - pct_from) * frac, &stage);
                Ok(())
            };
            u.copy_to(&f.path, &mut w, &mut cb).map_err(|e| format!("{}: {e}", f.path))?;
        }
        w.inner.sync_all().map_err(|e| format!("{}: {e}", ctx.t("Schreibfehler – Stick voll oder defekt?", "write error – stick full or faulty?")))?;
        done_total += f.size;
        sums.push((f.path.clone(), f.size, w.crc.finish()));
    }
    Ok(sums)
}

struct CrcWriter {
    inner: File,
    crc: Crc32,
    since_sync: u64,
}

impl Write for CrcWriter {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(b)?;
        self.crc.update(&b[..n]);
        self.since_sync += n as u64;
        if self.since_sync >= 64 * 1024 * 1024 {
            // keeps the progress honest (data really on the stick)
            self.inner.sync_data()?;
            self.since_sync = 0;
        }
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Re-reads every copied file from the stick and compares the checksum.
pub fn verify_files(dst: &Path, sums: &[(String, u64, u32)], ctx: &mut Ctx, pct_from: f64, pct_to: f64) -> R<()> {
    let total: u64 = sums.iter().map(|s| s.1).sum::<u64>().max(1);
    let mut done = 0u64;
    let stage = ctx.t("Prüfe Stick", "Verifying stick");
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    for (path, size, crc) in sums {
        let p = dst.join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mut f = File::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut c = Crc32::new();
        let mut n_total = 0u64;
        loop {
            let n = f.read(&mut buf).map_err(|e| format!("{}: {e}", p.display()))?;
            if n == 0 {
                break;
            }
            c.update(&buf[..n]);
            n_total += n as u64;
            ctx.progress(pct_from + (pct_to - pct_from) * ((done + n_total) as f64 / total as f64), &stage);
        }
        if n_total != *size || c.finish() != *crc {
            return Err(format!(
                "{} {}",
                ctx.t("Prüfung fehlgeschlagen – Datei auf dem Stick ist fehlerhaft:", "Verification failed – file on the stick is corrupt:"),
                path
            ) + &ctx.t(" (Stick defekt oder gefälscht? Anderen Stick/USB-Port versuchen.)", " (faulty or fake stick? Try another stick/USB port.)"));
        }
        done += size;
    }
    Ok(())
}

// ------------------------------------------------------------------ raw images

/// Opens a raw image, decompressing through an external tool if needed.
/// Returns a reader and a function giving the compressed bytes consumed.
pub struct RawSource {
    pub reader: Box<dyn Read + Send>,
    pub consumed: std::sync::Arc<std::sync::atomic::AtomicU64>,
    pub total_input: u64,
    pub child: Option<std::process::Child>,
}

pub fn open_raw(path: &Path, ctx: &Ctx) -> R<RawSource> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    let total = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let consumed = Arc::new(AtomicU64::new(0));
    match compression_of(path) {
        None => {
            let f = File::open(path).map_err(|e| e.to_string())?;
            let c = consumed.clone();
            Ok(RawSource { reader: Box::new(CountingReader { inner: f, count: c }), consumed, total_input: total, child: None })
        }
        Some(kind) => {
            let (prog, args): (&str, Vec<&str>) = match kind {
                "bz2" => ("bzip2", vec!["-dc"]),
                "gz" => ("gzip", vec!["-dc"]),
                "xz" => ("xz", vec!["-dc"]),
                "zst" => ("zstd", vec!["-dc"]),
                _ => ("unzip", vec!["-p"]),
            };
            let exe = find_tool(prog).ok_or_else(|| {
                format!("{} '{prog}' {}", ctx.t("Zum Entpacken wird", "Unpacking needs"), ctx.t("benötigt – bitte die Datei vorher entpacken.", "– please unpack the file first."))
            })?;
            let mut cmd = std::process::Command::new(exe);
            cmd.args(&args);
            if kind == "zip" {
                cmd.arg(path);
                cmd.stdin(std::process::Stdio::null());
            } else {
                cmd.stdin(std::process::Stdio::piped());
            }
            cmd.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
            let mut child = cmd.spawn().map_err(|e| format!("{prog}: {e}"))?;
            if kind != "zip" {
                let mut stdin = child.stdin.take().unwrap();
                let mut f = File::open(path).map_err(|e| e.to_string())?;
                let c = consumed.clone();
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; 1024 * 1024];
                    loop {
                        match f.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if stdin.write_all(&buf[..n]).is_err() {
                                    break;
                                }
                                c.fetch_add(n as u64, Ordering::Relaxed);
                            }
                        }
                    }
                });
            } else {
                consumed.store(0, Ordering::Relaxed);
            }
            let out = child.stdout.take().unwrap();
            Ok(RawSource { reader: Box::new(out), consumed, total_input: total, child: Some(child) })
        }
    }
}

struct CountingReader {
    inner: File,
    count: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
impl Read for CountingReader {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(b)?;
        self.count.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
        Ok(n)
    }
}

/// Fills `buf` as far as possible (handles short reads from pipes).
pub fn read_full(r: &mut dyn Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        let n = r.read(&mut buf[got..])?;
        if n == 0 {
            break;
        }
        got += n;
    }
    Ok(got)
}

pub fn find_tool(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
            dirs.push(d.join("tools"));
        }
    }
    if let Some(p) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&p));
    }
    for d in ["/usr/local/sbin", "/usr/local/bin", "/usr/sbin", "/usr/bin", "/sbin", "/bin"] {
        dirs.push(PathBuf::from(d));
    }
    for d in dirs {
        for cand in [name.to_string(), format!("{name}.exe")] {
            let p = d.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Arguments of the worker process.
#[derive(Debug, Clone)]
pub struct WorkerArgs {
    pub image: PathBuf,
    pub device: String,
    pub mode: Mode,
    pub force_any: bool,
    pub events: Option<PathBuf>,
    pub de: bool,
}

pub fn parse_worker_args(args: &[String]) -> R<WorkerArgs> {
    let mut image = None;
    let mut device = None;
    let mut mode = None;
    let mut force_any = false;
    let mut events = None;
    let mut de = true;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let val = || args.get(i + 1).cloned().ok_or_else(|| format!("missing value for {a}"));
        match a {
            "--image" => { image = Some(PathBuf::from(val()?)); i += 1; }
            "--device" => { device = Some(val()?); i += 1; }
            "--mode" => { mode = Some(if val()? == "windows" { Mode::Windows } else { Mode::Raw }); i += 1; }
            "--events" => { events = Some(PathBuf::from(val()?)); i += 1; }
            "--lang" => { de = val()? != "en"; i += 1; }
            "--force-any" => force_any = true,
            _ => {}
        }
        i += 1;
    }
    Ok(WorkerArgs {
        image: image.ok_or("--image missing")?,
        device: device.ok_or("--device missing")?,
        mode: mode.ok_or("--mode missing")?,
        force_any,
        events,
        de,
    })
}

/// Entry point of the elevated worker. Returns the process exit code.
pub fn worker_main(args: &[String]) -> i32 {
    let wa = match parse_worker_args(args) {
        Ok(a) => a,
        Err(e) => {
            println!("{}", Event::Error(e).to_json());
            return 2;
        }
    };
    let mut evfile = wa.events.as_ref().and_then(|p| File::create(p).ok());
    let mut emit = |e: Event| {
        let line = e.to_json();
        if let Some(f) = evfile.as_mut() {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        } else {
            println!("{line}");
            let _ = std::io::stdout().flush();
        }
    };
    let mut ctx = Ctx::new(wa.de, &mut emit);
    let res = run_worker(&wa, &mut ctx);
    match res {
        Ok(msg) => {
            (ctx.emit)(Event::Progress { pct: 100.0, stage: msg.clone() });
            (ctx.emit)(Event::Done(msg));
            0
        }
        Err(e) => {
            (ctx.emit)(Event::Error(e));
            1
        }
    }
}

#[cfg(target_os = "linux")]
fn run_worker(wa: &WorkerArgs, ctx: &mut Ctx) -> R<String> {
    linux::run(wa, ctx)
}
#[cfg(target_os = "windows")]
fn run_worker(wa: &WorkerArgs, ctx: &mut Ctx) -> R<String> {
    windows::run(wa, ctx)
}
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn run_worker(_wa: &WorkerArgs, _ctx: &mut Ctx) -> R<String> {
    Err("unsupported platform".into())
}

pub fn list_disks() -> R<Vec<Disk>> {
    #[cfg(target_os = "linux")]
    {
        linux::list_disks()
    }
    #[cfg(target_os = "windows")]
    {
        windows::list_disks()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        Ok(vec![])
    }
}
