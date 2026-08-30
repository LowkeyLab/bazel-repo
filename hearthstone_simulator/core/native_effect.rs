use std::{collections::BTreeMap, sync::Arc};

use bevy::{ecs::system::SystemId, prelude::*};

use crate::{Effect, EffectContext};

#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct NativeEffectId(pub String);

impl NativeEffectId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for NativeEffectId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

pub(crate) type NativeEffectSystem = SystemId<In<EffectContext>, Vec<Effect>>;
pub(crate) type NativeEffectFactory =
    Arc<dyn Fn(&mut World) -> NativeEffectSystem + Send + Sync + 'static>;

#[derive(Default, Resource)]
pub(crate) struct NativeEffectRegistry(pub BTreeMap<NativeEffectId, NativeEffectSystem>);
