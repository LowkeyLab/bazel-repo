use bevy::prelude::*;

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[relationship(relationship_target = NestedFrames)]
pub struct NestedUnder(#[relationship] pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = NestedUnder, linked_spawn)]
pub struct NestedFrames(#[relationship] Vec<Entity>);

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[relationship(relationship_target = QueueEntries)]
pub struct QueuedIn(#[relationship] pub Entity);

#[derive(Component, Debug)]
#[relationship_target(relationship = QueuedIn, linked_spawn)]
pub struct QueueEntries(#[relationship] Vec<Entity>);
