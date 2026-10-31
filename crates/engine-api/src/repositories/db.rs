use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::db::{Datastore, DbError, time};

#[derive(Clone)]
pub struct Queries {
    pub(crate) db: Arc<Datastore>,
}

impl Queries {
    pub fn new(db: Arc<Datastore>) -> Self {
        Self { db }
    }

    pub fn datastore(&self) -> &Datastore {
        &self.db
    }

    pub(crate) async fn all<T: DeserializeOwned>(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<Vec<T>, DbError> {
        self.db.query_params(surql, &params).await
    }

    pub(crate) async fn one<T: DeserializeOwned>(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<T, DbError> {
        self.all(surql, params)
            .await?
            .into_iter()
            .next()
            .ok_or(DbError::NotFound)
    }

    pub(crate) async fn opt<T: DeserializeOwned>(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<Option<T>, DbError> {
        Ok(self.all(surql, params).await?.into_iter().next())
    }

    pub(crate) async fn exec(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<(), DbError> {
        self.db.exec_params(surql, &params).await
    }

    pub(crate) async fn scalar<T: DeserializeOwned>(
        &self,
        surql: &str,
        params: BTreeMap<String, Value>,
    ) -> Result<T, DbError> {
        self.all(surql, params)
            .await?
            .into_iter()
            .next()
            .ok_or(DbError::NotFound)
    }

    pub(crate) async fn create<T: serde::Serialize + DeserializeOwned>(
        &self,
        table: &str,
        key: crate::db::Key,
        data: &T,
    ) -> Result<T, DbError> {
        self.db.create(table, key, data).await
    }
}

macro_rules! params {
    ($($name:ident = $value:expr),* $(,)?) => {
        ::std::collections::BTreeMap::<String, ::serde_json::Value>::from([
            $( (::std::stringify!($name).to_string(), ::serde_json::json!($value)) ),*
        ])
    };
}
pub(crate) use params;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteResult {
    rows: u64,
}

impl WriteResult {
    pub fn rows_affected(&self) -> u64 {
        self.rows
    }
}

pub(crate) async fn affected(
    queries: &Queries,
    surql: &str,
    binds: BTreeMap<String, Value>,
) -> Result<WriteResult, DbError> {
    let rows: Vec<Value> = queries.all(surql, binds).await?;
    Ok(WriteResult {
        rows: rows.len() as u64,
    })
}

pub(crate) fn ts(at: DateTime<Utc>) -> Value {
    Value::String(time::encode(at))
}
