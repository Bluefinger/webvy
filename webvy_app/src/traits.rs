use std::path::Path;

use bevy_ecs::system::EntityCommands;

use crate::{
    app::ProcessorApp,
    file::{FileName, FilePath},
};
pub trait Extractor {
    fn extract(&self, entity: &mut EntityCommands);

    fn extract_path(&self, entity: &mut EntityCommands, path: &Path) {
        entity.insert(default_from_path(path));
    }
}

fn default_from_path(path: &Path) -> (FileName, FilePath) {
    let path_string = path.to_string_lossy().into_owned();
    let file_name = path
        .extension()
        .map_or_else(
            || {
                path.file_name()
                    .map(|file_name| file_name.to_string_lossy().into_owned())
            },
            |file_ending| {
                path.file_name().map(|file_name| {
                    file_name
                        .to_string_lossy()
                        .trim_end_matches(&format!(".{}", file_ending.to_string_lossy().as_ref()))
                        .to_owned()
                })
            },
        )
        .expect("No file name in path");

    (FileName(file_name), FilePath(path_string))
}

pub trait ProcessorPlugin {
    fn register(self, app: &mut ProcessorApp);
}
