use serde::{Deserialize, Serialize};
use serde_json;
use std::{fs, path::Path, process::Command};

#[derive(Deserialize)]
struct CargoToml {
    package: PackageInfo,
}

#[derive(Deserialize)]
struct PackageInfo {
    name: String,
    version: String,
    authors: Vec<String>,
    description: String,
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    plugin: Option<PluginMetadata>,
}

#[derive(Deserialize)]
struct PluginMetadata {
    display_name: Option<String>,
}

#[derive(Serialize)]
struct PluginManifest {
    id: String,
    name: String,
    version: String,
    description: String,
    authors: Vec<String>,
}

fn main() {
    let plugins_dir = Path::new("plugins");
    let out_dir = Path::new("target/wasm-plugins");
    fs::create_dir_all(out_dir).unwrap();

    for entry in fs::read_dir(plugins_dir).unwrap() {
        let path = entry.unwrap().path();
        let cargo_toml_path = path.join("Cargo.toml");

        if !cargo_toml_path.exists() {
            continue;
        }

        let plugin_name = path.file_name().unwrap().to_str().unwrap();
        println!("Building plugin: {}", plugin_name);

        // Read and parse Cargo.toml
        let cargo_toml_content = fs::read_to_string(&cargo_toml_path).unwrap();
        let cargo_toml: CargoToml = toml::from_str(&cargo_toml_content).unwrap();

        // Get display name from metadata, or fall back to package name
        let display_name = cargo_toml
            .package
            .metadata
            .as_ref()
            .and_then(|m| m.plugin.as_ref())
            .and_then(|p| p.display_name.as_ref())
            .cloned()
            .unwrap_or_else(|| cargo_toml.package.name.clone());

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

        // Create plugin output directory
        let plugin_out_dir = out_dir.join(plugin_name);
        fs::create_dir_all(&plugin_out_dir).unwrap();

        // Copy the generated wasm file
        let wasm_file = Path::new("target/wasm32-wasip1/release")
            .join(format!("{}.wasm", plugin_name.replace("-", "")));

        if !wasm_file.exists() {
            panic!("Compiled wasm file not found: {:?}", wasm_file);
        }

        let dest_wasm = plugin_out_dir.join("plugin.wasm");
        fs::copy(&wasm_file, &dest_wasm).unwrap();

        // Generate manifest.json from Cargo.toml
        let manifest = PluginManifest {
            id: cargo_toml.package.name.clone(),
            name: display_name,
            version: cargo_toml.package.version.clone(),
            description: cargo_toml.package.description.clone(),
            authors: cargo_toml.package.authors.clone(),
        };

        let manifest_path = plugin_out_dir.join("manifest.json");
        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();
        fs::write(&manifest_path, manifest_json).unwrap();

        println!("  ✓ Plugin built to {:?}", plugin_out_dir);
    }

    println!("\nAll plugins built to {:?}", out_dir);
}
