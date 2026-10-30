use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, somnia::SurrealRecord)]
#[table("idempotency_key", schemaless)]
#[index(
    name = "idempotency_key_actor_scope_request_uniq",
    fields = "actor, scope, request_key",
    unique
)]
#[index(name = "idempotency_key_create_time", fields = "create_time")]
pub struct IdempotencyKey {
    #[field(thing)]
    pub id: String,
    pub actor: String,
    pub scope: String,
    pub request_key: String,
    pub request_hash: String,
    pub status: String,
    pub response: Value,
    #[serde(with = "time::iso")]
    pub create_time: DateTime<Utc>,
    #[serde(with = "time::iso")]
    pub update_time: DateTime<Utc>,
}
