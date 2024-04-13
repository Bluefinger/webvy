use bevy_ecs::component::Component;

#[derive(Debug, Default, Clone, Component)]
pub struct Title(pub String);

#[derive(Debug, Default, Clone, Component)]
pub struct Date(pub String);

#[derive(Debug, Clone, Component)]
pub struct Draft;
