use std::collections::{BTreeMap, BTreeSet};

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    prelude::*,
};

use crate::{GameEntityId, PlayerId};

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GameObject;

#[derive(Component, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[component(immutable)]
pub struct DefinitionId(pub String);

#[derive(Component, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum EntityKind {
    Player,
    Hero,
    Minion,
    Spell,
    Weapon,
    HeroCard,
    HeroPower,
    Location,
    Permanent,
    Dormant,
    Enchantment,
    Secret,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub struct Controller(pub PlayerId);

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct DisplayName(pub String);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayOrder(pub u64);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BaseStats {
    pub attack: i32,
    pub health: i32,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CurrentStats {
    pub attack: i32,
    pub maximum_health: i32,
}

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Damage(pub i32);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Armor(pub i32);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PendingDestroy;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Keyword {
    Charge,
    DivineShield,
    Immune,
    Lifesteal,
    Poisonous,
    Rush,
    Stealth,
    Taunt,
    Windfury,
}

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct Keywords(pub BTreeSet<Keyword>);

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct Abilities(pub Vec<String>);

#[derive(Component, Clone, Debug, Default, Eq, PartialEq)]
pub struct Enchantments(pub Vec<GameEntityId>);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttackState {
    pub attacks_this_turn: u8,
    pub exhausted: bool,
}

#[derive(Component, Clone, Debug, Eq, PartialEq)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub maximum_resources: i32,
    pub used_resources: i32,
    pub temporary_resources: i32,
    pub pending_overload: i32,
    pub locked_overload: i32,
    pub resources_spent: i32,
    pub fatigue: u32,
}

impl Player {
    pub fn available_resources(&self) -> i32 {
        self.maximum_resources + self.temporary_resources
            - self.used_resources
            - self.locked_overload
    }
}

#[derive(Clone, Debug, Default, Resource)]
pub struct GameEntityIndex(pub BTreeMap<GameEntityId, Entity>);

#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct NextGameEntityId(pub u64);

#[derive(Clone, Copy, Debug, Default, Resource)]
pub struct PlayOrderCounter(pub u64);

pub(crate) fn allocate_game_id(world: &mut World) -> GameEntityId {
    let mut next = world.resource_mut::<NextGameEntityId>();
    let id = GameEntityId(next.0);
    next.0 = next.0.checked_add(1).expect("game entity ID overflow");
    id
}

pub(crate) fn allocate_play_order(world: &mut World) -> PlayOrder {
    let mut next = world.resource_mut::<PlayOrderCounter>();
    let order = PlayOrder(next.0);
    next.0 = next.0.checked_add(1).expect("play order overflow");
    order
}

pub(crate) fn index_game_entity_hook(mut world: DeferredWorld, context: HookContext) {
    let id = *world
        .get::<GameEntityId>(context.entity)
        .expect("GameEntityId exists during its add hook");
    let replaced = world
        .resource_mut::<GameEntityIndex>()
        .0
        .insert(id, context.entity);
    assert!(
        replaced.is_none(),
        "duplicate logical game entity ID {id:?}"
    );
}

pub(crate) fn unindex_game_entity_hook(mut world: DeferredWorld, context: HookContext) {
    let id = *world
        .get::<GameEntityId>(context.entity)
        .expect("GameEntityId exists during its remove hook");
    let removed = world.resource_mut::<GameEntityIndex>().0.remove(&id);
    assert_eq!(
        removed,
        Some(context.entity),
        "game entity index drift for {id:?}"
    );
}

pub(crate) fn game_entity(world: &World, id: GameEntityId) -> Option<Entity> {
    world.resource::<GameEntityIndex>().0.get(&id).copied()
}
