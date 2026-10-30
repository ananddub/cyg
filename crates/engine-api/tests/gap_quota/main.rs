use std::time::Duration;

use engine_config::ControlConfig;
use engine_config::quota::{CHANNEL_SEND, LOGIN, QuotaScope, STORAGE_WRITE};
use uuid::Uuid;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod bucket;
mod durable;

pub use bucket::*;
pub use durable::*;

/// A plane with the quotas on. `windows` are the scopes this suite is checking;
/// everything else keeps the shipped default. The per-account request bucket is
/// widened, so what a durable test refuses on is a window and not a bucket.
pub fn quota_plane(windows: Vec<(&'static str, QuotaScope)>) -> impl Fn(u16) -> ControlConfig {
    move |port| {
        let mut control = control_config(port);
        control.quotas.enabled = true;
        control.quotas.player_rps = 1000;
        control.quotas.player_burst = 1000;
        for (scope, window) in &windows {
            control.quotas.scopes.insert(scope.to_string(), *window);
        }
        control
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn a_spent_window_refuses_with_a_honest_wait() {
    let env = Harness::start_with(quota_plane(vec![(
        CHANNEL_SEND,
        QuotaScope {
            cap: 3,
            period_secs: 300,
        },
    )]))
    .await;
    durable_surface(&env).await;
    env.stop().await;
    eprintln!("gap: quota windows passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn a_window_opens_again_when_the_period_passes() {
    let env = Harness::start_with(quota_plane(vec![(
        CHANNEL_SEND,
        QuotaScope {
            cap: 1,
            period_secs: 1,
        },
    )]))
    .await;
    rollover_surface(&env).await;
    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn a_storage_batch_spends_its_size() {
    let env = Harness::start_with(quota_plane(vec![(
        STORAGE_WRITE,
        QuotaScope {
            cap: 2,
            period_secs: 300,
        },
    )]))
    .await;
    batch_surface(&env).await;
    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn one_account_cannot_ask_faster_than_its_bucket() {
    let env = Harness::start_with(bucket_plane(true)).await;
    bucket_surface(&env).await;
    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn an_operator_is_bucketed_when_the_config_says_so() {
    let env = Harness::start_with(bucket_plane(false)).await;
    operator_bucket_surface(&env).await;
    env.stop().await;
}
