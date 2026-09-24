//! Compile Blueprint files to GtkBuilder XML and bundle them with CSS/icons into a GResource.

use std::path::{Path, PathBuf};
use std::process::Command;

const RESOURCE_PREFIX: &str = "/io/github/jonas_bickel/Primvokon";

/// Locate a working `blueprint-compiler`. `BLUEPRINT_COMPILER` overrides the lookup; otherwise
/// the one on `PATH` is tried, then the distro script run with an interpreter that has PyGObject
/// (some environments ship a different default `python3`).
fn find_blueprint_compiler() -> Vec<String> {
    println!("cargo:rerun-if-env-changed=BLUEPRINT_COMPILER");
    let mut candidates: Vec<Vec<String>> = Vec::new();
    if let Ok(custom) = std::env::var("BLUEPRINT_COMPILER") {
        candidates.push(custom.split_whitespace().map(String::from).collect());
    }
    candidates.push(vec!["blueprint-compiler".into()]);
    for py in ["python3.12", "python3.13", "python3.11", "python3"] {
        candidates.push(vec![py.into(), "/usr/bin/blueprint-compiler".into()]);
    }
    for c in candidates {
        let ok = Command::new(&c[0])
            .args(&c[1..])
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return c;
        }
    }
    panic!("no working blueprint-compiler found; install it or set BLUEPRINT_COMPILER");
}

fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let data_dir = Path::new("data");
    let ui_src = data_dir.join("ui");
    let ui_out = out_dir.join("ui");
    std::fs::create_dir_all(&ui_out).expect("create ui out dir");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data");

    let mut blp_files: Vec<PathBuf> = std::fs::read_dir(&ui_src)
        .expect("data/ui")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "blp"))
        .collect();
    blp_files.sort();
    for f in &blp_files {
        println!("cargo:rerun-if-changed={}", f.display());
    }

    let compiler = find_blueprint_compiler();
    let status = Command::new(&compiler[0])
        .args(&compiler[1..])
        .arg("batch-compile")
        .arg(&ui_out)
        .arg(&ui_src)
        .args(&blp_files)
        .status()
        .expect("blueprint-compiler not found: install the blueprint-compiler package");
    assert!(status.success(), "blueprint-compiler failed");

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gresources>\n");
    xml.push_str(&format!("  <gresource prefix=\"{RESOURCE_PREFIX}\">\n"));
    for f in &blp_files {
        let name = f.file_stem().unwrap().to_string_lossy();
        xml.push_str(&format!("    <file compressed=\"true\" preprocess=\"xml-stripblanks\">ui/{name}.ui</file>\n"));
    }
    xml.push_str("    <file compressed=\"true\">style.css</file>\n");
    for entry in std::fs::read_dir(data_dir.join("icons")).expect("data/icons") {
        let p = entry.expect("icon").path();
        if p.extension().is_some_and(|e| e == "svg") {
            xml.push_str(&format!(
                "    <file preprocess=\"xml-stripblanks\">icons/{}</file>\n",
                p.file_name().unwrap().to_string_lossy()
            ));
        }
    }
    xml.push_str("  </gresource>\n</gresources>\n");
    let xml_path = out_dir.join("primvokon.gresource.xml");
    std::fs::write(&xml_path, xml).expect("write gresource xml");

    glib_build_tools::compile_resources(
        &[out_dir.to_str().unwrap(), data_dir.to_str().unwrap()],
        xml_path.to_str().unwrap(),
        "primvokon.gresource",
    );
}
