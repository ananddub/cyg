use auto_di::singleton;

use crate::ApiState;

pub struct ReportService;

#[singleton]
impl ReportService {
    fn new() -> Self {
        Self
    }
}

mod create;
mod operate;
