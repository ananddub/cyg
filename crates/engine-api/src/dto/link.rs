use serde::Deserialize;

use crate::app::params::BoolFlag;
use crate::error::V2Error;

#[derive(Debug, Default, Deserialize)]
pub struct SyncQuery {
    #[serde(default)]
    pub sync: Option<String>,
}

impl SyncQuery {
    pub(crate) fn check(&self) -> Result<(), V2Error> {
        match &self.sync {
            Some(raw) => BoolFlag::parse(raw).map(|_| ()),
            None => Ok(()),
        }
    }
}
