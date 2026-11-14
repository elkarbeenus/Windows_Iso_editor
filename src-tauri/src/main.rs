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

fn main() {
    #[cfg(target_os = "linux")]
    {
        // Stable rendering on SteamOS / newer WebKitGTK (prevents grey or empty windows)
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }

    tauri::Builder::default()
        .setup(|app| {
            if let Some(w) = app.get_window("main") {
                let _ = w.set_title(&format!("Windows ISO Editor {VERSION}"));
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_version, get_catalog, inspect_iso, build_iso, save_xml, preview_script, open_folder])
        .run(tauri::generate_context!())
        .expect("error while running Windows ISO Editor");
}
