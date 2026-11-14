pub mod catalog;
pub mod crc32;
pub mod usb;
pub mod udf;
pub mod unattend;
pub mod update;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

pub use udf::Udf;

#[derive(Debug, Clone, Default)]
pub struct Edition {
    pub name: String,
    pub arch: String,
    pub build: String,
}

#[derive(Debug, Clone, Default)]
pub struct IsoInfo {
    pub volume_id: String,
    pub size: u64,
    pub archs: Vec<String>,
    pub image_file: String,
    pub has_autounattend: bool,
    pub editions: Vec<Edition>,
    pub windows11: Option<bool>,
}

fn arch_name(code: &str) -> String {
    match code.trim() {
        "9" => "x64".into(),
        "12" => "arm64".into(),
        "0" => "x86".into(),
        "5" => "arm".into(),
        other => other.into(),
    }
}

fn tag_values(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(s) = rest.find(&open) {
        let after = &rest[s + open.len()..];
        if let Some(e) = after.find(&close) {
            out.push(after[..e].to_string());
            rest = &after[e + close.len()..];
        } else {
            break;
        }
    }
    out
}

fn parse_wim_xml(udf: &mut Udf, path: &str) -> Vec<Edition> {
    let hdr = match udf.read_file_range(path, 0, 208) {
        Ok(h) if h.len() >= 208 && &h[..5] == b"MSWIM" => h,
        _ => return vec![],
    };
    let mut sz = [0u8; 8];
    sz[..7].copy_from_slice(&hdr[72..79]);
    let size = u64::from_le_bytes(sz);
    let off = u64::from_le_bytes(hdr[80..88].try_into().unwrap());
    if size == 0 || size > 16 * 1024 * 1024 {
        return vec![];
    }
    let raw = match udf.read_file_range(path, off, size) {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let u16s: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let xml = String::from_utf16_lossy(&u16s);
    let mut eds = Vec::new();
    let mut rest = xml.as_str();
    while let Some(s) = rest.find("<IMAGE ") {
        let after = &rest[s..];
        let end = after.find("</IMAGE>").unwrap_or(after.len());
        let block = &after[..end];
        let name = tag_values(block, "DISPLAYNAME")
            .into_iter()
            .next()
            .or_else(|| tag_values(block, "NAME").into_iter().next())
            .unwrap_or_default();
        let arch = tag_values(block, "ARCH").into_iter().next().map(|a| arch_name(&a)).unwrap_or_default();
        let build = tag_values(block, "BUILD").into_iter().next().unwrap_or_default();
        eds.push(Edition { name, arch, build });
        rest = &after[end.min(after.len())..];
        if end == after.len() {
            break;
        }
    }
    eds
}

pub fn inspect(path: &Path) -> Result<IsoInfo, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let mut udf = Udf::open(path).map_err(|e| e.to_string())?;
    let mut info = IsoInfo { volume_id: udf.volume_id.clone(), size, ..Default::default() };

    let efi = udf.list("efi/boot").unwrap_or_default();
    for (n, _, _) in &efi {
        let l = n.to_ascii_lowercase();
        let a = match l.as_str() {
            "bootx64.efi" => "x64",
            "bootaa64.efi" => "arm64",
            "bootia32.efi" => "x86",
            _ => continue,
        };
        if !info.archs.iter().any(|x| x == a) {
            info.archs.push(a.to_string());
        }
    }
    for f in ["sources/install.wim", "sources/install.esd", "sources/install.swm"] {
        if udf.exists(f) {
            info.image_file = f.rsplit('/').next().unwrap().to_string();
            info.editions = parse_wim_xml(&mut udf, f);
            break;
        }
    }
    for e in &info.editions {
        if !e.arch.is_empty() && !info.archs.contains(&e.arch) {
            info.archs.push(e.arch.clone());
        }
    }
    if let Some(b) = info.editions.iter().find_map(|e| e.build.parse::<u32>().ok()) {
        info.windows11 = Some(b >= 22000);
    }
    info.has_autounattend = udf.exists("autounattend.xml");
    if info.image_file.is_empty() || !udf.exists("sources/boot.wim") {
        return Err("Das ist keine Windows-Installations-ISO (sources\\install.wim / boot.wim fehlt). / Not a Windows setup ISO.".into());
    }
    Ok(info)
}

pub struct BuildReport {
    pub xml_bytes: usize,
    pub verified_entries: usize,
}

