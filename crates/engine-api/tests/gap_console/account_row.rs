use super::*;

pub const NEVER: &str = "1970-01-01T00:00:00Z";
pub async fn account_row(env: &Harness, token: &str, id: &str) -> Value {
    json_body(
        env.client
            .get(format!("{}/v2/console/account/{id}", env.base))
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
        "GET /v2/console/account/{id}",
    )
    .await
}
