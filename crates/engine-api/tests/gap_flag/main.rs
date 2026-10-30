use engine_api::infra::console::{self, ConsoleRole};
use uuid::Uuid;

#[path = "../harness/mod.rs"]
mod harness_shared;

pub use harness_shared::*;

mod surface;

pub use surface::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn an_experiment_places_every_account_and_never_moves_it() {
    let env = Harness::start().await;
    assignment_surface(&env).await;
    env.stop().await;
    eprintln!("gap: flag assignment passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn stopping_an_experiment_returns_everybody_to_the_control() {
    let env = Harness::start().await;
    rollout_surface(&env).await;
    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn only_an_admin_can_roll_past_half_the_population() {
    let env = Harness::start().await;
    permission_surface(&env).await;
    env.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub async fn a_deleted_flag_takes_its_arms_and_pins_with_it() {
    let env = Harness::start().await;
    delete_surface(&env).await;
    env.stop().await;
}

/// A console token with a chosen role. Sign-in only ever hands out the
/// bootstrapped admin, and the gate this suite is checking is the one between
/// an admin and everybody else.
fn operator(role: ConsoleRole) -> String {
    console::mint(
        &Uuid::new_v4().to_string(),
        "e2e-operator",
        "operator@console.local",
        role,
    )
}
