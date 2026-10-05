use std::{collections::BTreeMap, sync::Arc};

use bevy::{ecs::system::SystemId, prelude::*};

use crate::{Effect, EffectContext, NativeEffectId};

pub type NativeEffectSystem = SystemId<In<EffectContext>, Vec<Effect>>;
pub type NativeEffectFactory =
    Arc<dyn Fn(&mut World) -> NativeEffectSystem + Send + Sync + 'static>;

#[derive(Default, Resource)]
pub struct NativeEffectRegistry(pub BTreeMap<NativeEffectId, NativeEffectSystem>);
