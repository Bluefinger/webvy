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
    system::{
        CommandQueue, Commands, EntityCommands, In, IntoSystem, ParallelCommands, Query, Res,
        ResMut, Resource,
    },
    world::World,
};
use bevy_tasks::{ComputeTaskPool, Scope};
use webvy_matterparser::Parser as FrontMatterParser;
use log::{error, info, trace};
use pulldown_cmark::{html, Options, Parser};
use toml::Value;

use crate::{
    app::{Load, Process, ProcessorApp},
    deferred::DeferredTask,
    file::{FileName, FilePathBuf, HtmlBody, PageType},
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

                let data = read_from_directory(path).await?;

                command_queue.push(move |world: &mut World| {
                    let mut file_data = world.resource_mut::<MarkdownFiles>();

                    file_data.0 = data;
                });

                scope.send(command_queue);

                Ok::<(), std::io::Error>(())
            })
            .detach();
    }

    fn parse_markdown(
        bodies: Res<MarkdownFiles>,
        mut commands: Commands,
        q_config: Query<&ContentDir, With<FileConfig>>,
    ) -> std::io::Result<()> {
        let pool = ComputeTaskPool::get();
        let matter = FrontMatterParser::default();
        let config = q_config.single();

        let posts = pool.scope(|s: &Scope<Result<_, std::io::Error>>| {
            info!(
                "Parsing markdown content. {} pages to render",
                bodies.0.len()
            );
            bodies.0.iter().for_each(|(path, body)| {
                let matter = &matter;
                let origin = config.path();
                s.spawn(async move {
                    let mut markdown = matter.parse(body).ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Unable to parse markdown frontmatter",
                        )
                    })?;

                    let path = path
                        .strip_prefix(origin)
                        .map_err(std::io::Error::other)?
                        .to_path_buf();

                    trace!("Page: {}", path.as_path().display());

                    Ok((
                        MarkdownPost,
                        MarkdownBody(markdown.take_content()),
                        MarkdownFrontMatter(markdown.take_matter()),
                        FilePathBuf(path),
                    ))
                })
            });
        });

        commands.spawn_batch(
            posts
                .into_iter()
                .collect::<Result<Vec<_>, std::io::Error>>()?,
        );

        Ok(())
    }

    fn handle_io_error(In(err): In<std::io::Result<()>>) {
        if let Err(e) = err {
            error!("Error parsing markdown: {}", e);
        }
    }

    fn parse_frontmatter(
        mut commands: Commands,
        q_markdown: Query<
            (Entity, &MarkdownFrontMatter, &FilePathBuf),
            (With<MarkdownPost>, Without<MarkdownParsed>),
        >,
    ) {
        info!("Parsing frontmatter from loaded markdown pages");
        q_markdown
            .iter()
            .for_each(|(entity, front_matter, FilePathBuf(path))| {
                let mut post = commands.entity(entity);

                front_matter.extract_path(&mut post, path);
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

    fn index_sections(
        mut commands: Commands,
        q_pages: Query<(Entity, &FilePathBuf)>,
        mut section_index: ResMut<SectionIndex>,
    ) {
        info!("Indexing pages into sections");
        for (page, path) in q_pages.iter() {
            let dir = path.0.parent().unwrap();
            let indexed = section_index.0.entry(dir.to_path_buf()).or_default();
            let is_root = dir.to_str().is_some_and(str::is_empty);

            let page_type = if !path.0.ends_with("_index.md") {
                indexed.push(page);
                if is_root {
                    PageType::Page
                } else {
                    PageType::Post
                }
            } else if is_root {
                PageType::Index
            } else {
                PageType::Section
            };

            commands.entity(page).insert(page_type);
        }
    }
}

impl<T: Extractor + Send + Sync + 'static> ProcessorPlugin for MarkdownProcessor<T> {
    fn register(self, app: &mut ProcessorApp) {
        app.add_systems(Load, Self::read_directory_task)
            .add_systems(
                Process,
                (
                    Self::parse_markdown.pipe(Self::handle_io_error),
                    Self::parse_frontmatter.after(Self::parse_markdown),
                    Self::convert_markdown_to_html.after(Self::parse_markdown),
                    Self::index_sections.after(Self::parse_markdown),
                ),
            )
            .init_resource::<MarkdownFiles>()
            .init_resource::<SectionIndex>();
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

    fn extract_path(&self, entity: &mut EntityCommands, path: &Path) {
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
pub struct MarkdownPost;

#[derive(Debug, Component)]
struct MarkdownParsed;

#[derive(Debug, Default, Resource)]
pub struct SectionIndex(pub HashMap<PathBuf, Vec<Entity>>);

#[derive(Debug, Default, Resource)]
struct MarkdownFiles(Vec<(PathBuf, String)>);
