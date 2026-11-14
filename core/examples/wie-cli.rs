//! Command line front-end (also used for tests).
//!   wie-cli inspect IN.iso
//!   wie-cli build IN.iso OUT.iso [recommended|gentle|maximum|id,id,...]
//!   wie-cli xml [preset|ids]
use std::path::Path;
use wie_core::catalog::{preset_ids, Preset};

fn sel(arg: Option<&String>) -> Vec<String> {
    match arg.map(|s| s.as_str()) {
        None | Some("recommended") => preset_ids(Preset::Recommended).iter().map(|s| s.to_string()).collect(),
        Some("gentle") => preset_ids(Preset::Gentle).iter().map(|s| s.to_string()).collect(),
        Some("maximum") => preset_ids(Preset::Maximum).iter().map(|s| s.to_string()).collect(),
        Some("all") => wie_core::catalog::ITEMS.iter().map(|i| i.id.to_string()).collect(),
        Some(list) => list.split(',').map(|s| s.to_string()).collect(),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("inspect") => match wie_core::inspect(Path::new(&a[2])) {
            Ok(i) => println!("{i:#?}"),
            Err(e) => {
                eprintln!("ERROR: {e}");
                std::process::exit(1)
            }
        },
        Some("build") => {
            let s = sel(a.get(4));
            match wie_core::build_iso(Path::new(&a[2]), Path::new(&a[3]), &s, "dev", |p, d, t| {
                if p != "copy" || d == t {
                    eprintln!("{p} {d}/{t}")
                }
            }) {
                Ok(r) => println!("OK xml={} verified={}", r.xml_bytes, r.verified_entries),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1)
                }
            }
        }
        Some("usb-worker") => std::process::exit(wie_core::usb::worker_main(&a[2..])),
        Some("usb-list") => println!("{:#?}", wie_core::usb::list_disks()),
        Some("detect") => println!("{:#?}", wie_core::usb::detect_image(Path::new(&a[2]))),
        Some("catalog") => print!("{}", wie_core::catalog::catalog_json()),
        Some("xml") => {
            let x = wie_core::export_xml(&sel(a.get(2)), "dev").unwrap();
            match a.get(3) {
                Some(out) => std::fs::write(out, x).unwrap(),
                None => print!("{x}"),
            }
        }
        _ => eprintln!("usage: wie-cli inspect|build|xml ..."),
    }
}
