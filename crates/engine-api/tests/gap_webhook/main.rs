use std::time::Duration;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod delivery;
mod receiver;
mod surface;

pub use delivery::*;
pub use receiver::*;
pub use surface::*;

/// The key the operator writes down once and puts in the receiver's config.
pub const SECRET: &str = "e2e-webhook-secret-0123456789";

static TUNING: std::sync::Once = std::sync::Once::new();

/// Live-ops knobs resolve from the environment the first time anyone reads them,
/// which for a test binary is this line. The defaults are production patience:
/// five seconds to answer, half a second between beats, six attempts. A test
/// wants the same machine running on a wristwatch.
fn tuning() {
    TUNING.call_once(|| unsafe {
        std::env::set_var("OX_WEBHOOK_POLL_MS", "50");
        std::env::set_var("OX_WEBHOOK_BACKOFF_MS", "100");
        std::env::set_var("OX_WEBHOOK_TIMEOUT_MS", "1500");
        std::env::set_var("OX_WEBHOOK_CONCURRENCY", "1");
        std::env::set_var("OX_WEBHOOK_MAX_ATTEMPTS", "4");
        std::env::set_var("OX_WEBHOOK_CIRCUIT_TRIPS", "2");
        std::env::set_var("OX_WEBHOOK_CIRCUIT_COOLDOWN_MS", "5000");
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn signed_webhooks_end_to_end() {
    tuning();
    let env = Harness::start().await;
    let console = console_login(&env.client, &env.base).await;

    let taker = Receiver::answering(200, SECRET).await;
    let down = Receiver::answering(500, DOWN_SECRET).await;

    endpoint_surface(&env, &console, &taker.url).await;
    delivery_surface(&env, &console, &taker, &down).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the signed webhook surface passed");
}
