use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Object)]
pub struct Empty {}
