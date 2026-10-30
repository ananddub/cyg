use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct HistoryParams {
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DesyncParams {
    /// Narrow the list to one room; empty or absent means every room this
    /// process served.
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}
