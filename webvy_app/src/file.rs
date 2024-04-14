use std::path::{Path, PathBuf};

use bevy_ecs::component::Component;

#[derive(Debug, Component, Clone)]
pub struct FileName(pub String);

#[derive(Debug, Component, Clone)]
pub struct FilePath(pub PathBuf);

#[derive(Debug, Component, Clone)]
pub struct HtmlBody(pub String);

#[derive(Debug, Component, PartialEq, Eq, Hash, Clone)]
pub enum PageType {
    Index,
    Page,
    Section(Box<str>),
    Post(Box<str>),
}

impl PageType {
    pub fn has_parent_name(&self) -> Option<&str> {
        match self {
            PageType::Index => None,
            PageType::Page => None,
            PageType::Section(name) => Some(name.as_ref()),
            PageType::Post(name) => Some(name.as_ref()),
        }
    }
}

impl From<&PageType> for &'static str {
    fn from(value: &PageType) -> Self {
        match value {
            PageType::Index => "index",
            PageType::Page => "page",
            PageType::Section(_) => "section",
            PageType::Post(_) => "post",
        }
    }
}

impl std::fmt::Display for PageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", Into::<&str>::into(self))
    }
}

impl AsRef<Path> for PageType {
    fn as_ref(&self) -> &Path {
        Into::<&str>::into(self).as_ref()
    }
}
