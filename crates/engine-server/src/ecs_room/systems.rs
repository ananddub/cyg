use bevy_ecs::prelude::*;
use engine_core::{EntityId, Velocity};

use crate::ecs_room::input_flags;
use crate::ecs_room::state::MOVE_SPEED;
use crate::ecs_room::state::SessionInputs;

pub(super) fn apply_inputs(mut query: Query<(&EntityId, &mut Velocity)>, inputs: Res<SessionInputs>) {
    for (eid, mut vel) in &mut query {
        let Some(&flags) = inputs.flags.get(&eid.0) else {
            continue;
        };
        vel.vx = if flags & input_flags::D != 0 {
            MOVE_SPEED
        } else if flags & input_flags::A != 0 {
            -MOVE_SPEED
        } else {
            0.0
        };
        vel.vz = if flags & input_flags::S != 0 {
            MOVE_SPEED
        } else if flags & input_flags::W != 0 {
            -MOVE_SPEED
        } else {
            0.0
        };
    }
}
