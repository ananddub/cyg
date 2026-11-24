use serde::Deserialize;

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct BanAccountRequest {
    #[serde(default)]
    pub reason: String,
}

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct AddNoteRequest {
    #[serde(default)]
    pub note: String,
}

#[derive(poem_openapi::Object, Debug, Default, Deserialize)]
pub struct UpdateWalletRequest {
    #[serde(default)]
    pub changeset: serde_json::Value,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
