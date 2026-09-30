mod manager;
mod plugin;
mod service;

pub use service::*;

mod wit {
    use wasmtime::component::bindgen;

    bindgen!({
        path: "../pdk/wit",
        world: "plugin-host",
    });

    /// Must match the `package` declaration in `crates/pdk/wit/*.wit`.
    const PACKAGE: &str = "thrustr:plugin";
    const VERSION: &str = "0.1.0";

    pub(crate) fn export_name(interface: &str) -> String {
        format!("{PACKAGE}/{interface}@{VERSION}")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{fs, path::Path};

        #[test]
        fn package_matches_wit() {
            let expected = format!("package {PACKAGE}@{VERSION};");
            let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pdk/wit");

            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "wit") {
                    let wit = fs::read_to_string(&path).unwrap();

                    assert!(
                        wit.lines().any(|l| l.trim() == expected),
                        "{} does not declare `{expected}`",
                        path.display()
                    );
                }
            }
        }
    }
}
