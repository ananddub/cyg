use super::*;

/// Fields the update mask may name. The flattened spellings are accepted
/// because the dashboard already uses them; both map to the canonical config
/// key so a mask and the body cannot disagree about what changed.
const UPDATE_MASK_FIELDS: &[&str] = &[
    "name",
    "engine_type",
    "status",
    "config",
    "tick_rate",
    "max_clients",
    "persistent_rooms",
    "auto_close_seconds",
    "config.tick_rate",
    "config.max_clients",
    "config.persistent_rooms",
    "config.auto_close_seconds",
];

#[derive(Debug, Clone)]
pub(super) struct GameUpdatePlan {
    pub(super) name: String,
    pub(super) engine_type: String,
    pub(super) backend_mode: String,
    pub(super) status: String,
    pub(super) config: serde_json::Value,
}

pub(super) fn preview(current: &GameRow, plan: &GameUpdatePlan) -> GameDto {
    GameDto {
        id: current.id,
        game_key: current.game_key.clone(),
        name: plan.name.clone(),
        client_key: current.client_key.clone(),
        engine_type: plan.engine_type.clone(),
        backend_mode: plan.backend_mode.clone(),
        status: plan.status.clone(),
        config: plan.config.clone(),
        created_at: current.created_at,
        updated_at: current.updated_at,
    }
}

/// Apply the explicit mask to the body. A field named by the mask must carry a
/// value; a field the body carries but the mask does not name is ignored. An
/// absent mask keeps the book's implicit mask (the fields present in the body).
pub(super) fn masked_update(
    body: &UpdateGameRequest,
    mask: Option<&[String]>,
) -> Result<UpdateGameRequest, V2Error> {
    let Some(mask) = mask else {
        return Ok(body.clone());
    };
    if mask.is_empty() {
        return Err(V2Error::invalid_argument(
            "An updateMask must name at least one field.",
        ));
    }
    let mut out = UpdateGameRequest::default();
    for raw in mask {
        let field = raw.trim();
        if !UPDATE_MASK_FIELDS.contains(&field) {
            return Err(V2Error::invalid_argument(format!(
                "Unknown updateMask field '{field}'."
            )));
        }
        match field {
            "name" => out.name = Some(required(body.name.clone(), field)?),
            "engine_type" => out.engine_type = Some(required(body.engine_type.clone(), field)?),
            "status" => out.status = Some(required(body.status.clone(), field)?),
            "config" => out.config = Some(required(body.config.clone(), field)?),
            "tick_rate" | "config.tick_rate" => {
                let value = body.tick_rate.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("tick_rate"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.tick_rate = Some(required(value, field)?);
            }
            "max_clients" | "config.max_clients" => {
                let value = body.max_clients.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("max_clients"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.max_clients = Some(required(value, field)?);
            }
            "persistent_rooms" | "config.persistent_rooms" => {
                let value = body.persistent_rooms.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("persistent_rooms"))
                        .and_then(|value| value.as_bool())
                });
                out.persistent_rooms = Some(required(value, field)?);
            }
            "auto_close_seconds" | "config.auto_close_seconds" => {
                let value = body.auto_close_seconds.or_else(|| {
                    body.config
                        .as_ref()
                        .and_then(|config| config.get("auto_close_seconds"))
                        .and_then(|value| value.as_u64())
                        .map(|value| value as u32)
                });
                out.auto_close_seconds = Some(required(value, field)?);
            }
            _ => unreachable!("the mask was checked against UPDATE_MASK_FIELDS"),
        }
    }
    Ok(out)
}

pub(super) fn required<T>(value: Option<T>, field: &str) -> Result<T, V2Error> {
    value.ok_or_else(|| {
        V2Error::invalid_argument(format!(
            "The updateMask names '{field}', but the request does not set it."
        ))
    })
}
