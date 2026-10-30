use engine_config::ControlConfig;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod relay;
mod surface;

pub use relay::*;
pub use surface::*;

static TUNING: std::sync::Once = std::sync::Once::new();

/// Live-ops knobs resolve from the environment the first time anyone reads them,
/// which for a test binary is this line. The defaults are production patience:
/// ten seconds to answer, a second between beats, five tries. A test wants the
/// same machine running on a wristwatch, and one that never talks to Apple.
fn tuning(relay_port: u16) {
    TUNING.call_once(|| unsafe {
        std::env::set_var("OX_NOTIFY_POLL_MS", "50");
        std::env::set_var("OX_NOTIFY_BACKOFF_MS", "100");
        std::env::set_var("OX_NOTIFY_TIMEOUT_MS", "1500");
        std::env::set_var("OX_NOTIFY_CONCURRENCY", "2");
        std::env::set_var("OX_NOTIFY_MAX_ATTEMPTS", "5");
        std::env::set_var("OX_NOTIFY_TITLE", "e2e notifications");
        std::env::set_var("OX_EMAIL_HOST", "127.0.0.1");
        std::env::set_var("OX_EMAIL_PORT", relay_port.to_string());
        std::env::set_var("OX_EMAIL_FROM", "ops@notify-e2e.test");
        std::env::set_var("OX_EMAIL_DOMAIN", "notify-e2e.test");
        // Plain loopback, because there is no certificate on this machine's own
        // address and nothing secret is being carried over it.
        std::env::set_var("OX_EMAIL_TLS", "none");
    });
}

/// The budgets are on, because the deferred message is the point of one case: a
/// spent window has to keep the letter owed rather than send it anyway.
fn notify_plane(port: u16) -> ControlConfig {
    let mut control = control_config(port);
    control.quotas.enabled = true;
    control.quotas.player_rps = 1000;
    control.quotas.player_burst = 1000;
    control
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn notification_delivery_end_to_end() {
    let relay = Relay::listening().await;
    tuning(relay.port);
    let env = Harness::start_with(notify_plane).await;
    unsafe {
        std::env::set_var("OX_NOTIFY_UNSUBSCRIBE_BASE", env.base.clone());
    }
    let console = console_login(&env.client, &env.base).await;

    delivery_surface(&env, &relay, &console).await;

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    env.stop().await;
    eprintln!("gap: the notification delivery surface passed");
}
