use std::{
    collections::HashMap,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::{With, Without},
    schedule::IntoSystemConfigs,
    system::{CommandQueue, Commands, EntityCommands, ParallelCommands, Query, Res, Resource},
    world::World,
};
use log::{error, info};
use pulldown_cmark::{html, Options, Parser};
use toml::Value;
use webvy_matterparser::Parser as FrontMatterParser;

use crate::{
    app::{Load, Process, ProcessorApp},
    deferred::DeferredTask,
    file::{FileName, FilePath, HtmlBody, PageType},
    files::read_from_directory,
    front_matter::{Date, Draft, Title},
    traits::{Extractor, ProcessorPlugin},
};

use super::configuration::{ContentDir, FileConfig};

pub struct MarkdownProcessor<T: Extractor> {
    _marker: PhantomData<T>,
}

impl<T: Extractor + Send + Sync> MarkdownProcessor<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    fn read_directory_task(
        q_config: Query<&ContentDir, With<FileConfig>>,
        deferred: Res<DeferredTask>,
    ) {
        let path = q_config.single().path().to_path_buf();

        deferred
            .scoped_task(|scope| async move {
                let mut command_queue = CommandQueue::default();

                info!("Reading markdown content from disk");

                let data = read_from_directory(path.as_path()).await?;

                command_queue.push(move |world: &mut World| {
                    world.spawn_batch(data.into_iter().scan(
                        path,
                        |origin, (page_path, content)| {
                            let page_path = page_path.strip_prefix(origin).unwrap().to_path_buf();

                            Some((FilePath(page_path), MarkdownPost(content)))
                        },
                    ));
                });

                scope.send(command_queue);

                Ok::<(), std::io::Error>(())
            })
            .detach();
    }

    fn parse_markdown(
        commands: ParallelCommands,
        q_pages: Query<(Entity, &MarkdownPost, &FilePath)>,
    ) {
        let matter = FrontMatterParser::default();

        q_pages.par_iter().for_each(|(page, content, path)| {
            if let Some(mut markdown) = matter.parse(&content.0) {
                commands.command_scope(move |mut commands| {
                    commands.entity(page).insert((
                        MarkdownBody(markdown.take_content()),
                        MarkdownFrontMatter(markdown.take_matter()),
                    ));
                });
            } else {
                error!("Couldn't parse page: {}", path.0.display());
            }
        });
    }

    fn parse_frontmatter(
        mut commands: Commands,
        q_markdown: Query<
            (Entity, &MarkdownFrontMatter, &FilePath),
            (With<MarkdownPost>, Without<MarkdownParsed>),
        >,
    ) {
        info!("Parsing frontmatter from loaded markdown pages");
        q_markdown
            .iter()
            .for_each(|(entity, front_matter, FilePath(path))| {
                let mut post = commands.entity(entity);

                front_matter.extract_from_path(&mut post, path);
                front_matter.extract(&mut post);

                post.insert(MarkdownParsed);
            });
    }

    fn convert_markdown_to_html(
        par_commands: ParallelCommands,
        q_markdown: Query<(Entity, &MarkdownBody), (With<MarkdownPost>, Without<HtmlBody>)>,
    ) {
        info!("Parsing frontmatter from markdown page");
        q_markdown
            .par_iter()
            .for_each(|(entity, MarkdownBody(body))| {
                let parser = Parser::new_ext(body, Options::all());
                let mut html = String::new();
                html::push_html(&mut html, parser);
                par_commands.command_scope(move |mut commands| {
                    commands.entity(entity).insert(HtmlBody(html));
                });
            });
    }

    fn index_sections(mut commands: Commands, q_pages: Query<(Entity, &FilePath)>) {
        let mut index: HashMap<PathBuf, Vec<Entity>> = HashMap::new();
        info!("Indexing pages into sections");
        for (page, path) in q_pages.iter() {
            let dir = path.0.parent().unwrap();
            let indexed = index.entry(dir.to_path_buf()).or_default();
            let is_root = dir.to_str().is_some_and(str::is_empty);

            let page_type = if !path.0.ends_with("_index.md") {
                indexed.push(page);
                if is_root {
                    PageType::Page
                } else {
                    PageType::Post(dir.to_str().unwrap().into())
                }
            } else if is_root {
                PageType::Index
            } else {
                PageType::Section(dir.to_str().unwrap().into())
            };

            commands.entity(page).insert(page_type);
        }

        commands.insert_resource(SectionIndex(index));
    }
}

impl<T: Extractor + Send + Sync + 'static> ProcessorPlugin for MarkdownProcessor<T> {
    fn register(self, app: &mut ProcessorApp) {
        app.add_systems(Load, Self::read_directory_task)
            .add_systems(
                Process,
                (
                    Self::parse_markdown,
                    Self::parse_frontmatter.after(Self::parse_markdown),
                    Self::convert_markdown_to_html.after(Self::parse_markdown),
                    Self::index_sections.after(Self::parse_markdown),
                ),
            );
    }
}

impl<T: Extractor + Send + Sync> Default for MarkdownProcessor<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Component)]
pub struct MarkdownFrontMatter(Option<toml::Table>);

impl MarkdownFrontMatter {
    pub fn access(&self) -> Option<&toml::Table> {
        self.0.as_ref()
    }
}

impl Extractor for MarkdownFrontMatter {
    fn extract(&self, entity: &mut EntityCommands) {
        if let Some(data) = self.access() {
            if let Some(title) = data.get("title").map(Value::to_string) {
                entity.insert(Title(title));
            }

            if let Some(date) = data.get("date").map(Value::to_string) {
                entity.insert(Date(date));
            }

            if data
                .get("draft")
                .and_then(|value| value.as_bool())
                .is_some_and(|draft| draft)
            {
                entity.insert(Draft);
            }
        }
    }

    fn extract_from_path(&self, entity: &mut EntityCommands, path: &Path) {
        if let Some(file_name) = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(|file_name| {
                if file_name.contains("_index") {
                    String::from("index.html")
                } else {
                    format!("{}.html", file_name.trim_end_matches(".md"))
                }
            })
        {
            entity.insert(FileName(file_name));
        }
    }
}

#[derive(Debug, Clone, Component)]
struct MarkdownBody(String);

#[derive(Debug, Component)]
pub struct MarkdownPost(String);

#[derive(Debug, Component)]
struct MarkdownParsed;

#[derive(Debug, Default, Resource)]
pub struct SectionIndex(pub HashMap<PathBuf, Vec<Entity>>);
