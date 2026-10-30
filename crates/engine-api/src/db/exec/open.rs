use super::*;

impl Datastore {
    pub async fn open(
        endpoint: &str,
        ns: &str,
        db: &str,
        user: Option<&str>,
        pass: Option<&str>,
        budget: KvBudget,
    ) -> Result<Self, DbError> {
        let capabilities = capabilities();
        if endpoint.starts_with("surrealkv://") {
            return Self::open_surrealkv(endpoint, ns, db, budget, capabilities).await;
        }
        let client = surreal_connect((endpoint, Config::new().capabilities(capabilities)))
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        if !is_embedded(endpoint) {
            if let Some(user) = user {
                client
                    .signin(Root {
                        username: user.to_string(),
                        password: pass.unwrap_or_default().to_string(),
                    })
                    .await
                    .map_err(|err| DbError::Connection(err.to_string()))?;
            }
        }
        client
            .use_ns(ns.to_string())
            .use_db(db.to_string())
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        Ok(Self {
            client: Client::Sdk(client),
        })
    }
    async fn open_surrealkv(
        endpoint: &str,
        ns: &str,
        db: &str,
        budget: KvBudget,
        capabilities: Capabilities,
    ) -> Result<Self, DbError> {
        let (notify, notifications) = surrealdb::channel::bounded(NOTIFICATIONS_CHANNEL_SIZE);
        let kvs = Kvs::builder()
            .with_capabilities(capabilities.into())
            .with_config(budget.config())
            .with_notify(notify)
            .without_maintenance_tasks()
            .build_with_path(endpoint)
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        report_budget(endpoint, &budget);
        kvs.check_version()
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        kvs.bootstrap()
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        kvs.start_maintenance_tasks();
        let client = Surreal::<Db>::unstable_from_datastore(kvs, Some(notifications))
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        client
            .use_ns(ns.to_string())
            .use_db(db.to_string())
            .await
            .map_err(|err| DbError::Connection(err.to_string()))?;
        Ok(Self {
            client: Client::Kv(client),
        })
    }
    pub async fn query<T>(&self, q: &(impl ToString + ?Sized)) -> Result<Vec<T>, DbError>
    where
        T: DeserializeOwned,
    {
        let rows = self.run(&q.to_string(), None).await?;
        decode_rows(rows.into_iter().next().unwrap_or(serde_json::Value::Null))
    }
    pub async fn query_params<T>(
        &self,
        surql: &str,
        params: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<T>, DbError>
    where
        T: DeserializeOwned,
    {
        let rows = self.run(surql, Some(params)).await?;
        decode_rows(rows.into_iter().next().unwrap_or(serde_json::Value::Null))
    }
    pub async fn query_one<T>(&self, q: &(impl ToString + ?Sized)) -> Result<Option<T>, DbError>
    where
        T: DeserializeOwned,
    {
        Ok(self.query(q).await?.into_iter().next())
    }
    /// Runs a multi-statement transaction and decodes the first statement that
    /// returned rows. `BEGIN`/`LET`/`COMMIT` answer empty, so a guarded `IF`
    /// block yields its own rows or nothing when the guard refused.
    pub async fn query_transaction<T>(
        &self,
        surql: &str,
        params: &BTreeMap<String, serde_json::Value>,
    ) -> Result<Vec<T>, DbError>
    where
        T: DeserializeOwned,
    {
        let statements = self.run(surql, Some(params)).await?;
        let rows = statements
            .into_iter()
            .find(|statement| matches!(statement, serde_json::Value::Array(rows) if !rows.is_empty()))
            .unwrap_or(serde_json::Value::Array(Vec::new()));
        decode_rows(rows)
    }
    pub async fn live(
        &self,
        surql: &str,
        params: &BTreeMap<String, serde_json::Value>,
    ) -> Result<LiveStream, DbError> {
        let mut response = match &self.client {
            Client::Sdk(client) => request(client, surql, Some(params)).await?,
            Client::Kv(client) => request(client, surql, Some(params)).await?,
        }
        .check()
        .map_err(DbError::from)?;
        let inner = response.stream::<SqlValue>(0).map_err(DbError::from)?;
        Ok(LiveStream { inner })
    }
    pub async fn exec(&self, surql: &str) -> Result<(), DbError> {
        self.run(surql, None).await.map(|_| ())
    }
    pub async fn exec_params(
        &self,
        surql: &str,
        params: &BTreeMap<String, serde_json::Value>,
    ) -> Result<(), DbError> {
        self.run(surql, Some(params)).await.map(|_| ())
    }
    pub async fn create<T>(&self, table: &str, key: Key, data: &T) -> Result<T, DbError>
    where
        T: Serialize + DeserializeOwned,
    {
        let (key_expr, key_value) = match &key {
            Key::Uuid(uuid) => ("<uuid>$key", serde_json::json!(uuid.to_string())),
            Key::String(text) => ("$key", serde_json::json!(text)),
            Key::Int(number) => ("$key", serde_json::json!(number)),
        };
        let params = BTreeMap::from([
            ("key".to_string(), key_value),
            ("data".to_string(), payload(data)?),
        ]);
        let surql = format!("CREATE type::record('{table}', {key_expr}) CONTENT $data RETURN AFTER");
        let rows = self.query_params(&surql, &params).await?;
        rows.into_iter()
            .next()
            .ok_or_else(|| DbError::Query(format!("create into {table} returned no record")))
    }
    pub async fn create_many<T>(&self, table: &str, records: &[(Key, T)]) -> Result<Vec<T>, DbError>
    where
        T: Serialize + DeserializeOwned,
    {
        let mut stored = Vec::with_capacity(records.len());
        for (key, data) in records {
            stored.push(self.create(table, key.clone(), data).await?);
        }
        Ok(stored)
    }
}
