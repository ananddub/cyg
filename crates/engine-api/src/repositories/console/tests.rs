use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::repositories::console_mutations::{InsertAuditLogParams, InsertConsoleUserParams};
use crate::tests::games::{db, skipped_without_database};

#[tokio::test]
async fn console_users_and_accounts_list_from_their_tables() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    let id = Uuid::new_v4();
    q.insert_console_user(&InsertConsoleUserParams {
        id,
        username: "root".into(),
        email: "root@example.com".into(),
        acl: json!({}),
        password: b"secret".to_vec(),
        mfa_required: false,
    })
    .await
    .expect("the console user registers");

    assert_eq!(q.count_console_users().await.expect("users count"), 1);
    let found = q
        .find_console_user("root@example.com")
        .await
        .expect("the user resolves by email");
    assert_eq!(found.id, id);
    assert_eq!(
        q.list_accounts(10, 0).await.expect("accounts list").len(),
        0
    );
}

#[tokio::test]
async fn audit_usernames_rank_by_activity() {
    let Some((state, _db)) = db::state().await else {
        return skipped_without_database();
    };
    let q = state.q.as_ref().expect("the fixture opens queries");
    for (username, entries) in [("root", 2), ("dev", 1)] {
        for _ in 0..entries {
            q.insert_audit_log(&InsertAuditLogParams {
                id: Uuid::new_v4(),
                console_user_id: Uuid::nil(),
                console_username: username.into(),
                email: format!("{username}@example.com"),
                action: "test".into(),
                resource: "test".into(),
                message: "entry".into(),
                metadata: json!({}),
            })
            .await
            .expect("the audit entry appends");
        }
    }

    assert_eq!(
        q.audit_usernames().await.expect("usernames rank"),
        vec!["root".to_string(), "dev".to_string()]
    );
    assert_eq!(q.list_audit_logs(10, 0).await.expect("logs list").len(), 3);
}
