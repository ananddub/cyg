use super::*;

impl Datastore {
    pub async fn raw_all(&self, surql: &str) -> Result<Vec<serde_json::Value>, DbError> {
        self.run(surql, None).await
    }
    pub async fn apply<I, S>(&self, statements: I) -> Result<(), DbError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for statement in statements {
            self.exec(statement.as_ref()).await?;
        }
        Ok(())
    }
    pub(super) async fn run(
        &self,
        surql: &str,
        params: Option<&BTreeMap<String, serde_json::Value>>,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        self.respond(surql, params, true).await
    }
    /// Runs a statement without waking live readers. For probes that rewrite
    /// rows only when something stalled, where the empty case is the normal
    /// one and the real change is announced by the writes that follow it.
    pub(super) async fn run_quiet(
        &self,
        surql: &str,
        params: Option<&BTreeMap<String, serde_json::Value>>,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        self.respond(surql, params, false).await
    }
    async fn respond(
        &self,
        surql: &str,
        params: Option<&BTreeMap<String, serde_json::Value>>,
        announce: bool,
    ) -> Result<Vec<serde_json::Value>, DbError> {
        let response = match &self.client {
            Client::Sdk(client) => request(client, surql, params).await?,
            Client::Kv(client) => request(client, surql, params).await?,
        };
        let mut response = response.check().map_err(DbError::from)?;
        if announce {
            for table in touch::touched_tables(surql) {
                let _ = self.touches.send(table);
            }
        }
        let count = response.num_statements();
        let mut out = Vec::with_capacity(count);
        for index in 0..count {
            let rows: Vec<serde_json::Value> = response.take(index).map_err(DbError::from)?;
            out.push(serde_json::Value::Array(rows));
        }
        Ok(out)
    }
}
