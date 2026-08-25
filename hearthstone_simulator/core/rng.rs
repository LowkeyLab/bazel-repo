use bevy::prelude::Resource;

use crate::{CanonicalTrace, GameEntityId, TraceEntry};

pub const RNG_ALGORITHM_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Resource)]
pub struct DeterministicRng {
    state: u64,
    position: u64,
}

impl Default for DeterministicRng {
    fn default() -> Self {
        Self::new(0)
    }
}

impl DeterministicRng {
    pub const fn new(seed: u64) -> Self {
        Self {
            state: seed,
            position: 0,
        }
    }

    pub const fn state(&self) -> RngSnapshot {
        RngSnapshot {
            algorithm_version: RNG_ALGORITHM_VERSION,
            state: self.state,
            position: self.position,
        }
    }

    pub const fn from_snapshot(snapshot: RngSnapshot) -> Option<Self> {
        if snapshot.algorithm_version != RNG_ALGORITHM_VERSION {
            return None;
        }
        Some(Self {
            state: snapshot.state,
            position: snapshot.position,
        })
    }

    fn next_u64(&mut self) -> u64 {
        // SplitMix64 is specified here rather than delegated to a library so dependency upgrades
        // cannot alter replay behavior.
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.position += 1;
        value ^ (value >> 31)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RngSnapshot {
    pub algorithm_version: u32,
    pub state: u64,
    pub position: u64,
}

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
        let position = rng.position;
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

    use super::*;

    #[test]
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

        assert_eq!(select(42), select(42));
        assert_ne!(select(42).1, select(43).1);
    }
}
