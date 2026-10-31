use serde::Deserialize;

#[derive(Deserialize)]
pub struct SignalQuery {
    pub seat: Option<String>,
}
