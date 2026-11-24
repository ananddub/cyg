use crate::services::plane::Plane;
use auto_di::singleton;
use std::sync::Arc;

pub struct ReportService {
    pub(crate) plane: Arc<Plane>,
}

#[singleton]
impl ReportService {
    pub fn new(plane: Arc<Plane>) -> Self {
        Self { plane }
    }
}

mod create;
mod operate;
