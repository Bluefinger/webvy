use std::path::{Path, PathBuf};

use bevy_ecs::{
    component::Component,
    query::With,
    system::{CommandQueue, Commands, Query, Res, Resource},
    world::World,
};
use log::{error, info};
use smol::{
    fs::{read_dir, read_to_string},
    stream::StreamExt,
};
use toml::{Table, Value};

use crate::{
    app::{Load, Preload, ProcessorApp},
    deferred::DeferredTask,
    file::{EnumeratedSections, PageType},
    traits::ProcessorPlugin,
};

#[derive(Debug, Clone, Resource)]
pub struct ConfigurationProcessor(PathBuf);

impl ConfigurationProcessor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    fn init_section_page_types(
        mut commands: Commands,
        q_config: Query<&ContentDir, With<FileConfig>>,
        deferred: Res<DeferredTask>,
    ) {
        let path = q_config.single().path().to_path_buf();

        commands.spawn_batch([PageType::Index, PageType::Page]);

        deferred
            .scoped_task(|ex| async move {
                match Self::read_first_level_directory(path.as_path()).await {
                    Ok(commands) => ex.send(commands),
                    Err(err) => error!("Unable to read content directory: {}", err),
                }
            })
            .detach();
    }

    async fn read_first_level_directory(path: &Path) -> std::io::Result<CommandQueue> {
        let mut entry = read_dir(path).await?;

        let to_visit = CommandQueue::default();

        entry.try_fold(to_visit, |mut queue, entry| {
            let path = entry.path();

            if let Some(section) = path.is_dir().then(|| EnumeratedSections::new(path)).flatten() {
                queue.push(section);
            }

            Ok(queue)
        }).await
    }

    fn init_config(config_path: Res<Self>, deferred: Res<DeferredTask>) {
        let path = config_path.0.to_path_buf();

        deferred
            .scoped_task(|scope| async move {
                info!("Reading and loading configuration");
                if let Err(e) = read_to_string(path.as_path())
                    .await
                    .map(|config_file| {
                        let mut queue = CommandQueue::default();

                        queue.push(move |commands: &mut World| {
                            match toml::from_str::<Table>(&config_file) {
                                Ok(config_file) => {
                                    if let Some(files) =
                                        config_file.get("files").and_then(Value::as_table)
                                    {
                                        let mut file_config = commands.spawn(FileConfig);

                                        if let Some(content) =
                                            files.get("content").and_then(Value::as_str)
                                        {
                                            file_config.insert(ContentDir::new(content));
                                        }

                                        if let Some(output) =
                                            files.get("output").and_then(Value::as_str)
                                        {
                                            file_config.insert(OutputDir::new(output));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Error with deserializing: {}", e);
                                }
                            };
                        });

                        queue
                    })
                    .map(|queue| scope.send(queue))
                {
                    error!("Error with reading: {}", e);
                }
            })
            .detach();
    }
}

impl ProcessorPlugin for ConfigurationProcessor {
    fn register(self, app: &mut ProcessorApp) {
        app.insert_resource(self)
            .add_systems(Preload, Self::init_config)
            .add_systems(Load, Self::init_section_page_types);
    }
}

#[derive(Debug, Component)]
pub struct ContentDir(PathBuf);

impl ContentDir {
    fn new(dir: impl Into<PathBuf>) -> Self {
        Self(dir.into())
    }

    pub fn path(&self) -> &Path {
        self.0.as_path()
    }
}

#[derive(Debug, Component)]
pub struct FileConfig;

#[derive(Debug, Component)]
pub struct OutputDir(PathBuf);

impl OutputDir {
    fn new(dir: impl Into<PathBuf>) -> Self {
        Self(dir.into())
    }

    pub fn path(&self) -> &Path {
        self.0.as_path()
    }
}
