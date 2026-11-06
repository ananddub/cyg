use crate::game::{GameConfig, PhysicsFeature};

pub(super) fn apply(cfg: &mut GameConfig, table: &toml::value::Table) {
    let Some(f) = table.get("features").and_then(|v| v.as_table()) else {
        return;
    };
    if let Some(v) = f.get("physics").and_then(|v| v.as_str()) {
        match PhysicsFeature::parse(v) {
            Some(mode) => cfg.features.physics = mode,
            None => {
                return_unknown("[features] physics", v);
            }
        }
    }
    if let Some(v) = f.get("voice").and_then(|v| v.as_bool()) {
        cfg.features.voice = v;
    }
}

fn return_unknown(key: &str, value: &str) {
    // Caller surfaces this via the returned config error path; a warning log
    // keeps the boot alive while making the typo loud.
    tracing::warn!(
        "[config] {key} = '{value}' is not one of both/2d/3d/none — keeping the default"
    );
}
