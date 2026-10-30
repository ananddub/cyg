use super::*;

#[derive(Debug, Clone)]
pub struct LiveChange {
    pub action: LiveAction,
    pub row: serde_json::Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveAction {
    Create,
    Update,
    Delete,
}
impl LiveAction {
    pub fn as_str(self) -> &'static str {
        match self {
            LiveAction::Create => "create",
            LiveAction::Update => "update",
            LiveAction::Delete => "delete",
        }
    }
}
pub struct LiveStream {
    pub(super) inner: QueryStream<SqlValue>,
}
impl LiveStream {
    pub async fn next_change(&mut self) -> Option<Result<LiveChange, DbError>> {
        let notification = self.inner.next().await?;
        Some(match notification {
            Ok(notification) => {
                let action = notification.action;
                let row = notification.data.into_json_value();
                match action {
                    LiveQueryAction::Create => Ok(LiveChange {
                        action: LiveAction::Create,
                        row,
                    }),
                    LiveQueryAction::Update => Ok(LiveChange {
                        action: LiveAction::Update,
                        row,
                    }),
                    LiveQueryAction::Delete => Ok(LiveChange {
                        action: LiveAction::Delete,
                        row,
                    }),
                    LiveQueryAction::Killed => return None,
                    LiveQueryAction::Error => Err(DbError::Query(match row {
                        serde_json::Value::String(message) => message,
                        other => other.to_string(),
                    })),
                }
            }
            Err(err) => Err(DbError::from(err)),
        })
    }
}
pub(super) async fn request<C: Connection>(
    client: &Surreal<C>,
    surql: &str,
    params: Option<&BTreeMap<String, serde_json::Value>>,
) -> Result<IndexedResults, DbError> {
    let mut query = client.query(surql);
    if let Some(params) = params {
        for (name, value) in params {
            query = query.bind((name.as_str(), value.clone()));
        }
    }
    query.await.map_err(DbError::from)
}
pub(super) fn capabilities() -> Capabilities {
    Capabilities::new().with_experimental_features_allowed(&[ExperimentalFeature::Files])
}
pub fn is_embedded(endpoint: &str) -> bool {
    [
        "mem://",
        "surrealkv://",
        "rocksdb://",
        "indxdb://",
        "tikv://",
    ]
    .iter()
    .any(|scheme| endpoint.starts_with(scheme))
}
pub(super) fn report_budget(endpoint: &str, budget: &KvBudget) {
    let describe = |mib: Option<u64>| match mib {
        Some(mib) => format!("{mib} MiB"),
        None => "host default".to_string(),
    };
    tracing::info!(
        "[database] {endpoint}: memtable {}, block cache {}",
        describe(budget.memtable_mib),
        describe(budget.cache_mib),
    );
}
pub(super) fn decode_rows<T: DeserializeOwned>(
    value: serde_json::Value,
) -> Result<Vec<T>, DbError> {
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|mut item| {
                normalize_id(&mut item);
                serde_json::from_value(item).map_err(DbError::from)
            })
            .collect(),
        mut other => {
            normalize_id(&mut other);
            Ok(vec![serde_json::from_value(other).map_err(DbError::from)?])
        }
    }
}
fn normalize_id(value: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };
    let Some(serde_json::Value::String(id)) = map.get("id") else {
        return;
    };
    let Some((_, key)) = id.split_once(':') else {
        return;
    };
    map.insert("id".to_string(), serde_json::Value::String(strip_key(key)));
}
pub(crate) fn strip_key(key: &str) -> String {
    let key = key.trim();
    if let Some(rest) = key.strip_prefix('u').or_else(|| key.strip_prefix('U')) {
        let rest = rest.trim();
        let quoted = (rest.starts_with('\'') && rest.ends_with('\''))
            || (rest.starts_with('"') && rest.ends_with('"'));
        if quoted && rest.len() >= 2 {
            return rest[1..rest.len() - 1].to_string();
        }
    }
    if key.len() >= 2 && key.starts_with('`') && key.ends_with('`') {
        return key[1..key.len() - 1].to_string();
    }
    if let Some(inner) = key.strip_prefix('⟨').and_then(|k| k.strip_suffix('⟩')) {
        return inner.to_string();
    }
    key.to_string()
}
pub(super) fn payload<T: Serialize>(data: &T) -> Result<serde_json::Value, DbError> {
    let mut value = serde_json::to_value(data)?;
    if let serde_json::Value::Object(map) = &mut value {
        map.remove("id");
    }
    Ok(value)
}
