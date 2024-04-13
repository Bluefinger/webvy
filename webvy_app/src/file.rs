use std::path::PathBuf;

use bevy_ecs::component::Component;

#[derive(Debug, Component, Clone)]
pub struct FileName(pub String);

#[derive(Debug, Component, Clone)]
pub struct FilePath(pub String);

#[derive(Debug, Component, Clone)]
pub struct FilePathBuf(pub PathBuf);

#[derive(Debug, Component, Clone)]
pub struct HtmlBody(pub String);

#[derive(Debug, Component)]
pub enum PageType {
    Index,
    Section,
    Page,
    Post,
}

impl From<&PageType> for &'static str {
    fn from(value: &PageType) -> Self {
        match value {
            PageType::Index => "index.html",
            PageType::Section => "section.html",
            PageType::Page => "page.html",
            PageType::Post => "post.html",
        }
    }
}
