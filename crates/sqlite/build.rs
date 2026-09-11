use std::path::PathBuf;

fn main() {
    let include = std::env::var_os("DEP_SQLITE3_INCLUDE")
        .map(PathBuf::from)
        .expect("libsqlite3-sys should export DEP_SQLITE3_INCLUDE for the bundled build");

    cc::Build::new()
        .file("vendor/spellfix.c")
        .include(&include)
        .warnings(false)
        .compile("spellfix");

    println!("cargo:rerun-if-changed=vendor/spellfix.c");
    println!("cargo:rerun-if-changed=build.rs");
}
