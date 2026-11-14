//! End-to-end: generate a Microsoft-style UDF test ISO (python fixture), patch it,
//! read it back and validate every descriptor with an independent checker.
use std::path::{Path, PathBuf};
use std::process::Command;

fn tools() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/test")
}

fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| Command::new(p).arg("--version").output().is_ok())
}

fn run(variant: &[&str], tag: &str) {
    let Some(py) = python() else { eprintln!("python missing, skipped"); return };
    let dir = std::env::temp_dir().join(format!("wie-test-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("in.iso");
    let dst = dir.join("out.iso");
    let st = Command::new(py).arg(tools().join("gen_udf.py")).arg(&src).args(variant).status().unwrap();
    assert!(st.success());
    let orig = std::fs::read(&src).unwrap();

    let info = wie_core::inspect(&src).unwrap();
    assert_eq!(info.archs, vec!["x64"]);
    assert_eq!(info.editions.len(), 2);
    assert!(!info.has_autounattend);

    let ids: Vec<String> = wie_core::catalog::preset_ids(wie_core::catalog::Preset::Maximum).iter().map(|s| s.to_string()).collect();
    wie_core::build_iso(&src, &dst, &ids, "test", |_, _, _| {}).unwrap();
    assert_eq!(std::fs::read(&src).unwrap(), orig, "source must never change");

    let out = Command::new(py).arg(tools().join("check_tags.py")).arg(&dst).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));

    // patch again (replace existing autounattend.xml)
    let dst2 = dir.join("out2.iso");
    wie_core::build_iso(&dst, &dst2, &["copilot".to_string()], "test", |_, _, _| {}).unwrap();
    assert!(wie_core::inspect(&dst2).unwrap().has_autounattend);
    let out = Command::new(py).arg(tools().join("check_tags.py")).arg(&dst2).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));

    // refusing to overwrite the source
    assert!(wie_core::build_iso(&src, &src, &ids, "t", |_, _, _| {}).is_err());
    assert_eq!(std::fs::read(&src).unwrap(), orig);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn patch_short_ad() { run(&[], "short"); }

#[test]
fn patch_long_ad_trailing_anchor() { run(&["--long-ad", "--trailing-avdp"], "long"); }
