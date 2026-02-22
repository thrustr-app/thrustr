use std::{env, fs, process};

fn main() {
    let path = env::args().nth(1).expect("No commit-msg file provided");
    let msg = fs::read_to_string(&path).expect("Could not read file");
    let first_line = msg.lines().next().unwrap_or("").trim();

    let valid_types = [
        "feat", "fix", "docs", "chore", "refactor", "test", "style", "ci",
    ];
    let is_valid = valid_types.iter().any(|t| {
        first_line.starts_with(&format!("{t}:")) || first_line.starts_with(&format!("{t}("))
    });

    if !is_valid {
        eprintln!("❌ Invalid commit message: '{first_line}'");
        eprintln!("   Expected format: <type>(<scope>): <description>");
        eprintln!("   Valid types: {}", valid_types.join(", "));
        process::exit(1);
    }

    println!("✅ Commit message is valid.");
}
