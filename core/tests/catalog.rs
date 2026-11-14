use wie_core::catalog::{preset_ids, Action, Preset, ITEMS, PROTECTED};

fn like(name: &str, pat: &str) -> bool {
    // minimal wildcard match ('*' only), case-insensitive like PowerShell -like
    let (n, p) = (name.to_ascii_lowercase(), pat.to_ascii_lowercase());
    let parts: Vec<&str> = p.split('*').collect();
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() { continue; }
        match n[pos..].find(part) {
            Some(f) if i > 0 || f == 0 => pos += f + part.len(),
            _ => return false,
        }
    }
    p.ends_with('*') || pos == n.len()
}

#[test]
fn no_selected_app_is_protected() {
    for it in ITEMS {
        for a in it.actions {
            if let Action::Appx(v) = a {
                for n in *v {
                    for p in PROTECTED {
                        assert!(!like(n, p), "{} ({}) matches protected {}", n, it.id, p);
                    }
                }
            }
        }
    }
}

#[test]
fn gaming_and_bluetooth_never_touched() {
    let xml = wie_core::export_xml(&ITEMS.iter().map(|i| i.id.to_string()).collect::<Vec<_>>(), "t").unwrap();
    let body: String = xml.lines().filter(|l| !l.contains("$Protected = @(")).collect::<Vec<_>>().join("\n").to_lowercase();
    for bad in ["xbox", "gamingservices", "bthserv", "bluetooth", "windowsstore", "wuauserv", "windefend", "msedge.exe", "remove-appxprovisionedpackage -online -allusers"] {
        assert!(!body.contains(bad), "generated script mentions {bad}");
    }
}

#[test]
fn xml_is_ascii_and_commands_fit() {
    for p in [Preset::Gentle, Preset::Recommended, Preset::Maximum] {
        let ids: Vec<String> = preset_ids(p).iter().map(|s| s.to_string()).collect();
        let xml = wie_core::export_xml(&ids, "0.0.1").unwrap();
        assert!(xml.is_ascii(), "xml must be ASCII");
        for seg in xml.split("<Path>").skip(1) {
            let cmd = seg.split("</Path>").next().unwrap().replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">");
            assert!(cmd.len() < 259, "command too long: {}", cmd.len());
        }
        assert!(!xml.contains("<DiskConfiguration") && !xml.contains("<ProductKey") && !xml.contains("<LocalAccounts"));
    }
}

#[test]
fn presets_are_nested() {
    let g = preset_ids(Preset::Gentle);
    let r = preset_ids(Preset::Recommended);
    let m = preset_ids(Preset::Maximum);
    assert!(g.iter().all(|i| r.contains(i)));
    assert!(r.iter().all(|i| m.contains(i)));
    assert!(!m.contains(&"bypass_hw"));
}

#[test]
fn unique_ids() {
    let mut ids: Vec<_> = ITEMS.iter().map(|i| i.id).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(n, ids.len());
}
