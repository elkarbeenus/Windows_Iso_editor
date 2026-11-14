#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Windows ISO Editor – desktop front-end.
//! Created by Dominic Lesina x Claude.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::Manager;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Clone)]
struct Progress {
    phase: String,
    done: u64,
    total: u64,
}

#[derive(Serialize)]
struct EditionDto {
    name: String,
    arch: String,
    build: String,
}

#[derive(Serialize)]
struct InfoDto {
    path: String,
    file_name: String,
    volume_id: String,
    size: u64,
    archs: Vec<String>,
    image_file: String,
    has_autounattend: bool,
    editions: Vec<EditionDto>,
    windows11: Option<bool>,
    suggested_output: String,
}

#[derive(Serialize)]
struct BuildDto {
    output: String,
    xml_bytes: usize,
    verified_entries: usize,
}

fn suggested_output(src: &Path) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("windows");
    let dir = src.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut candidate = dir.join(format!("{stem}_debloated.iso"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}_debloated_{n}.iso"));
        n += 1;
    }
    candidate
}

#[tauri::command]
fn app_version() -> String {
    VERSION.to_string()
}

#[tauri::command]
fn get_catalog() -> String {
    wie_core::catalog::catalog_json()
}

#[tauri::command]
async fn inspect_iso(path: String) -> Result<InfoDto, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<InfoDto, String> {
        let p = PathBuf::from(&path);
        let i = wie_core::inspect(&p)?;
        Ok(InfoDto {
            file_name: p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string(),
            suggested_output: suggested_output(&p).to_string_lossy().to_string(),
            path,
            volume_id: i.volume_id,
            size: i.size,
            archs: i.archs,
            image_file: i.image_file,
            has_autounattend: i.has_autounattend,
            editions: i.editions.into_iter().map(|e| EditionDto { name: e.name, arch: e.arch, build: e.build }).collect(),
            windows11: i.windows11,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn build_iso(window: tauri::Window, src: String, dst: String, selected: Vec<String>) -> Result<BuildDto, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<BuildDto, String> {
        let mut last = Instant::now() - Duration::from_secs(1);
        let report = wie_core::build_iso(Path::new(&src), Path::new(&dst), &selected, VERSION, |phase, done, total| {
            if phase != "copy" || done == total || last.elapsed() > Duration::from_millis(120) {
                last = Instant::now();
                let _ = window.emit("build-progress", Progress { phase: phase.to_string(), done, total });
            }
        })?;
        Ok(BuildDto { output: dst, xml_bytes: report.xml_bytes, verified_entries: report.verified_entries })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn save_xml(path: String, selected: Vec<String>) -> Result<(), String> {
    let xml = wie_core::export_xml(&selected, VERSION)?;
    std::fs::write(&path, xml).map_err(|e| e.to_string())
}

#[tauri::command]
fn preview_script(selected: Vec<String>) -> Result<String, String> {
    let g = wie_core::unattend::generate(&selected, VERSION)?;
    Ok(g.specialize_ps1)
}

/// Shows the created ISO in the file manager.
#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let dir = p.parent().map(Path::to_path_buf).unwrap_or(p.clone());
        let mut cmd = std::process::Command::new("xdg-open");
        cmd.arg(&dir);
        // AppImage runtime variables would break the system file manager
        for v in ["LD_LIBRARY_PATH", "LD_PRELOAD", "GDK_PIXBUF_MODULE_FILE", "GDK_PIXBUF_MODULEDIR", "GIO_MODULE_DIR", "GIO_EXTRA_MODULES", "GTK_PATH", "GTK_EXE_PREFIX", "GTK_DATA_PREFIX", "GTK_IM_MODULE_FILE", "GSETTINGS_SCHEMA_DIR", "GST_PLUGIN_SYSTEM_PATH", "GST_PLUGIN_SYSTEM_PATH_1_0", "PYTHONHOME", "PYTHONPATH", "PERLLIB", "QT_PLUGIN_PATH", "APPDIR", "APPIMAGE", "OWD", "ARGV0", "WEBKIT_DISABLE_DMABUF_RENDERER", "WEBKIT_DISABLE_COMPOSITING_MODE"] {
            cmd.env_remove(v);
        }
        cmd.spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ------------------------------------------------------------------ USB sticks

#[derive(Serialize)]
struct DiskDto {
    id: String,
    name: String,
    size: u64,
    removable: bool,
    usb: bool,
    mounts: Vec<String>,
}

#[derive(Serialize)]
struct ImageDto {
    path: String,
    file_name: String,
    mode: String,
    kind: String,
    label: String,
    size: u64,
    compressed: Option<String>,
    needed: u64,
}

#[tauri::command]
async fn list_usb() -> Result<Vec<DiskDto>, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<Vec<DiskDto>, String> {
        Ok(wie_core::usb::list_disks()?
            .into_iter()
            .map(|d| DiskDto { id: d.id, name: d.name, size: d.size, removable: d.removable, usb: d.usb, mounts: d.mounts })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn detect_image(path: String) -> Result<ImageDto, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ImageDto, String> {
        let p = PathBuf::from(&path);
        let i = wie_core::usb::detect_image(&p)?;
        Ok(ImageDto {
            file_name: p.file_name().and_then(|s| s.to_str()).unwrap_or_default().to_string(),
            path,
            mode: if i.mode == wie_core::usb::Mode::Windows { "windows".into() } else { "raw".into() },
            kind: i.kind,
            label: i.label,
            size: i.size,
            compressed: i.compressed,
            needed: i.needed,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn worker_args(image: &str, device: &str, mode: &str, lang: &str) -> Vec<String> {
    vec![
        "--usb-worker".into(),
        "--image".into(),
        image.into(),
        "--device".into(),
        device.into(),
        "--mode".into(),
        mode.into(),
        "--lang".into(),
        lang.into(),
    ]
}

/// Parses one JSON line of the worker and forwards it to the UI.
/// Returns Some(Ok/Err) for the final event.
fn handle_line(window: &tauri::Window, line: &str) -> Option<Result<String, String>> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let _ = window.emit("usb-event", v.clone());
    match v.get("t").and_then(|t| t.as_str()) {
        Some("done") => Some(Ok(v.get("msg").and_then(|m| m.as_str()).unwrap_or_default().to_string())),
        Some("error") => Some(Err(v.get("msg").and_then(|m| m.as_str()).unwrap_or_default().to_string())),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn spawn_worker(window: &tauri::Window, args: Vec<String>, de: bool) -> Result<String, String> {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let is_root = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("Uid:")).map(|l| l.split_whitespace().nth(1) == Some("0")))
        .unwrap_or(false);
    let mut cmd = if is_root {
        let mut c = Command::new(&exe);
        c.args(&args);
        c
    } else {
        let pkexec = wie_core::usb::find_tool("pkexec").ok_or_else(|| {
            if de { "pkexec wurde nicht gefunden – Root-Rechte können nicht angefordert werden.".to_string() } else { "pkexec not found – cannot request root rights.".to_string() }
        })?;
        let mut c = Command::new(pkexec);
        match std::env::var_os("APPIMAGE") {
            // root cannot enter the user's FUSE mount of the AppImage -> let it extract itself
            Some(ai) => {
                c.arg("/usr/bin/env").arg("APPIMAGE_EXTRACT_AND_RUN=1").arg(ai);
            }
            None => {
                c.arg(&exe);
            }
        }
        c.args(&args);
        c
    };
    // AppImage runtime variables must not leak into the root process
    for v in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "APPIMAGE", "OWD", "ARGV0"] {
        cmd.env_remove(v);
    }
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().unwrap();
    let mut result: Option<Result<String, String>> = None;
    for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
        if let Some(r) = handle_line(window, &line) {
            result = Some(r);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let mut errtxt = String::new();
    if let Some(mut e) = child.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut errtxt);
    }
    match result {
        Some(r) => r,
        None if status.code() == Some(126) || status.code() == Some(127) => Err(if de {
            "Abgebrochen: Root-Rechte wurden nicht erteilt. (Unter SteamOS muss für den Benutzer „deck“ ein Passwort gesetzt sein – im Terminal: passwd)".into()
        } else {
            "Cancelled: root rights were not granted. (On SteamOS the user “deck” needs a password – in a terminal: passwd)".into()
        }),
        None => Err(format!("Worker: {status} {}", errtxt.lines().rev().take(6).collect::<Vec<_>>().join(" / "))),
    }
}

#[cfg(target_os = "windows")]
fn spawn_worker(window: &tauri::Window, mut args: Vec<String>, de: bool) -> Result<String, String> {
    use std::io::{Seek, SeekFrom};
    use std::os::windows::process::CommandExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let events = std::env::temp_dir().join(format!("wie-usb-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&events);
    args.push("--events".into());
    args.push(events.to_string_lossy().to_string());
    let arglist = args.iter().map(|a| format!("\"{}\"", a.replace('"', ""))).collect::<Vec<_>>().join(" ");
    let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let script = format!(
        "try {{ $p = Start-Process -FilePath {} -ArgumentList {} -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ exit 1223 }}",
        q(&exe.to_string_lossy()),
        q(&arglist)
    );
    let mut child = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script])
        .creation_flags(0x0800_0000)
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut result: Option<Result<String, String>> = None;
    let mut pos = 0u64;
    let mut pending = String::new();
    loop {
        let finished = child.try_wait().map_err(|e| e.to_string())?;
        if let Ok(mut f) = std::fs::File::open(&events) {
            if f.seek(SeekFrom::Start(pos)).is_ok() {
                let mut chunk = String::new();
                use std::io::Read;
                if let Ok(n) = f.read_to_string(&mut chunk) {
                    pos += n as u64;
                    pending.push_str(&chunk);
                    while let Some(i) = pending.find('\n') {
                        let line: String = pending.drain(..=i).collect();
                        if let Some(r) = handle_line(window, &line) {
                            result = Some(r);
                        }
                    }
                }
            }
        }
        if let Some(st) = finished {
            let _ = std::fs::remove_file(&events);
            return match result {
                Some(r) => r,
                None if st.code() == Some(1223) => Err(if de { "Abgebrochen: Administratorrechte wurden nicht erteilt.".into() } else { "Cancelled: administrator rights were not granted.".into() }),
                None => Err(format!("Worker exit {st}")),
            };
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn spawn_worker(_window: &tauri::Window, _args: Vec<String>, _de: bool) -> Result<String, String> {
    Err("unsupported".into())
}

#[tauri::command]
async fn write_usb(window: tauri::Window, image: String, device: String, mode: String, lang: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let args = worker_args(&image, &device, if mode == "windows" { "windows" } else { "raw" }, &lang);
        spawn_worker(&window, args, lang != "en")
    })
    .await
    .map_err(|e| e.to_string())?
}

// ------------------------------------------------------------------ in-app updater

const REPO: &str = "elkarbeenus/Windows_Iso_editor";

#[derive(Serialize, Clone)]
struct UpdateDto {
    version: String,
    display: String,
    notes: String,
    asset_name: String,
    asset_url: String,
    size: u64,
    sums_url: String,
    /// "appimage" | "portable" | "setup" | "" (cannot install automatically)
    method: String,
    page: String,
}

fn curl() -> std::process::Command {
    let mut c = std::process::Command::new(if cfg!(target_os = "windows") { "curl.exe" } else { "curl" });
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    #[cfg(target_os = "linux")]
    for v in ["LD_LIBRARY_PATH", "LD_PRELOAD"] {
        c.env_remove(v);
    }
    c
}

fn http_get(url: &str) -> Result<String, String> {
    let out = curl()
        .args(["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", "-H", "User-Agent: windows-iso-editor"])
        .arg(url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("HTTP: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn exe_dir_writable() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let Some(dir) = exe.parent() else { return false };
    let probe = dir.join(".wie-write-test");
    let ok = std::fs::write(&probe, b"x").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

#[tauri::command]
async fn check_update() -> Result<Option<UpdateDto>, String> {
    if std::env::var_os("WIE_NO_UPDATE_CHECK").is_some() {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(|| -> Result<Option<UpdateDto>, String> {
        let txt = http_get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))?;
        let v: serde_json::Value = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        let tag = v["tag_name"].as_str().unwrap_or_default().to_string();
        if !wie_core::update::is_newer(&tag, VERSION) {
            return Ok(None);
        }
        let assets = v["assets"].as_array().cloned().unwrap_or_default();
        let find = |suffix: &str| assets.iter().find(|a| a["name"].as_str().map(|n| n.ends_with(suffix)).unwrap_or(false)).cloned();
        let (method, asset) = if cfg!(target_os = "linux") {
            if std::env::var_os("APPIMAGE").is_some() { ("appimage", find("_x86_64.AppImage")) } else { ("", find("_x86_64.AppImage")) }
        } else if cfg!(target_os = "windows") {
            if exe_dir_writable() { ("portable", find("_x64-portable.exe")) } else { ("setup", find("_x64-setup.exe")) }
        } else {
            ("", None)
        };
        let sums = find("SHA256SUMS.txt");
        let notes: String = v["body"].as_str().unwrap_or_default().lines().filter(|l| !l.trim().is_empty()).take(8).collect::<Vec<_>>().join("\n");
        let (asset_name, asset_url, size) = match &asset {
            Some(a) => (
                a["name"].as_str().unwrap_or_default().to_string(),
                a["browser_download_url"].as_str().unwrap_or_default().to_string(),
                a["size"].as_u64().unwrap_or(0),
            ),
            None => (String::new(), String::new(), 0),
        };
        let can = asset.is_some() && sums.is_some() && !method.is_empty();
        Ok(Some(UpdateDto {
            display: wie_core::update::display_version(&tag),
            version: tag,
            notes,
            asset_name,
            asset_url,
            size,
            sums_url: sums.and_then(|s| s["browser_download_url"].as_str().map(String::from)).unwrap_or_default(),
            method: if can { method.to_string() } else { String::new() },
            page: v["html_url"].as_str().unwrap_or_default().to_string(),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads with progress (file size polling), verifies SHA-256.
fn download_verified(window: &tauri::Window, u: &UpdateDto, target: &Path) -> Result<(), String> {
    let sums = http_get(&u.sums_url)?;
    let expected = wie_core::update::sum_for(&sums, &u.asset_name).ok_or("SHA256SUMS: entry missing")?;
    let _ = std::fs::remove_file(target);
    let mut child = curl()
        .args(["-fL", "--retry", "3", "-H", "User-Agent: windows-iso-editor", "-o"])
        .arg(target)
        .arg(&u.asset_url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            if !st.success() {
                let _ = std::fs::remove_file(target);
                return Err("Download fehlgeschlagen / download failed".into());
            }
            break;
        }
        let done = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
        let _ = window.emit("update-progress", Progress { phase: "download".into(), done, total: u.size });
        std::thread::sleep(Duration::from_millis(300));
    }
    let _ = window.emit("update-progress", Progress { phase: "verify".into(), done: u.size, total: u.size });
    let got = wie_core::update::sha256_file(target).map_err(|e| e.to_string())?;
    if got != expected {
        let _ = std::fs::remove_file(target);
        return Err(format!("SHA-256 stimmt nicht / mismatch ({})", &got[..12]));
    }
    Ok(())
}

fn spawn_detached(path: &Path) -> Result<(), String> {
    let mut c = std::process::Command::new(path);
    #[cfg(target_os = "linux")]
    for v in ["LD_LIBRARY_PATH", "LD_PRELOAD", "APPDIR", "APPIMAGE", "OWD", "ARGV0", "GDK_PIXBUF_MODULE_FILE", "GIO_EXTRA_MODULES", "GTK_PATH", "GTK_EXE_PREFIX", "GTK_DATA_PREFIX", "GTK_IM_MODULE_FILE", "GSETTINGS_SCHEMA_DIR", "GI_TYPELIB_PATH"] {
        c.env_remove(v);
    }
    c.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_update(window: tauri::Window, update: serde_json::Value) -> Result<(), String> {
    let u = UpdateDto {
        version: update["version"].as_str().unwrap_or_default().into(),
        display: String::new(),
        notes: String::new(),
        asset_name: update["asset_name"].as_str().unwrap_or_default().into(),
        asset_url: update["asset_url"].as_str().unwrap_or_default().into(),
        size: update["size"].as_u64().unwrap_or(0),
        sums_url: update["sums_url"].as_str().unwrap_or_default().into(),
        method: update["method"].as_str().unwrap_or_default().into(),
        page: String::new(),
    };
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        match u.method.as_str() {
            "appimage" => {
                let target = PathBuf::from(std::env::var_os("APPIMAGE").ok_or("APPIMAGE not set")?);
                let tmp = target.with_extension("AppImage.download");
                download_verified(&window, &u, &tmp)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
                }
                // atomic replace in the same folder; the running copy keeps working until restart
                std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
                spawn_detached(&target)?;
            }
            "portable" => {
                let cur = std::env::current_exe().map_err(|e| e.to_string())?;
                let tmp = cur.with_extension("download");
                download_verified(&window, &u, &tmp)?;
                let old = cur.with_extension("old");
                let _ = std::fs::remove_file(&old);
                // a running exe cannot be overwritten, but it can be renamed
                std::fs::rename(&cur, &old).map_err(|e| e.to_string())?;
                if let Err(e) = std::fs::rename(&tmp, &cur) {
                    let _ = std::fs::rename(&old, &cur);
                    return Err(e.to_string());
                }
                spawn_detached(&cur)?;
            }
            "setup" => {
                let tmp = std::env::temp_dir().join(&u.asset_name);
                download_verified(&window, &u, &tmp)?;
                spawn_detached(&tmp)?;
            }
            _ => return Err("Automatisches Update hier nicht möglich / not possible here".into()),
        }
        std::thread::sleep(Duration::from_millis(400));
        std::process::exit(0);
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Marker file: exists while a start is "unconfirmed". If the UI never reports
/// back (grey window), the next start uses software rendering.
fn start_marker() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.map(|b| b.join("windows-iso-editor").join("start-unconfirmed"))
}

/// Called by the UI as soon as it is visible.
#[tauri::command]
fn ui_ready() {
    if let Some(m) = start_marker() {
        let _ = std::fs::remove_file(m);
    }
}

fn main() {
    // elevated helper process for writing USB sticks (no window)
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(|a| a == "--usb-worker").unwrap_or(false) {
        std::process::exit(wie_core::usb::worker_main(&argv[2..]));
    }

    // leftover of a previous portable update
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }

    #[cfg(target_os = "linux")]
    {
        // Stable rendering on SteamOS / newer WebKitGTK (prevents grey or empty windows)
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        let previous_failed = start_marker().map(|m| m.exists()).unwrap_or(false);
        if previous_failed || std::env::var_os("WIE_SAFE_MODE").is_some() {
            // last start never showed the UI -> software rendering (always works)
            std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
        }
        if let Some(m) = start_marker() {
            if let Some(d) = m.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&m, if previous_failed { "safe" } else { "normal" });
        }
    }

    tauri::Builder::default()
        .setup(|app| {
            if let Some(w) = app.get_window("main") {
                let _ = w.set_title(&format!("Windows ISO Editor {VERSION}"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_version, get_catalog, inspect_iso, build_iso, save_xml, preview_script, open_folder, ui_ready, list_usb, detect_image, write_usb, check_update, install_update])
        .run(tauri::generate_context!())
        .expect("error while running Windows ISO Editor");
}
