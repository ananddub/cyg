//! `LinkFacebookRequest.sync` — the one link flag that arrives in the query string.

use serde::Deserialize;

use crate::app::params::BoolFlag;
use crate::error::V2Error;

/// `LinkFacebookRequest.sync`, read off the query string because the gateway maps the
/// body onto the `account` field alone; an unparsable value is a 400, as there.
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
