//! The definitions table, written by hand because the index lives only in Postgres.
//!
//! `value` is `JSONB`, so an index field is a top-level key of it and a query is a
//! `CASE`-guarded `jsonb_typeof`/`->>` expression over the storage rows themselves.

pub(super) const INSERT_INDEX: &str = "
INSERT INTO storage_index (name, collection, key, fields, sortable_fields, max_entries, index_only, user_id)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
RETURNING name, collection, key, fields, sortable_fields, max_entries, index_only, user_id";

pub(super) const GET_INDEX: &str = "
SELECT name, collection, key, fields, sortable_fields, max_entries, index_only, user_id
FROM storage_index WHERE name = $1";

pub(super) const LIST_INDEXES: &str = "
SELECT name, collection, key, fields, sortable_fields, max_entries, index_only, user_id
FROM storage_index ORDER BY name ASC";

pub(super) const DELETE_INDEX: &str = "
DELETE FROM storage_index WHERE name = $1";

/// The projection head; the value column is written by the page builder because
/// an `index_only` index answers with the indexed keys and nothing else.
pub(super) const INDEX_READ_HEAD: &str = "
SELECT s.collection, s.key, s.user_id, ";

/// After the value expression: the rest of the row and the collection the index names.
pub(super) const INDEX_READ_TAIL: &str = "
, s.version, s.read, s.write, s.create_time, s.update_time
FROM storage s WHERE s.collection = ";

/// The columns a page may sort on without the object value; a name not here is
/// tested against the index's sortable fields.
pub(super) fn sort_column(field: &str) -> Option<&'static str> {
    match field {
        "collection" => Some("collection"),
        "key" => Some("key"),
        "user_id" => Some("user_id"),
        "version" => Some("version"),
        "read" => Some("read"),
        "write" => Some("write"),
        "create_time" => Some("create_time"),
        "update_time" => Some("update_time"),
        _ => None,
    }
}
