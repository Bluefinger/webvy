use std::path::{Path, PathBuf};

use bevy_ecs::{
    component::Component,
    system::{CommandQueue, Res, Resource},
    world::World,
};
use log::{error, info};
use smol::fs::read_to_string;
use toml::{Table, Value};

use crate::{
    app::{Preload, ProcessorApp},
    deferred::DeferredTask,
    traits::ProcessorPlugin,
};

#[derive(Debug, Clone, Resource)]
pub struct ConfigurationProcessor(PathBuf);

impl ConfigurationProcessor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
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
            .add_systems(Preload, Self::init_config);
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
