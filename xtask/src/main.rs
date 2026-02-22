use crate::commands::{build_plugins, setup};
use anyhow::{Context, Result, bail};
use std::{env, path::PathBuf, process::Command};

mod commands;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("setup") => setup()?,
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
