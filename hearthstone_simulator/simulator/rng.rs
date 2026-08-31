use crate::{CanonicalTrace, DeterministicRng, GameEntityId, TraceEntry};

#[cfg(test)]
use crate::{RNG_ALGORITHM_VERSION, RngSnapshot};

pub(crate) fn choose_game_entity(
    world: &mut bevy::prelude::World,
    mut candidates: Vec<GameEntityId>,
) -> Option<GameEntityId> {
    candidates.sort_unstable();
    candidates.dedup();
    if candidates.is_empty() {
        return None;
    }
    let (position, index) = {
        let mut rng = world.resource_mut::<DeterministicRng>();
        let position = rng.state().position;
        let index = (rng.next_u64() % candidates.len() as u64) as usize;
        (position, index)
    };
    let selected = candidates[index];
    world
        .resource_mut::<CanonicalTrace>()
        .entries
        .push(TraceEntry::RngChoice {
            position,
            candidates,
            selected,
        });
    Some(selected)
}

#[cfg(test)]
mod tests {
    use bevy::prelude::World;
    use googletest::prelude::*;

    use super::*;

    #[googletest::test]
    fn same_seed_and_candidates_produce_same_traceable_choice() {
        fn select(seed: u64) -> (GameEntityId, RngSnapshot) {
            let mut world = World::new();
            world.insert_resource(DeterministicRng::new(seed));
            world.init_resource::<CanonicalTrace>();
            let selected = choose_game_entity(
                &mut world,
                vec![GameEntityId(9), GameEntityId(2), GameEntityId(5)],
            )
            .unwrap();
            (selected, world.resource::<DeterministicRng>().state())
        }

        assert_that!(select(42), eq(select(42)));
        assert_that!(select(42).1, not(eq(select(43).1)));
    }

    #[googletest::test]
    fn snapshots_validate_versions_and_empty_choices_do_not_advance() {
        let snapshot = RngSnapshot {
            algorithm_version: RNG_ALGORITHM_VERSION,
            state: 17,
            position: 4,
        };
        assert_that!(
            DeterministicRng::from_snapshot(snapshot).unwrap().state(),
            eq(snapshot)
        );
        assert_that!(
            DeterministicRng::from_snapshot(RngSnapshot {
                algorithm_version: RNG_ALGORITHM_VERSION + 1,
                ..snapshot
            }),
            none()
        );

        let mut world = World::new();
        world.insert_resource(DeterministicRng::from_snapshot(snapshot).unwrap());
        world.init_resource::<CanonicalTrace>();
        assert_that!(choose_game_entity(&mut world, Vec::new()), none());
        assert_that!(world.resource::<DeterministicRng>().state(), eq(snapshot));

        let selected = choose_game_entity(
            &mut world,
            vec![GameEntityId(3), GameEntityId(1), GameEntityId(3)],
        )
        .unwrap();
        assert_that!(
            world.resource::<CanonicalTrace>().entries.last(),
            eq(Some(&TraceEntry::RngChoice {
                position: 4,
                candidates: vec![GameEntityId(1), GameEntityId(3)],
                selected,
            }))
        );
    }
}
