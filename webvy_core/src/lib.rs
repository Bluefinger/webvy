use bevy_ecs::system::EntityCommands;

pub trait Extractor {
    fn extract(&self, commands: &mut EntityCommands);
}