/// phase: "copy" | "patch" | "verify"
pub fn build_iso(
    src: &Path,
    dst: &Path,
    selected: &[String],
    version: &str,
    mut progress: impl FnMut(&str, u64, u64),
) -> Result<BuildReport, String> {
    // Never touch the source: refuse before anything happens (and never clean it up).
    if dst.exists() && same_file(src, dst) {
        return Err("Ziel darf nicht die Original-ISO sein. / Output must differ from the source ISO.".into());
    }
    let res = build_iso_inner(src, dst, selected, version, &mut progress);
    if res.is_err() && !same_file(src, dst) {
        let _ = std::fs::remove_file(dst);
    }
    res
}

fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            if x.dev() == y.dev() && x.ino() == y.ino() {
                return true;
            }
        }
    }
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

fn build_iso_inner(
    src: &Path,
    dst: &Path,
    selected: &[String],
    version: &str,
    progress: &mut impl FnMut(&str, u64, u64),
) -> Result<BuildReport, String> {
    if dst.exists() && same_file(src, dst) {
        return Err("Ziel darf nicht die Original-ISO sein. / Output must differ from the source ISO.".into());
    }
    let info = inspect(src)?;
    if !info.archs.is_empty() && !info.archs.iter().any(|a| a == "x64" || a == "arm64") {
        return Err("Nur x64- und ARM64-ISOs werden unterstützt. / Only x64 and ARM64 ISOs are supported.".into());
    }
    let gen = unattend::generate(selected, version)?;

    // original root listing for verification
    let before = {
        let mut u = Udf::open(src).map_err(|e| e.to_string())?;
        u.list("").map_err(|e| e.to_string())?
    };

    // 1) bit-exact copy
    let total = info.size;
    {
        let mut r = File::open(src).map_err(|e| e.to_string())?;
        let mut w = File::create(dst).map_err(|e| format!("Ziel kann nicht geschrieben werden: {e}"))?;
        let mut buf = vec![0u8; 8 * 1024 * 1024];
        let mut done = 0u64;
        loop {
            let n = r.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n]).map_err(|e| format!("Schreibfehler (Speicher voll?): {e}"))?;
            done += n as u64;
            progress("copy", done, total);
        }
        w.sync_all().map_err(|e| e.to_string())?;
        if done != total {
            return Err("Kopie unvollständig / incomplete copy".into());
        }
    }

    // 2) append autounattend.xml
    progress("patch", 0, 1);
    {
        let mut u = Udf::open_rw(dst).map_err(|e| e.to_string())?;
        u.add_root_files(&[("autounattend.xml", gen.xml.as_bytes())]).map_err(|e| e.to_string())?;
    }
    progress("patch", 1, 1);

    // 3) verify by reading the result back
    progress("verify", 0, 1);
    let mut u = Udf::open(dst).map_err(|e| format!("Prüfung fehlgeschlagen: {e}"))?;
    let back = u.read_file("autounattend.xml", 16 * 1024 * 1024).map_err(|e| format!("Prüfung fehlgeschlagen: {e}"))?;
    if back != gen.xml.as_bytes() {
        return Err("Prüfung fehlgeschlagen: autounattend.xml stimmt nicht überein".into());
    }
    let after = u.list("").map_err(|e| e.to_string())?;
    let mut checked = 0;
    for (name, is_dir, size) in &before {
        if name.eq_ignore_ascii_case("autounattend.xml") {
            continue;
        }
        match after.iter().find(|(n, _, _)| n == name) {
            Some((_, d, s)) if d == is_dir && s == size => checked += 1,
            _ => return Err(format!("Prüfung fehlgeschlagen: {name} fehlt oder ist verändert")),
        }
    }
    for p in ["sources/boot.wim", "boot/etfsboot.com", "efi/microsoft/boot/efisys.bin", "bootmgr"] {
        let mut o = Udf::open(src).map_err(|e| e.to_string())?;
        if o.exists(p) {
            let a = o.read_file_range(p, 0, 1 << 20).map_err(|e| e.to_string())?;
            let b = u.read_file_range(p, 0, 1 << 20).map_err(|e| e.to_string())?;
            if a != b {
                return Err(format!("Prüfung fehlgeschlagen: {p} verändert"));
            }
        }
    }
    if !u.exists(&format!("sources/{}", info.image_file)) {
        return Err("Prüfung fehlgeschlagen: Windows-Abbild fehlt".into());
    }
    progress("verify", 1, 1);
    Ok(BuildReport { xml_bytes: gen.xml.len(), verified_entries: checked })
}

pub fn export_xml(selected: &[String], version: &str) -> Result<String, String> {
    Ok(unattend::generate(selected, version)?.xml)
}
