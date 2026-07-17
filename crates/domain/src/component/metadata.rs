use crate::component::Image;
use semver::Version;

#[derive(Debug)]
pub enum Origin {
    Core,
    Plugin,
}

impl Origin {
    pub fn is_core(&self) -> bool {
        matches!(self, Self::Core)
    }

    pub fn is_plugin(&self) -> bool {
        matches!(self, Self::Plugin)
    }
}

#[derive(Debug)]
pub struct Metadata<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub origin: Origin,
    pub description: Option<&'a str>,
    pub icon: Option<&'a Image>,
    pub version: &'a Version,
    pub authors: &'a [String],
}
