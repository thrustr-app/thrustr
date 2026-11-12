pub(super) fn is_device_name(name: &str) -> bool {
    let stem = name.split_once('.').map_or(name, |(stem, _)| stem);
    let stem = stem.trim_end_matches(' ');
    let (prefix, rest) = stem.split_at_checked(3).unwrap_or_default();
    let is = |device: &str| prefix.eq_ignore_ascii_case(device);

    let mut rest = rest.chars();
    match (rest.next(), rest.next()) {
        (None, _) => is("CON") || is("PRN") || is("AUX") || is("NUL"),
        (Some('0'..='9' | '¹' | '²' | '³'), None) => is("COM") || is("LPT"),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check(name: &str, expected: bool) {
        assert_eq!(is_device_name(name), expected, "{name:?}");
    }

    #[test]
    fn device_names_are_rejected() {
        let numbers = ('0'..='9').chain(['¹', '²', '³']);
        let numbered = numbers.flat_map(|n| [format!("COM{n}"), format!("LPT{n}")]);
        let devices = ["CON", "PRN", "AUX", "NUL"].map(String::from).into_iter();

        for device in devices.chain(numbered) {
            check(&device, true);
            check(&device.to_lowercase(), true);
            check(&format!("{device}.txt"), true);
            check(&format!("{device}.tar.gz"), true);
            check(&format!("{device} .txt"), true);
        }
    }

    #[test]
    fn other_names_are_accepted() {
        for name in [
            "",
            ".",
            "..",
            ".con",
            "x.con",
            "console",
            "auxiliary",
            "nul_",
            "CO",
            "COM",
            "COM10",
            "LPT01",
            "COMa",
            "COM¹¹",
            "名前",
        ] {
            check(name, false);
        }
    }
}
