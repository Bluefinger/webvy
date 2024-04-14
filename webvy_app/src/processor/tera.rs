use std::path::{Path, PathBuf};

use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::With,
    system::{Commands, In, IntoSystem, Query, Res, Resource},
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
    app::{PostProcess, Write},
    deferred::DeferredTask,
    file::{FileName, FilePath, HtmlBody, PageType},
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
        q_pages: Query<(Entity, &PageType, Option<&TemplateName>)>,
    ) {
        for (page, page_type, template) in q_pages.iter() {
            match (template, page_type.has_parent_name()) {
                (None, Some(parent)) => {
                    let mut path = PathBuf::new();
                    path.push(parent);
                    path.push(format!("{}.html", page_type));
                    commands.entity(page).insert(TemplateName(path));
                }
                (None, None) => {
                    let mut path = PathBuf::new();
                    path.push(format!("{}.html", page_type));
                    commands.entity(page).insert(TemplateName(path));
                }
                _ => {}
            };
        }
    }

    fn process_pages(
        q_config: Query<&OutputDir, With<FileConfig>>,
        q_pages: Query<(&TemplateName, &HtmlBody, &FileName, &FilePath)>,
        tera: Res<Self>,
    ) -> Vec<(PathBuf, String)> {
        let dir = q_config.single().path();

        info!("Rendering content to templates");

        q_pages
            .iter()
            .map(|(template_name, html, file_name, path)| {
                let output_path = dir.join(path.0.with_file_name(&file_name.0));

                let mut context = tera::Context::new();

                context.insert("content", &html.0);

                let content = tera
                    .templates
                    .render(template_name.0.to_str().unwrap(), &context)
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
            .add_systems(PostProcess, Self::index_templates)
            .add_systems(Write, Self::process_pages.pipe(Self::write_to_disk));
    }
}

impl Default for TeraProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Component)]
pub struct TemplateName(PathBuf);
