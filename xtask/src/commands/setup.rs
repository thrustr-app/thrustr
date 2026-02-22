use anyhow::Result;
use std::fs;

pub fn setup() -> Result<()> {
    println!("Setting up commit linting...");

    fs::copy(".hooks/commit-msg", ".git/hooks/commit-msg")?;
    #[cfg(unix)]
    {
        use std::{fs::Permissions, os::unix::fs::PermissionsExt};
        fs::set_permissions(".git/hooks/commit-msg", Permissions::from_mode(0o755))?;
    }
    println!("Done! Commit linting is set up.");
    Ok(())
}
