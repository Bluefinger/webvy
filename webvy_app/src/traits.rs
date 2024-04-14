use std::path::Path;

use bevy_ecs::system::EntityCommands;

use crate::app::ProcessorApp;
pub trait Extractor {
    fn extract(&self, entity: &mut EntityCommands);

    fn extract_from_path(&self, _entity: &mut EntityCommands, _path: &Path) {}
}

pub trait ProcessorPlugin {
    fn register(self, app: &mut ProcessorApp);
}
