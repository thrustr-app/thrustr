use anyhow::{Context, Result, bail};
use std::{
    env,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};
use zip::{ZipWriter, write::SimpleFileOptions};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("build-plugins") => build_plugins()?,
        Some(cmd) => bail!("Unknown command: {}", cmd),
        None => {
            println!("Usage: cargo xtask <command>");
            println!();
            println!("Commands:");
            println!("  build-plugins    Build all plugins and bundle them as .tp files");
        }
    }

    Ok(())
}

fn workspace_root() -> Result<PathBuf> {
    let output = Command::new(env!("CARGO"))
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()
        .context("Failed to run cargo locate-project")?;

    let path = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in cargo output")?
        .trim()
        .to_string();

    Ok(PathBuf::from(path)
        .parent()
        .context("Failed to get workspace root")?
        .to_path_buf())
}

fn build_plugins() -> Result<()> {
    let root = workspace_root()?;
    let plugins_dir = root.join("plugins");
    let target_plugins_dir = root.join("target").join("plugins");

    // Create target/plugins directory
    fs::create_dir_all(&target_plugins_dir).context("Failed to create target/plugins directory")?;

    // Find all plugin directories (directories containing manifest.toml)
    let mut plugins = Vec::new();
    for entry in fs::read_dir(&plugins_dir).context("Failed to read plugins directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && path.join("manifest.toml").exists() {
            plugins.push(path);
        }
    }

    if plugins.is_empty() {
        println!("No plugins found in {}", plugins_dir.display());
        return Ok(());
    }

    println!("Found {} plugin(s)", plugins.len());

    // Build all plugins with cargo (specify each plugin package explicitly)
    println!("\n==> Building plugins...");
    let mut build_args = vec!["build", "--target", "wasm32-wasip2", "--release"];

    // Get plugin package names from directory names
    let plugin_names: Vec<String> = plugins
        .iter()
        .filter_map(|p| p.file_name()?.to_str().map(String::from))
        .collect();

    // Add -p flag for each plugin
    for name in &plugin_names {
        build_args.push("-p");
        build_args.push(name);
    }

    let status = Command::new(env!("CARGO"))
        .args(&build_args)
        .current_dir(&root)
        .status()
        .context("Failed to run cargo build")?;

    if !status.success() {
        bail!("cargo build failed");
    }

    // Bundle each plugin
    println!("\n==> Bundling plugins...");
    for plugin_path in &plugins {
        bundle_plugin(plugin_path, &root, &target_plugins_dir)?;
    }

    println!(
        "\n==> Done! Plugins are in {}",
        target_plugins_dir.display()
    );
    Ok(())
}

fn bundle_plugin(plugin_path: &Path, workspace_root: &Path, output_dir: &Path) -> Result<()> {
    let plugin_name = plugin_path
        .file_name()
        .context("Invalid plugin path")?
        .to_str()
        .context("Invalid plugin name")?;

    // Convert plugin name to wasm file name (replace - with _)
    let wasm_name = format!("{}.wasm", plugin_name.replace('-', "_"));
    let wasm_path = workspace_root
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join(&wasm_name);

    let manifest_path = plugin_path.join("manifest.toml");

    // Check that required files exist
    if !wasm_path.exists() {
        bail!(
            "Plugin wasm not found: {}. Make sure the plugin is a cdylib.",
            wasm_path.display()
        );
    }
    if !manifest_path.exists() {
        bail!("Plugin manifest not found: {}", manifest_path.display());
    }

    // Create .tp file (zip archive)
    let tp_path = output_dir.join(format!("{}.tp", plugin_name));
    let file = File::create(&tp_path)
        .with_context(|| format!("Failed to create {}", tp_path.display()))?;

    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(9));

    // Add plugin.wasm
    zip.start_file("plugin.wasm", options)?;
    let mut wasm_file = File::open(&wasm_path)?;
    let mut wasm_content = Vec::new();
    wasm_file.read_to_end(&mut wasm_content)?;
    zip.write_all(&wasm_content)?;

    // Add manifest.toml
    zip.start_file("manifest.toml", options)?;
    let mut manifest_file = File::open(&manifest_path)?;
    let mut manifest_content = Vec::new();
    manifest_file.read_to_end(&mut manifest_content)?;
    zip.write_all(&manifest_content)?;

    // Add icon.* if present (any image extension)
    let icon_path = fs::read_dir(plugin_path)
        .context("Failed to read plugin directory")?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s == "icon")
                .unwrap_or(false)
                && p.extension().is_some()
        });

    if let Some(icon_path) = icon_path {
        let icon_file_name = icon_path
            .file_name()
            .context("Invalid icon file name")?
            .to_str()
            .context("Non-UTF-8 icon file name")?
            .to_string();
        zip.start_file(&icon_file_name, options)?;
        let mut icon_file = File::open(&icon_path)?;
        let mut icon_content = Vec::new();
        icon_file.read_to_end(&mut icon_content)?;
        zip.write_all(&icon_content)?;
    }

    zip.finish()?;

    let tp_size = fs::metadata(&tp_path)?.len();
    println!(
        "  {} -> {} ({:.2} KB)",
        plugin_name,
        tp_path.display(),
        tp_size as f64 / 1024.0
    );

    Ok(())
}
