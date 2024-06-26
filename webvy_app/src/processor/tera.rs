use std::path::{Path, PathBuf};

use bevy_ecs::{
    component::Component,
    entity::{Entity, EntityHashMap},
    query::{With, Without},
    system::{Commands, In, IntoSystem, Query, Res, ResMut, Resource},
};
use bevy_tasks::Task;
use log::{error, info, trace};
use smol::{
    fs::{DirBuilder, File},
    io::{AsyncWriteExt, BufWriter},
    stream::{iter, StreamExt},
};
use tera::Tera;

use crate::{
    app::{PostProcess, Process, Write},
    deferred::DeferredTask,
    file::{FileName, FilePath, HtmlBody, PageType, SectionName},
    traits::ProcessorPlugin,
};

use super::configuration::{FileConfig, OutputDir};

#[derive(Debug, Resource)]
pub struct TeraProcessor {
    templates: Tera,
}

impl TeraProcessor {
    pub fn new() -> Self {
        let templates = Tera::new("templates/**/*").unwrap();

        Self { templates }
    }

    fn index_templates(
        mut commands: Commands,
        q_page_types: Query<(Entity, &PageType, Option<&SectionName>), Without<TemplateName>>,
    ) {
        info!("Indexing templates");
        for (page, page_type, section_name) in q_page_types.iter() {
            match page_type {
                PageType::Index | PageType::Page => {
                    let path: PathBuf = [format!("{}.html", page_type)].into_iter().collect();
                    trace!("Indexed template as {}", path.display());
                    commands.entity(page).insert(TemplateName(path));
                }
                PageType::Section | PageType::Post => {
                    let parent = section_name.unwrap();
                    let path: PathBuf =
                        [parent.as_ref().to_string(), format!("{}.html", page_type)]
                            .into_iter()
                            .collect();
                    trace!("Indexed template as {}", path.display());
                    commands.entity(page).insert(TemplateName(path));
                }
            }
        }
    }

    fn associate_pages_to_templates(
        mut commands: Commands,
        q_pages: Query<(Entity, &FilePath), Without<AssociatedPageType>>,
        q_page_types: Query<(Entity, &PageType, Option<&SectionName>)>,
    ) {
        info!("Associating pages to templates");
        q_pages.iter().for_each(|(page, path)| {
            let dir = path.as_ref().parent().unwrap();
            let dir = dir.to_str().unwrap();
            let is_root = dir.is_empty();
            let is_listing = path.as_ref().ends_with("_index.md");

            let page_type = match (is_root, is_listing) {
                (true, true) => PageType::Index,
                (true, false) => PageType::Page,
                (false, true) => PageType::Section,
                (false, false) => PageType::Post,
            };

            q_page_types
                .iter()
                .find_map(|(page, kind, section)| match page_type {
                    PageType::Index | PageType::Page if page_type.eq(kind) => {
                        Some(AssociatedPageType(page))
                    }
                    PageType::Section | PageType::Post
                        if page_type.eq(kind)
                            && section.is_some_and(|section| dir.contains(section.as_ref())) =>
                    {
                        Some(AssociatedPageType(page))
                    }
                    _ => None,
                })
                .map_or_else(
                    || {
                        error!("{} doesn't exist. Maybe it hasn't been indexed?", page_type);
                    },
                    |associated_type| {
                        trace!("{} indexed as {}", path.as_ref().display(), page_type);
                        commands.entity(page).insert(associated_type);
                    },
                );
        });
    }

    fn populate_context(
        mut q_pages: Query<(Entity, &HtmlBody)>,
        mut contexts: ResMut<PageContexts>,
    ) {
        info!("Populating page contexts");
        for (page, content) in q_pages.iter_mut() {
            let context = contexts.0.entry(page).or_default();

            context.insert("content", content.as_ref());
        }
    }

    fn process_pages(
        q_config: Query<&OutputDir, With<FileConfig>>,
        q_pages: Query<(Entity, &AssociatedPageType, &FileName, &FilePath)>,
        q_page_types: Query<&TemplateName>,
        tera: Res<Self>,
        contexts: Res<PageContexts>,
    ) -> Vec<(PathBuf, String)> {
        let dir = q_config.single().path();

        info!("Rendering content to templates");

        q_pages
            .iter()
            .map(|(page, template_name, file_name, path)| {
                let output_path = dir.join(path.as_ref().with_file_name(&file_name.0));

                let template_name = q_page_types.get(template_name.0).unwrap();

                let context = contexts.0.get(&page).unwrap();

                let content = tera
                    .templates
                    .render(template_name.0.to_str().unwrap(), context)
                    .unwrap();

                (output_path, content)
            })
            .collect()
    }

    async fn write_file_to_disk(file: &Path, content: &[u8]) -> std::io::Result<()> {
        let mut file = BufWriter::new(File::create(file).await?);

        file.write_all(content).await?;

        file.flush().await?;

        Ok(())
    }

    fn write_to_disk(In(pages): In<Vec<(PathBuf, String)>>, deferred: Res<DeferredTask>) {
        deferred
            .scoped_task(|scope| async move {
                info!("Writing rendered content to disk");
                let stream: Vec<Task<_>> = iter(pages.into_iter())
                    .then(|(output_path, content)| async move {
                        if let Some(directory) = output_path.parent().filter(|path| !path.exists())
                        {
                            trace!("Creating directory: {}", directory.display());

                            if let Err(e) =
                                DirBuilder::new().recursive(true).create(directory).await
                            {
                                error!("Error creating directory {}: {}", directory.display(), e);
                            }
                        }

                        (output_path, content)
                    })
                    .map(|(output_path, content)| {
                        trace!("Spawning write task for {}", output_path.display());

                        scope.spawn(async move {
                            trace!("Writing {}", output_path.display());

                            Self::write_file_to_disk(output_path.as_path(), content.as_bytes())
                                .await
                        })
                    })
                    .collect()
                    .await;

                for handle in stream.into_iter() {
                    if let Err(e) = handle.await {
                        error!("Error writing to disk: {}", e);
                    };
                }
            })
            .detach();
    }
}

impl ProcessorPlugin for TeraProcessor {
    fn register(self, app: &mut crate::app::ProcessorApp) {
        app.insert_resource(self)
            .init_resource::<PageContexts>()
            .add_systems(Process, Self::index_templates)
            .add_systems(
                PostProcess,
                (Self::associate_pages_to_templates, Self::populate_context),
            )
            .add_systems(Write, Self::process_pages.pipe(Self::write_to_disk));
    }
}

impl Default for TeraProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Component)]
struct TemplateName(PathBuf);

#[derive(Debug, Component)]
struct AssociatedPageType(Entity);

#[derive(Debug, Default, Resource)]
struct PageContexts(EntityHashMap<tera::Context>);
