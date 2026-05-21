use std::env;
use std::fs;
use std::path::Path;

// This file isn't needed by plugins, just copies the `plugin.json` to be beside `plugin.wasm`

fn main() {
    println!("cargo::rerun-if-changed=plugin.json");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_dir = get_cargo_target_dir().unwrap();
    let src = Path::new(&manifest_dir).join("plugin.json");
    let dest = Path::new(&build_dir).join("plugin.json");

    if src.exists() {
        fs::copy(&src, &dest).expect("Failed to copy plugin.json");
    }
}

fn get_cargo_target_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let profile = std::env::var("PROFILE")?;
    let mut target_dir = None;
    let mut sub_path = out_dir.as_path();
    while let Some(parent) = sub_path.parent() {
        if parent.ends_with(&profile) {
            target_dir = Some(parent);
            break;
        }
        sub_path = parent;
    }
    let target_dir = target_dir.ok_or("not found")?;
    Ok(target_dir.to_path_buf())
}
