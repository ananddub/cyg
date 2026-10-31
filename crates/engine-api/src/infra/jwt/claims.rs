use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::V2Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tid: String,
    pub uid: String,
    #[serde(rename = "usn", default, skip_serializing_if = "String::is_empty")]
    pub un: String,
    #[serde(rename = "vrs", default, skip_serializing_if = "HashMap::is_empty")]
    pub vars: HashMap<String, String>,
    pub exp: i64,
    #[serde(default, skip_serializing_if = "num_zero")]
    pub iat: i64,
}

fn num_zero(v: &i64) -> bool {
    *v == 0
}

impl Claims {
    pub fn uuid(&self) -> Result<Uuid, V2Error> {
        Uuid::parse_str(&self.uid)
            .map_err(|_| V2Error::unauthenticated("session token carries an invalid user id"))
    }
}
