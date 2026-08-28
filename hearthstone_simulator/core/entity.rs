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
pub struct ComputedStats {
    pub attack: i32,
    pub maximum_health: i32,
}

pub type CurrentStats = ComputedStats;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Damage(pub i32);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Armor(pub i32);

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PendingDestroy;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
#[require(GameObject, BaseStats, ComputedStats, Damage, AttackState)]
pub struct StatBearing;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
#[require(StatBearing, Armor)]
pub struct HeroForm;

#[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
#[require(StatBearing)]
pub struct MinionForm;

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

impl Keyword {
    pub const ALL: [Self; 9] = [
        Self::Charge,
        Self::DivineShield,
        Self::Immune,
        Self::Lifesteal,
        Self::Poisonous,
        Self::Rush,
        Self::Stealth,
        Self::Taunt,
        Self::Windfury,
    ];
}

macro_rules! keyword_markers {
    ($($keyword:ident),+ $(,)?) => {
        $(
            #[derive(Component, Clone, Copy, Debug, Default, Eq, PartialEq)]
            #[require(GameObject)]
            pub struct $keyword;
        )+
    };
}

keyword_markers!(
    Charge,
    DivineShield,
    Immune,
    Lifesteal,
    Poisonous,
    Rush,
    Stealth,
    Taunt,
    Windfury,
);

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

pub(crate) fn assert_runtime_shape_invariants(world: &World) -> Result<(), String> {
    for entity in world.iter_entities() {
        let has_hero_form = entity.contains::<HeroForm>();
        let has_minion_form = entity.contains::<MinionForm>();
        if has_hero_form && has_minion_form {
            return Err("entity has conflicting runtime forms".to_string());
        }
        match entity.get::<EntityKind>() {
            Some(EntityKind::Hero) if !has_hero_form => {
                return Err("Hero entity has an invalid runtime form".to_string());
            }
            Some(EntityKind::Minion) if !has_minion_form => {
                return Err("minion entity has an invalid runtime form".to_string());
            }
            Some(EntityKind::Hero | EntityKind::Minion) => {}
            _ if has_hero_form => {
                return Err("non-Hero entity has a Hero runtime form".to_string());
            }
            _ if has_minion_form => {
                return Err("non-minion entity has a minion runtime form".to_string());
            }
            _ => {}
        }
        if (has_hero_form || has_minion_form) && !entity.contains::<StatBearing>() {
            return Err("runtime-form entity is missing StatBearing".to_string());
        }
        if has_hero_form && !entity.contains::<Armor>() {
            return Err("Hero-form entity is missing Armor".to_string());
        }
        if !has_hero_form && entity.contains::<Armor>() {
            return Err("non-Hero-form entity has Armor".to_string());
        }
        if entity.contains::<StatBearing>() && !(has_hero_form || has_minion_form) {
            return Err("entity without a runtime form has StatBearing".to_string());
        }
        if entity.contains::<StatBearing>()
            && (!entity.contains::<GameObject>()
                || !entity.contains::<BaseStats>()
                || !entity.contains::<ComputedStats>()
                || !entity.contains::<Damage>()
                || !entity.contains::<AttackState>())
        {
            return Err("stat-bearing entity is missing a required component".to_string());
        }
    }
    Ok(())
}

pub(crate) fn materialize_entity_form(world: &mut World, entity: Entity, kind: EntityKind) {
    let mut entity = world.entity_mut(entity);
    entity.remove::<(HeroForm, MinionForm, StatBearing, Armor)>();
    match kind {
        EntityKind::Hero => {
            entity.insert(HeroForm);
        }
        EntityKind::Minion => {
            entity.insert(MinionForm);
        }
        _ => {}
    }
}

pub(crate) fn has_keyword(world: &World, entity: Entity, keyword: Keyword) -> bool {
    match keyword {
        Keyword::Charge => world.get::<Charge>(entity).is_some(),
        Keyword::DivineShield => world.get::<DivineShield>(entity).is_some(),
        Keyword::Immune => world.get::<Immune>(entity).is_some(),
        Keyword::Lifesteal => world.get::<Lifesteal>(entity).is_some(),
        Keyword::Poisonous => world.get::<Poisonous>(entity).is_some(),
        Keyword::Rush => world.get::<Rush>(entity).is_some(),
        Keyword::Stealth => world.get::<Stealth>(entity).is_some(),
        Keyword::Taunt => world.get::<Taunt>(entity).is_some(),
        Keyword::Windfury => world.get::<Windfury>(entity).is_some(),
    }
}

