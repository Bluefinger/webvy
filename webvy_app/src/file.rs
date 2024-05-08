use std::path::{Path, PathBuf};

use bevy_ecs::{component::Component, system::Command};
use log::trace;

#[derive(Debug, Component, Clone)]
pub struct FileName(pub String);

#[derive(Debug, Component, Clone)]
pub struct FilePath(PathBuf);

impl FilePath {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }
}

impl AsRef<Path> for FilePath {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

#[derive(Debug, Component, Clone)]
pub struct HtmlBody(Box<str>);

impl HtmlBody {
    pub fn new(body: String) -> Self {
        Self(body.into_boxed_str())
    }
}

impl AsRef<str> for HtmlBody {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Component, Clone)]
pub struct SectionName(Box<str>);

impl AsRef<str> for SectionName {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

#[derive(Debug, Component, PartialEq, Eq, Hash, Clone, Copy)]
pub enum PageType {
    Index,
    Page,
    Section,
    Post,
}

pub(crate) struct EnumeratedSections(Box<str>);

impl EnumeratedSections {
    pub fn new(path: PathBuf) -> Option<Self> {
        Some(Self(path.file_stem()?.to_str()?.into()))
    }
}

impl Command for EnumeratedSections {
    fn apply(self, world: &mut bevy_ecs::world::World) {
        trace!("Enumerated section: {}", self.0);
        world.spawn_batch([
            (PageType::Post, SectionName(self.0.clone())),
            (PageType::Section, SectionName(self.0)),
        ]);
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
