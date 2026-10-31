use std::collections::BTreeMap;

use futures::StreamExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use somnia::Key;
use surrealdb::engine::any::{Any, connect as surreal_connect};
use surrealdb::engine::local::Db;
use surrealdb::method::QueryStream;
use surrealdb::opt::Config;
use surrealdb::opt::auth::Root;
use surrealdb::opt::capabilities::{Capabilities, ExperimentalFeature};
use surrealdb::types::{Action as LiveQueryAction, Value as SqlValue};
use surrealdb::{Connection, IndexedResults, Surreal};
use surrealdb_cnf::{ConfigMap, NOTIFICATIONS_CHANNEL_SIZE};
use surrealdb_core::kvs::Datastore as Kvs;

use super::error::DbError;

mod kv_budget;
mod live_change;
mod open;
mod raw_all;
mod touch;

pub use kv_budget::*;
pub use live_change::*;