pub(crate) fn entity_keywords(world: &World, entity: Entity) -> BTreeSet<Keyword> {
    Keyword::ALL
        .into_iter()
        .filter(|keyword| has_keyword(world, entity, *keyword))
        .collect()
}

pub(crate) fn insert_keyword(world: &mut World, entity: Entity, keyword: Keyword) {
    let mut entity = world.entity_mut(entity);
    match keyword {
        Keyword::Charge => entity.insert(Charge),
        Keyword::DivineShield => entity.insert(DivineShield),
        Keyword::Immune => entity.insert(Immune),
        Keyword::Lifesteal => entity.insert(Lifesteal),
        Keyword::Poisonous => entity.insert(Poisonous),
        Keyword::Rush => entity.insert(Rush),
        Keyword::Stealth => entity.insert(Stealth),
        Keyword::Taunt => entity.insert(Taunt),
        Keyword::Windfury => entity.insert(Windfury),
    };
}

pub(crate) fn remove_keyword(world: &mut World, entity: Entity, keyword: Keyword) {
    let mut entity = world.entity_mut(entity);
    match keyword {
        Keyword::Charge => entity.remove::<Charge>(),
        Keyword::DivineShield => entity.remove::<DivineShield>(),
        Keyword::Immune => entity.remove::<Immune>(),
        Keyword::Lifesteal => entity.remove::<Lifesteal>(),
        Keyword::Poisonous => entity.remove::<Poisonous>(),
        Keyword::Rush => entity.remove::<Rush>(),
        Keyword::Stealth => entity.remove::<Stealth>(),
        Keyword::Taunt => entity.remove::<Taunt>(),
        Keyword::Windfury => entity.remove::<Windfury>(),
    };
}

pub(crate) fn clear_keywords(world: &mut World, entity: Entity) {
    world.entity_mut(entity).remove::<(
        Charge,
        DivineShield,
        Immune,
        Lifesteal,
        Poisonous,
        Rush,
        Stealth,
        Taunt,
        Windfury,
    )>();
}

