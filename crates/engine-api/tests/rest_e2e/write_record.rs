use super::*;

pub async fn write_record(client: &Client, base: &str, token: &str, id: &str, body: Value) -> Value {
    json_body(
        client
            .post(format!("{base}/v2/tournament/{id}"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .expect("write a tournament record"),
        StatusCode::OK,
        "write a tournament record",
    )
    .await
}
pub fn tournament_ids(page: &Value) -> Vec<String> {
    page["tournaments"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["id"].as_str().expect("tournament id").to_owned())
                .collect()
        })
        .unwrap_or_default()
}
