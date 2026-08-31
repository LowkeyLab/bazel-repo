use std::{collections::BTreeMap, sync::Arc};

use bevy::{ecs::system::SystemId, prelude::*};

use crate::{Effect, EffectContext, NativeEffectId};

pub(crate) type NativeEffectSystem = SystemId<In<EffectContext>, Vec<Effect>>;
pub(crate) type NativeEffectFactory =
    Arc<dyn Fn(&mut World) -> NativeEffectSystem + Send + Sync + 'static>;

#[derive(Default, Resource)]
pub(crate) struct NativeEffectRegistry(pub BTreeMap<NativeEffectId, NativeEffectSystem>);