pub(crate) fn materialize_keywords(
    world: &mut World,
    entity: Entity,
    keywords: &BTreeSet<Keyword>,
) {
    clear_keywords(world, entity);
    for keyword in keywords {
        insert_keyword(world, entity, *keyword);
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[googletest::test]
    fn runtime_form_markers_materialize_required_structural_components() {
        let mut world = World::new();
        let minion = world.spawn(MinionForm).id();
        let hero = world.spawn(HeroForm).id();

        for entity in [minion, hero] {
            assert_that!(world.get::<GameObject>(entity).is_some(), is_true());
            assert_that!(world.get::<StatBearing>(entity).is_some(), is_true());
            assert_that!(
                world.get::<BaseStats>(entity),
                eq(Some(&BaseStats::default()))
            );
            assert_that!(
                world.get::<ComputedStats>(entity),
                eq(Some(&ComputedStats::default()))
            );
            assert_that!(world.get::<Damage>(entity), eq(Some(&Damage::default())));
            assert_that!(
                world.get::<AttackState>(entity),
                eq(Some(&AttackState::default()))
            );
        }
        assert_that!(world.get::<Armor>(minion).is_none(), is_true());
        assert_that!(world.get::<Armor>(hero), eq(Some(&Armor::default())));
    }

    #[googletest::test]
    fn runtime_shape_invariants_reject_kind_marker_drift() {
        let mut hero = World::new();
        hero.spawn((EntityKind::Hero, GameObject));
        assert_that!(
            assert_runtime_shape_invariants(&hero),
            err(eq(&"Hero entity has an invalid runtime form".to_string()))
        );

        let mut minion = World::new();
        minion.spawn((EntityKind::Minion, GameObject));
        assert_that!(
            assert_runtime_shape_invariants(&minion),
            err(eq(&"minion entity has an invalid runtime form".to_string()))
        );
    }

    #[googletest::test]
    fn runtime_shape_invariants_reject_inverse_and_conflicting_form_drift() {
        let mut non_hero = World::new();
        non_hero.spawn((EntityKind::Spell, HeroForm));
        assert_that!(
            assert_runtime_shape_invariants(&non_hero),
            err(eq(&"non-Hero entity has a Hero runtime form".to_string()))
        );

        let mut non_minion = World::new();
        non_minion.spawn((EntityKind::Spell, MinionForm));
        assert_that!(
            assert_runtime_shape_invariants(&non_minion),
            err(eq(
                &"non-minion entity has a minion runtime form".to_string()
            ))
        );

        let mut conflicting = World::new();
        conflicting.spawn((EntityKind::Hero, HeroForm, MinionForm));
        assert_that!(
            assert_runtime_shape_invariants(&conflicting),
            err(eq(&"entity has conflicting runtime forms".to_string()))
        );
    }

    #[googletest::test]
    fn runtime_shape_invariants_reject_missing_form_requirements() {
        let mut missing_stats = World::new();
        let minion = missing_stats.spawn((EntityKind::Minion, MinionForm)).id();
        missing_stats.entity_mut(minion).remove::<StatBearing>();
        assert_that!(
            assert_runtime_shape_invariants(&missing_stats),
            err(eq(&"runtime-form entity is missing StatBearing".to_string()))
        );

        let mut missing_armor = World::new();
        let hero = missing_armor.spawn((EntityKind::Hero, HeroForm)).id();
        missing_armor.entity_mut(hero).remove::<Armor>();
        assert_that!(
            assert_runtime_shape_invariants(&missing_armor),
            err(eq(&"Hero-form entity is missing Armor".to_string()))
        );

        let mut missing_structure = World::new();
        let minion = missing_structure
            .spawn((EntityKind::Minion, MinionForm))
            .id();
        missing_structure.entity_mut(minion).remove::<GameObject>();
        assert_that!(
            assert_runtime_shape_invariants(&missing_structure),
            err(eq(
                &"stat-bearing entity is missing a required component".to_string()
            ))
        );
    }

    #[googletest::test]
    fn runtime_shape_invariants_reject_stale_form_owned_components() {
        let mut stale_armor = World::new();
        stale_armor.spawn((EntityKind::Minion, MinionForm, Armor(3)));
        assert_that!(
            assert_runtime_shape_invariants(&stale_armor),
            err(eq(&"non-Hero-form entity has Armor".to_string()))
        );

        let mut stale_stat_bearing = World::new();
        stale_stat_bearing.spawn((EntityKind::Spell, StatBearing));
        assert_that!(
            assert_runtime_shape_invariants(&stale_stat_bearing),
            err(eq(
                &"entity without a runtime form has StatBearing".to_string()
            ))
        );
    }

    #[googletest::test]
    fn runtime_form_and_keyword_materialization_replace_previous_shape() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        materialize_entity_form(&mut world, entity, EntityKind::Hero);
        world.entity_mut(entity).get_mut::<Armor>().unwrap().0 = 7;
        materialize_keywords(
            &mut world,
            entity,
            &BTreeSet::from([Keyword::DivineShield, Keyword::Taunt]),
        );
        assert_that!(world.get::<HeroForm>(entity).is_some(), is_true());
        assert_that!(world.get::<MinionForm>(entity).is_none(), is_true());
        assert_that!(has_keyword(&world, entity, Keyword::Taunt), is_true());

        materialize_entity_form(&mut world, entity, EntityKind::Minion);
        materialize_keywords(&mut world, entity, &BTreeSet::from([Keyword::Rush]));
        assert_that!(world.get::<HeroForm>(entity).is_none(), is_true());
        assert_that!(world.get::<MinionForm>(entity).is_some(), is_true());
        assert_that!(world.get::<StatBearing>(entity).is_some(), is_true());
        assert_that!(world.get::<Armor>(entity).is_none(), is_true());
        assert_that!(
            entity_keywords(&world, entity),
            eq(&BTreeSet::from([Keyword::Rush]))
        );

        materialize_entity_form(&mut world, entity, EntityKind::Spell);
        assert_that!(world.get::<MinionForm>(entity).is_none(), is_true());
        assert_that!(world.get::<StatBearing>(entity).is_none(), is_true());
        assert_that!(world.get::<Armor>(entity).is_none(), is_true());
    }

    #[googletest::test]
    fn every_keyword_marker_supports_insertion_detection_and_removal() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();

        for keyword in Keyword::ALL {
            insert_keyword(&mut world, entity, keyword);
            assert_that!(has_keyword(&world, entity, keyword), is_true());
            assert_that!(
                entity_keywords(&world, entity),
                eq(&BTreeSet::from([keyword]))
            );
            remove_keyword(&mut world, entity, keyword);
            assert_that!(has_keyword(&world, entity, keyword), is_false());
        }
    }

    #[googletest::test]
    fn despawning_a_game_entity_removes_its_logical_index_entry() {
        let mut world = World::new();
        world.init_resource::<GameEntityIndex>();
        let entity = world.spawn(GameEntityId(7)).id();

        assert_that!(game_entity(&world, GameEntityId(7)), eq(Some(entity)));
        assert_that!(world.despawn(entity), is_true());
        assert_that!(game_entity(&world, GameEntityId(7)), none());
    }
}
