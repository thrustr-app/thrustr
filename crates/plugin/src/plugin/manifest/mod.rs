use super::windows::is_device_name;
use domain::component::{ConfigSchema, LoginForm};
use semver::Version;
use serde::Deserialize;
use std::{fmt, sync::Arc};
use thiserror::Error;

mod form;

#[derive(Deserialize, Debug)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    #[serde(default, deserialize_with = "form::login_form")]
    pub auth: Option<LoginForm>,
    #[serde(default, deserialize_with = "form::config_schema")]
    pub config: Option<ConfigSchema>,
}

#[derive(Deserialize, Debug)]
pub struct PluginInfo {
    pub id: PluginId,
    pub name: String,
    #[serde(default)]
    pub authors: Vec<String>,
    pub version: Version,
    pub description: Option<String>,
    #[serde(default, rename = "allowed-hosts")]
    pub allowed_hosts: Arc<[String]>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct PluginId(String);

const MIN_ID_LEN: usize = 3;
const MAX_ID_LEN: usize = 32;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum InvalidId {
    #[error("plugin id must be {MIN_ID_LEN} to {MAX_ID_LEN} characters long")]
    Length,
    #[error("plugin id may only contain lowercase ASCII letters, digits, `.`, `-` and `_`")]
    Characters,
    #[error("plugin id must start and end with a letter or digit")]
    Ends,
    #[error("plugin id is a reserved file name on Windows")]
    Reserved,
}

impl PluginId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PluginId {
    type Error = InvalidId;

    fn try_from(id: String) -> Result<Self, InvalidId> {
        let alphanumeric = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
        let allowed = |c: char| alphanumeric(c) || matches!(c, '.' | '-' | '_');

        if !(MIN_ID_LEN..=MAX_ID_LEN).contains(&id.len()) {
            return Err(InvalidId::Length);
        }
        if !id.chars().all(allowed) {
            return Err(InvalidId::Characters);
        }
        if !(id.starts_with(alphanumeric) && id.ends_with(alphanumeric)) {
            return Err(InvalidId::Ends);
        }
        if is_device_name(&id) {
            return Err(InvalidId::Reserved);
        }

        Ok(Self(id))
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(id: &str) -> Result<(), InvalidId> {
        PluginId::try_from(id.to_owned()).map(drop)
    }

    #[track_caller]
    fn check(id: &str, expected: Result<(), InvalidId>) {
        assert_eq!(validate(id), expected, "id {id:?}");
    }

    #[test]
    fn accepts_lowercase_ids_with_separators() {
        check("itchio", Ok(()));
        check("legacy-games", Ok(()));
        check("com.example.store", Ok(()));
        check("my_store2", Ok(()));
        check("7z-tools", Ok(()));
    }

    #[test]
    fn ids_must_be_lowercase() {
        check("Itchio", Err(InvalidId::Characters));
        check("legacyGames", Err(InvalidId::Characters));
        check("ITCHIO", Err(InvalidId::Characters));
    }

    #[test]
    fn ids_must_be_inside_length_range() {
        check("", Err(InvalidId::Length));
        check("ab", Err(InvalidId::Length));
        check("abc", Ok(()));
        check(&"a".repeat(MAX_ID_LEN), Ok(()));
        check(&"a".repeat(MAX_ID_LEN + 1), Err(InvalidId::Length));
    }

    #[test]
    fn only_lowercase_letters_digits_and_separators_are_allowed() {
        let non_ascii = ['é', 'ß', 'İ', '٣', '\u{200b}'];
        for c in (0..=127u8).map(char::from).chain(non_ascii) {
            let allowed = c.is_ascii_lowercase() || c.is_ascii_digit() || ".-_".contains(c);
            let expected = if allowed {
                Ok(())
            } else {
                Err(InvalidId::Characters)
            };
            check(&format!("a{c}b"), expected);
        }
    }

    #[test]
    fn ids_must_start_and_end_with_a_letter_or_digit() {
        check("...", Err(InvalidId::Ends));
        check(".hidden", Err(InvalidId::Ends));
        check("trailing.", Err(InvalidId::Ends));
        check("-dash", Err(InvalidId::Ends));
        check("dash-", Err(InvalidId::Ends));
        check("_under_", Err(InvalidId::Ends));
    }

    #[test]
    fn windows_device_names_are_rejected() {
        check("con", Err(InvalidId::Reserved));
        check("nul.store", Err(InvalidId::Reserved));
        check("com1", Err(InvalidId::Reserved));
        check("lpt0.store", Err(InvalidId::Reserved));
        check("console", Ok(()));
    }
}
