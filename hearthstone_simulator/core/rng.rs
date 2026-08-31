use bevy::prelude::Resource;

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

    pub fn next_u64(&mut self) -> u64 {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RngSnapshot {
    pub algorithm_version: u32,
    pub state: u64,
    pub position: u64,
}
