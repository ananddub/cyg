
#[derive(Debug, Clone)]
pub struct IapConfig {

    pub apple_shared_secret: String,

    pub apple_verify_url: String,

    pub apple_sandbox_url: String,

    pub google_service_account: String,

    pub google_package_name: String,

    pub google_token_url: String,

    pub google_api_base: String,

    pub allow_unverified: bool,
}

impl Default for IapConfig {
    fn default() -> Self {
        Self {
            apple_shared_secret: String::new(),
            apple_verify_url: "https://buy.itunes.apple.com/verifyReceipt".into(),
            apple_sandbox_url: "https://sandbox.itunes.apple.com/verifyReceipt".into(),
            google_service_account: String::new(),
            google_package_name: String::new(),
            google_token_url: "https://oauth2.googleapis.com/token".into(),
            google_api_base: "https://androidpublisher.googleapis.com/androidpublisher/v3".into(),
            allow_unverified: false,
        }
    }
}

impl IapConfig {

    pub fn apple_configured(&self) -> bool {
        !self.apple_shared_secret.trim().is_empty()
    }

    pub fn google_configured(&self) -> bool {
        !self.google_service_account.trim().is_empty()
            && !self.google_package_name.trim().is_empty()
    }
}
