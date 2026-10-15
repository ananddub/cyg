//! The statement a page runs, chosen from the listing shape and whether a
//! cursor resumes it.

use uuid::Uuid;

use crate::utils::storage::StorageRow;
use crate::error::V2Error;

use super::statements::{
    OWN_FIRST, OWN_NEXT, PUBLIC_ALL_FIRST, PUBLIC_ALL_NEXT, PUBLIC_USER_FIRST, PUBLIC_USER_NEXT,
};
use super::cursor::Resumed;

#[derive(Clone, Copy)]
pub(super) enum Listing {
    PublicAll,
    Own,
    PublicUser,
}

pub(super) async fn fetch(
    db: &sqlx::PgPool,
    listing: Listing,
    collection: &str,
    owner: Option<Uuid>,
    after: Option<&Resumed>,
    limit: i64,
) -> Result<Vec<StorageRow>, V2Error> {
    let page = limit + 1;
    let rows = match (listing, after) {
        (Listing::PublicAll, None) => {
            sqlx::query_as::<_, StorageRow>(PUBLIC_ALL_FIRST)
                .bind(collection)
                .bind(page)
                .fetch_all(db)
                .await
        }
        (Listing::PublicAll, Some(c)) => {
            sqlx::query_as::<_, StorageRow>(PUBLIC_ALL_NEXT)
                .bind(collection)
                .bind(&c.key)
                .bind(c.user_id)
                .bind(page)
                .fetch_all(db)
                .await
        }
        (Listing::Own, None) => {
            sqlx::query_as::<_, StorageRow>(OWN_FIRST)
                .bind(collection)
                .bind(owner)
                .bind(page)
                .fetch_all(db)
                .await
        }
        (Listing::Own, Some(c)) => {
            sqlx::query_as::<_, StorageRow>(OWN_NEXT)
                .bind(collection)
                .bind(owner)
                .bind(c.read)
                .bind(&c.key)
                .bind(page)
                .fetch_all(db)
                .await
        }
        (Listing::PublicUser, None) => {
            sqlx::query_as::<_, StorageRow>(PUBLIC_USER_FIRST)
                .bind(collection)
                .bind(owner)
                .bind(page)
                .fetch_all(db)
                .await
        }
        (Listing::PublicUser, Some(c)) => {
            sqlx::query_as::<_, StorageRow>(PUBLIC_USER_NEXT)
                .bind(collection)
                .bind(owner)
                .bind(&c.key)
                .bind(page)
                .fetch_all(db)
                .await
        }
    };
    rows.map_err(V2Error::from_sqlx)
}
