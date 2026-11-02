mod env;
mod env_social;
mod flat;
#[cfg(test)]
mod tests;
mod toml;

use std::path::Path;

use crate::game::GameConfig;

fn clamp_windows(cfg: &mut GameConfig) {
    cfg.lifecycle.clamp();
    cfg.watchdog.clamp();
    cfg.gameplay.normalize();
}

pub fn parse_config(text: &str) -> GameConfig {
    let mut cfg = GameConfig::default();

    if let Ok(value) = text.parse::<::toml::Value>()
        && let Some(table) = value.as_table()
    {
        toml::fill(&mut cfg, table);
        clamp_windows(&mut cfg);
        return cfg;
    }

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let (key, value) = if let Some((k, v)) = line.split_once('=') {
            (k, v)
        } else if let Some((k, v)) = line.split_once(':') {
            (k, v)
        } else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            continue;
        }
        flat::apply(&mut cfg, key, value);
    }
    clamp_windows(&mut cfg);
    cfg
}

pub fn load_config(path: &Path) -> std::io::Result<GameConfig> {
    let file = if path.is_dir() {
        if path.join("config.toml").exists() {
            path.join("config.toml")
        } else {
            path.join("config.yml")
        }
    } else {
        path.to_path_buf()
    };
    let text = std::fs::read_to_string(&file)?;
    let mut cfg = parse_config(&text);
    env::apply(&mut cfg);
    clamp_windows(&mut cfg);
    Ok(cfg)
}
