use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::{flag_value, positional, usage};

/// `cyg-game config diff <a.json> <b.json|live>` — a key-path diff of two
/// config documents. `--game <id>` replaces `b` with the live `games.config`
/// of that game, so an operator can see exactly what a proposed file would
/// change before posting it.
pub fn run(args: &[String], dir: &Path) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage("config diff <a.json> <b.json|live> [--game <id>]");
    }
    let files = positional(args);
    let game = flag_value(args, "--game");
    if files.len() != 2 {
        usage("config diff <a.json> <b.json|live> [--game <id>]");
    }
    let left = read_doc(files[0]);
    let right = match game {
        Some(id) => live_config(dir, &id),
        None => read_doc(files[1]),
    };
    let diffs = diff("", &left, &right);
    if diffs.is_empty() {
        tracing::info!("✓ no differences");
        return;
    }
    for line in &diffs {
        println!("{line}");
    }
    tracing::error!("✗ {} path(s) differ", diffs.len());
    std::process::exit(1);
}

fn read_doc(path: &str) -> Value {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!("✗ config diff: cannot read {path}: {e}");
            std::process::exit(1);
        }
    };
    match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("✗ config diff: {path} is not valid JSON: {e}");
            std::process::exit(1);
        }
    }
}

fn live_config(dir: &Path, game: &str) -> Value {
    let session = super::load_session(dir);
    match super::block_on(async move {
        let db = super::open_datastore(&session).await?;
        let rows: Vec<Value> = db
            .query_params(
                "SELECT config FROM games WHERE id = $game OR game_key = $game LIMIT 1",
                &std::collections::BTreeMap::from([(
                    "game".to_owned(),
                    Value::String(game.into()),
                )]),
            )
            .await
            .map_err(|e| e.to_string())?;
        rows.into_iter()
            .next()
            .and_then(|row| row.get("config").cloned())
            .ok_or_else(|| format!("game '{game}' has no row"))
    }) {
        Ok(value) => value,
        Err(e) => {
            tracing::error!("✗ config diff: {e}");
            std::process::exit(1);
        }
    }
}

/// Flat key-path comparison; arrays compare as whole values, which keeps the
/// output stable for config documents.
pub fn diff(prefix: &str, left: &Value, right: &Value) -> Vec<String> {
    let mut out = Vec::new();
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                match (a.get(key), b.get(key)) {
                    (Some(l), Some(r)) => out.extend(diff(&path, l, r)),
                    (Some(l), None) => out.push(format!("- {path} = {}", compact(l))),
                    (None, Some(r)) => out.push(format!("+ {path} = {}", compact(r))),
                    (None, None) => {}
                }
            }
        }
        _ if left == right => {}
        _ => out.push(format!(
            "~ {prefix} = {} -> {}",
            compact(left),
            compact(right)
        )),
    }
    out
}

fn compact(value: &Value) -> String {
    // Strings keep their quotes, so `1` and `"1"` can never render alike.
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn differing_paths_are_reported_with_directions() {
        let a = json!({"tick_rate": 30, "name": "old", "gone": 1});
        let b = json!({"tick_rate": 60, "name": "old", "added": true});
        let lines = diff("", &a, &b);
        assert!(
            lines.contains(&"~ tick_rate = 30 -> 60".to_owned()),
            "{lines:?}"
        );
        assert!(lines.contains(&"- gone = 1".to_owned()), "{lines:?}");
        assert!(lines.contains(&"+ added = true".to_owned()), "{lines:?}");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn identical_documents_produce_no_diff() {
        let a = json!({"a": {"b": [1, 2]}, "c": "x"});
        assert!(diff("", &a, &a).is_empty());
    }

    #[test]
    fn nested_objects_flatten_into_paths() {
        let a = json!({"net": {"udp": {"port": 7777}}});
        let b = json!({"net": {"udp": {"port": 8000}}});
        assert_eq!(
            diff("", &a, &b),
            vec!["~ net.udp.port = 7777 -> 8000".to_owned()]
        );
    }

    #[test]
    fn type_changes_are_a_value_change() {
        let a = json!({"n": 1});
        let b = json!({"n": "1"});
        assert_eq!(diff("", &a, &b), vec!["~ n = 1 -> \"1\"".to_owned()]);
    }

    #[test]
    fn maps_stay_whole() {
        let map = BTreeMap::<String, i32>::new();
        let _ = map;
        let a = json!({"list": [1, 2]});
        let b = json!({"list": [2, 1]});
        assert_eq!(diff("", &a, &b), vec!["~ list = [1,2] -> [2,1]".to_owned()]);
    }
}
