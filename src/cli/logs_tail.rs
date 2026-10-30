use super::{block_on, flag_value, has_flag, positional, usage};

/// `cyg-game logs tail [--url http://127.0.0.1:7777] [--token <console>] [--limit 100]`
/// — print the newest mirrored log lines from a running control plane. The
/// dashboard's live view streams the same feed over the WebSocket.
pub fn run(args: &[String]) {
    if has_flag(args, "--help") || has_flag(args, "-h") {
        usage("logs tail [--url http://127.0.0.1:7777] [--token <console token>] [--limit 100]");
    }
    let extra = positional(args);
    if !extra.is_empty() {
        usage("logs tail [--url http://127.0.0.1:7777] [--token <console token>] [--limit 100]");
    }
    let url = flag_value(args, "--url")
        .unwrap_or_else(|| "http://127.0.0.1:7777".to_owned())
        .trim_end_matches('/')
        .to_owned();
    let token = flag_value(args, "--token").unwrap_or_default();
    let limit: usize = flag_value(args, "--limit")
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(100);

    let lines = match block_on(fetch(&url, &token, limit)) {
        Ok(lines) => lines,
        Err(e) => {
            tracing::error!("✗ logs tail: {e}");
            std::process::exit(1);
        }
    };
    if lines.is_empty() {
        tracing::info!("no log lines yet — is the log feed layer installed?");
        return;
    }
    for line in &lines {
        println!("{}", render(line));
    }
}

async fn fetch(url: &str, token: &str, limit: usize) -> Result<Vec<serde_json::Value>, String> {
    let mut request = reqwest::Client::new()
        .get(format!("{url}/v2/console/logs/recent?limit={limit}"))
        .header("accept", "application/json");
    if !token.is_empty() {
        request = request.bearer_auth(token);
    }
    let response = request
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("{url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "{url} answered {} — pass --token with a console token",
            response.status()
        ));
    }
    response
        .json::<Vec<serde_json::Value>>()
        .await
        .map_err(|e| format!("the reply is not a log array: {e}"))
}

fn render(line: &serde_json::Value) -> String {
    let at = line
        .get("atMs")
        .and_then(serde_json::Value::as_i64)
        .map(stamp)
        .unwrap_or_else(|| "--:--:--".to_owned());
    let level = line
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("INFO");
    let target = line
        .get("target")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    let message = line
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    format!("{at} {level:<5} {target}: {message}")
}

fn stamp(at_ms: i64) -> String {
    let secs = at_ms.div_euclid(1000);
    let millis = at_ms.rem_euclid(1000);
    let time =
        chrono::DateTime::from_timestamp(secs, (millis as u32) * 1_000_000).unwrap_or_default();
    time.format("%H:%M:%S%.3f").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_line_renders_with_time_level_target_message() {
        let rendered = render(&json!({
            "atMs": 1793376000123i64,
            "level": "WARN",
            "target": "cyg::room",
            "message": "room full"
        }));
        assert!(rendered.contains("WARN"), "{rendered}");
        assert!(rendered.contains("cyg::room"), "{rendered}");
        assert!(rendered.ends_with("room full"), "{rendered}");
    }

    #[test]
    fn missing_fields_render_without_panicking() {
        let rendered = render(&json!({}));
        assert!(rendered.contains("--:--:--"), "{rendered}");
    }
}
