use std::{env, fs, process};

fn main() {
    let path = env::args().nth(1).expect("No commit-msg file provided");
    let msg = fs::read_to_string(&path).expect("Could not read file");
    let first_line = msg.lines().next().unwrap_or("").trim();

    let valid_types = [
        ("feat", "A new feature"),
        ("fix", "A bug fix"),
        ("docs", "Documentation changes only"),
        ("chore", "Maintenance tasks, tooling, dependencies"),
        (
            "refactor",
            "Code change that neither fixes a bug nor adds a feature",
        ),
        ("test", "Adding or updating tests"),
        ("style", "Formatting, whitespace, missing semicolons, etc."),
        ("ci", "CI/CD configuration and scripts"),
    ];

    let is_valid = valid_types.iter().any(|(t, _)| {
        first_line.starts_with(&format!("{t}:")) || first_line.starts_with(&format!("{t}("))
    });

    if !is_valid {
        eprintln!("❌ Invalid commit message: '{first_line}'");
        eprintln!();
        eprintln!("   Expected format: <type>: <description>");
        eprintln!("   Or:              <type>(<scope>): <description>");
        eprintln!();
        eprintln!("   Valid types:");
        for (t, desc) in &valid_types {
            eprintln!("     {:<10} {}", t, desc);
        }
        process::exit(1);
    }
}
