use std::sync::Arc;

use crate::ApiState;
use crate::db::BlobStore;
use crate::error::V2Error;
use crate::repositories::db::Queries;

pub(crate) async fn q(state: &ApiState) -> Result<Arc<Queries>, V2Error> {
    match crate::app::state::resolve::<Queries>().await {
        Ok(queries) => Ok(queries),
        Err(_) => state.q.clone().ok_or_else(V2Error::no_db),
    }
}

pub(crate) async fn blobs(state: &ApiState) -> Result<Arc<BlobStore>, V2Error> {
    match crate::app::state::resolve::<BlobStore>().await {
        Ok(blobs) => Ok(blobs),
        Err(_) => state.blobs.clone().ok_or_else(V2Error::no_blobs),
    }
}
