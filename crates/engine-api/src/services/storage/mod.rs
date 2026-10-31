use auto_di::singleton;

use crate::ApiState;
use crate::error::V2Error;

pub mod delete;
pub mod index;
pub mod list;
pub mod read;
pub mod write;

pub struct StorageService;

#[singleton]
impl StorageService {
    fn new() -> Self {
        Self
    }
}

pub(crate) async fn queries(
    state: &ApiState,
) -> Result<std::sync::Arc<crate::repositories::db::Queries>, V2Error> {
    crate::services::deps::q(state).await
}
