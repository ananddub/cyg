use super::{claim_endpoint_invalidation, endpoint_invalidation};
use std::collections::HashSet;

#[test]
fn coalesces_query_variants_into_one_endpoint_invalidation() {
    let mut claimed = HashSet::new();

    assert!(claim_endpoint_invalidation(
        &mut claimed,
        "/_cyg/live",
        "DeploymentController::list",
    ));
    assert!(!claim_endpoint_invalidation(
        &mut claimed,
        "/_cyg/live",
        "DeploymentController::list",
    ));
    assert!(claim_endpoint_invalidation(
        &mut claimed,
        "/_cyg/live",
        "ComposeController::get",
    ));
    assert!(claim_endpoint_invalidation(
        &mut claimed,
        "/another-namespace",
        "DeploymentController::list",
    ));
}

#[test]
fn endpoint_invalidation_explicitly_targets_every_args_variant() {
    assert_eq!(
        endpoint_invalidation("DeploymentController::list"),
        serde_json::json!({
            "endpoint": "DeploymentController::list",
            "args": null,
        }),
    );
}
