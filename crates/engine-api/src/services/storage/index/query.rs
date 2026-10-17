//! The indexed read: a JSONB query over `storage`, narrowed to the object keys the
//! index declares, permission-filtered the way the index listing is, and paged by
//! an offset cursor that only replays for the same query, limit and order.

use axum::Json;
use sqlx::postgres::PgArguments;

use crate::ApiState;
use crate::app::middleware::Authed;
use crate::dto::storage::StorageObject;
use crate::dto::storage_index::{IndexPageCursor, IndexQueryRequest, StorageIndexList};
use crate::error::V2Error;
use crate::services::social;
use crate::services::storage::StorageService;
use crate::utils::storage::{CURSOR_MESSAGE, StorageRow};

use super::IndexRow;
use super::parse::{self, Term};
use super::sql::{Sort, build_query, resolve_order};

const DEFAULT_LIMIT: i32 = 100;
const MAX_LIMIT: i32 = 100;
const LIMIT_MESSAGE: &str = "Invalid limit - limit must be between 1 and 100.";
const FIELD_MESSAGE: &str = "This index does not cover the field named in the query.";

impl StorageService {
    /// Answer `POST /v2/storage/index/{name}/query`: the objects the index matched,
    /// exactly as the storage row holds them.
    pub async fn query_index(
        &self,
        state: &ApiState,
        auth: &Authed,
        name: &str,
        request: IndexQueryRequest,
    ) -> Result<Json<StorageIndexList>, V2Error> {
        let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(V2Error::invalid_argument(LIMIT_MESSAGE));
        }
        let terms = parse::parse(&request.query)?;
        let offset = resume_cursor(&request)?;

        let def = self.index_by_name(state, name).await?;
        let fields = IndexRow::strings(&def.fields);
        if terms.iter().any(|term| !fields.contains(term_field(term))) {
            return Err(V2Error::invalid_argument(FIELD_MESSAGE));
        }
        let sorts: Vec<(Sort, bool)> = resolve_order(&def, &request.order)?;

        let window = i64::from(def.max_entries).saturating_sub(offset);
        if window <= 0 {
            return Ok(Json(StorageIndexList {
                objects: Vec::new(),
                next_cursor: String::new(),
            }));
        }
        let page = i64::from(limit).min(window);

        let mut args = PgArguments::default();
        let sql = build_query(&def, &terms, &sorts, auth.uid(), page + 1, offset, &mut args);
        let rows = sqlx::query_as_with::<_, StorageRow, _>(&sql, args)
            .fetch_all(super::super::pool(state)?)
            .await
            .map_err(V2Error::from_sqlx)?;

        Ok(reply(rows, page, offset, &request, limit))
    }
}

fn term_field(term: &Term) -> &String {
    match term {
        Term::Eq { field, .. } | Term::Range { field, .. } => field,
    }
}

/// The cursor replays the search that made it: any drift in query, limit or order
/// means it belongs to a different page and is refused like a malformed one.
fn resume_cursor(request: &IndexQueryRequest) -> Result<i64, V2Error> {
    if request.cursor.is_empty() {
        return Ok(0);
    }
    let value = social::cursor_decode(&request.cursor, CURSOR_MESSAGE)?;
    let cursor: IndexPageCursor =
        serde_json::from_value(value).map_err(|_| V2Error::invalid_argument(CURSOR_MESSAGE))?;
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT);
    if cursor.query != request.query || cursor.limit != limit || cursor.order != request.order {
        return Err(V2Error::invalid_argument(CURSOR_MESSAGE));
    }
    if cursor.offset < 0 {
        return Err(V2Error::invalid_argument(CURSOR_MESSAGE));
    }
    Ok(cursor.offset)
}

/// The page was over-fetched by one row: that row proves there is more, and the
/// window it lands in becomes the next cursor.
fn reply(
    rows: Vec<StorageRow>,
    page: i64,
    offset: i64,
    request: &IndexQueryRequest,
    limit: i32,
) -> Json<StorageIndexList> {
    let mut objects: Vec<StorageObject> = rows.into_iter().map(StorageObject::from).collect();
    let mut next_cursor = String::new();
    if objects.len() as i64 > page {
        objects.truncate(page as usize);
        let cursor = IndexPageCursor {
            query: request.query.clone(),
            limit,
            order: request.order.clone(),
            offset: offset + page,
        };
        next_cursor =
            social::cursor_encode(&serde_json::to_value(cursor).expect("a cursor is plain JSON"));
    }
    Json(StorageIndexList {
        objects,
        next_cursor,
    })
}
