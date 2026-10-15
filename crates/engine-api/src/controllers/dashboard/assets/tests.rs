use axum::Router;
use axum::routing::get;

use super::*;

#[test]
fn the_route_table_registers_without_conflicting() {
    // Axum panics on an ambiguous route at registration, so building this is the assertion.
    let _router = Router::<()>::new()
        .route("/dashboard", get(index))
        .route("/dashboard/", get(index))
        .route("/dashboard/assets/{*rest}", get(asset));
}
