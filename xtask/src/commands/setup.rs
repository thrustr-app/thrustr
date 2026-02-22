use anyhow::Result;
use std::{fs, process::Command};

pub fn setup() -> Result<()> {
    println!("Setting up commit linting...");

    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "commit_lint"])
        .status()?;
    if !status.success() {
        anyhow::bail!("Failed to build commit_lint");
    }

    fs::copy("hooks/commit-msg", ".git/hooks/commit-msg")?;
    #[cfg(unix)]
    {
        use std::{fs::Permissions, os::unix::fs::PermissionsExt};
        fs::set_permissions(".git/hooks/commit-msg", Permissions::from_mode(0o755))?;
    }
    println!("Done! Commit linting is set up.");
    Ok(())
}
