use anyhow::Result;
use std::fs;

pub fn setup() -> Result<()> {
    println!("Setting up git hooks...");

    for entry in fs::read_dir(".hooks")? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let dest = format!(".git/hooks/{}", entry.file_name().to_string_lossy());
        fs::copy(entry.path(), &dest)?;
        #[cfg(unix)]
        {
            use std::{fs::Permissions, os::unix::fs::PermissionsExt};
            fs::set_permissions(&dest, Permissions::from_mode(0o755))?;
        }
    }
    println!("Done! Git hooks are set up.");
    Ok(())
}
