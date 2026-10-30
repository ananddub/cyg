use std::time::Duration;

use uuid::Uuid;
#[path = "../harness/mod.rs"]
mod harness_shared;
pub use harness_shared::*;

mod grant_idempotency;
mod notification_idempotency;
mod purchase_idempotency;
mod wallet_idempotency;

pub use grant_idempotency::*;
pub use notification_idempotency::*;
pub use purchase_idempotency::*;
pub use wallet_idempotency::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn idempotent_economy_end_to_end() {
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    wallet_idempotency(&env, &console).await;
    grant_idempotency(&env, &console).await;
    purchase_idempotency(&env).await;
    notification_idempotency(&env, &console).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the idempotent economy surface passed");
}

pub async fn rows_matching(env: &Harness, table: &str, needle: &str) -> Vec<serde_json::Value> {
    let rows: Vec<serde_json::Value> = env
        .db()
        .query(&format!("SELECT * FROM {table}"))
        .await
        .expect("rows from table");
    rows.into_iter()
        .filter(|row| row.to_string().contains(needle))
        .collect()
}

pub fn uid() -> Uuid {
    Uuid::new_v4()
}
