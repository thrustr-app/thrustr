use std::{fs, path::Path, process::Command};

fn main() {
    let plugins_dir = Path::new("plugins");
    let out_dir = Path::new("target/wasm-plugins");
    fs::create_dir_all(out_dir).unwrap();

    for entry in fs::read_dir(plugins_dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.join("Cargo.toml").exists() {
            continue;
        }

        let plugin_name = path.file_name().unwrap().to_str().unwrap();
        println!("Building plugin: {}", plugin_name);
        // Build plugin for wasm32 target
        let status = Command::new("xtp")
            .args([
                "plugin",
                "build",
                "--path",
                &format!("plugins/{plugin_name}"),
            ])
            .status()
            .unwrap();

        assert!(status.success(), "Plugin build failed");

        // Copy the generated wasm into a central folder
        let wasm_file = Path::new("target/wasm32-wasip1/release")
            .join(format!("{}.wasm", plugin_name.replace("-", "")));
        if !wasm_file.exists() {
            panic!("Compiled wasm file not found: {:?}", wasm_file);
        }
        let dest_file = out_dir.join(format!("{}.wasm", plugin_name));
        fs::copy(&wasm_file, &dest_file).unwrap();
    }

    println!("All plugins built to {:?}", out_dir);
}
