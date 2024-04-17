use std::path::{Path, PathBuf};

use bevy_ecs::component::Component;

#[derive(Debug, Component, Clone)]
pub struct FileName(pub String);

#[derive(Debug, Component, Clone)]
pub struct FilePath(pub PathBuf);

#[derive(Debug, Component, Clone)]
pub struct HtmlBody(pub String);

#[derive(Debug, Component, Clone)]
pub struct SectionName(pub Box<str>);

#[derive(Debug, Component, PartialEq, Eq, Hash, Clone, Copy)]
pub enum PageType {
    Index,
    Page,
    Section,
    Post,
}

pub(crate) struct EnumeratedSections(PathBuf);

impl EnumeratedSections {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    pub fn into_page_type_bundles(self) -> Option<[(PageType, SectionName); 2]> {
        let name: Box<str> = self.0.file_stem()?.to_str()?.into();

        Some([
            (PageType::Post, SectionName(name.clone())),
            (PageType::Section, SectionName(name)),
        ])
    }
}

impl From<&PageType> for &'static str {
    fn from(value: &PageType) -> Self {
        match value {
            PageType::Index => "index",
            PageType::Page => "page",
            PageType::Section => "section",
            PageType::Post => "post",
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
