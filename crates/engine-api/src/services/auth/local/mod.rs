use std::collections::HashMap;

use axum::Json;
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::validate::Login;
use crate::ApiState;
use crate::db::{DbError, time};
use crate::dto::session::SessionResponse;
use crate::error::V2Error;
use crate::infra::jwt;
use crate::repositories::auth::{CreateUserWithCustomIdParams, CreateUserWithEmailParams};
use crate::repositories::models::Users;
use crate::services::deps::q;
use crate::utils::auth::{existing, insert_failed, lookup_failed, session};

mod create_device;
mod device;

use create_device::*;
pub use device::*;
