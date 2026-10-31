use super::*;

#[test]
pub(crate) fn the_read_and_write_gates_on_group_export_audit_and_record_delete() {
    let server = V2Server::start(v2_state());
    let readonly = token_for(ConsoleRole::Readonly);
    let maintainer = token_for(ConsoleRole::Maintainer);

    let reply = server.get("/v2/console/audit/log/users", Some(&readonly));
    assert_eq!(reply.status, 503, "{}", reply.body);

    let reply = server.get(
        &format!("/v2/console/group/{NOBODY}/export"),
        Some(&readonly),
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.get("/v2/console/group/not-a-uuid/export", Some(&maintainer));
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid account id.");
    let reply = server.get(
        &format!("/v2/console/group/{NOBODY}/export"),
        Some(&maintainer),
    );
    assert_eq!(reply.status, 503, "{}", reply.body);

    let reply = server.delete(
        &format!("/v2/console/leaderboard/board-1/owner/{NOBODY}"),
        Some(&readonly),
    );
    assert_eq!(
        reply.refusal(PERMISSION_DENIED),
        "This console user has read-only access."
    );
    let reply = server.delete(
        "/v2/console/leaderboard/board-1/owner/not-a-uuid",
        Some(&maintainer),
    );
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid owner ID.");
    let reply = server.delete(
        &format!("/v2/console/leaderboard/board-1/owner/{NOBODY}"),
        Some(&maintainer),
    );
    assert_eq!(reply.status, 503, "{}", reply.body);
}
#[test]
pub(crate) fn per_id_import_addresses_the_record_by_its_path_id() {
    let server = V2Server::start(v2_state());
    let admin = token_for(ConsoleRole::Admin);
    let reply = server.post(
        "/v2/console/account/not-a-uuid/import",
        Some(&admin),
        "null",
    );
    assert_eq!(reply.refusal(INVALID_ARGUMENT), "Invalid account id.");

    let id = Uuid::new_v4();
    let reply = server.post(
        &format!("/v2/console/account/{id}/import"),
        Some(&admin),
        "null",
    );
    assert_eq!(
        reply.refusal(INVALID_ARGUMENT),
        "The account id in the body must match the id in the path."
    );
}
