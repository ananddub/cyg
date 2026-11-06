use crate::game::{GameConfig, RoomVariantDef};

/// `[rooms.duo]` tables — named room variants for filtered matchmaking.
pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
    let Some(rooms) = table.get("rooms").and_then(|v| v.as_table()) else {
        return;
    };
    for (name, def) in rooms {
        let Some(def) = def.as_table() else {
            continue;
        };
        let variant = RoomVariantDef {
            filter_by: def
                .get("filter_by")
                .and_then(|v| v.as_array())
                .map(|list| {
                    list.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
            max_clients: def.get("max_clients").and_then(|v| v.as_integer()).map(|v| v as i32),
            persistent: def.get("persistent").and_then(|v| v.as_bool()),
            options: def.get("options").and_then(|v| v.as_str()).map(str::to_string),
        };
        cfg.rooms.insert(name.clone(), variant);
    }
}
