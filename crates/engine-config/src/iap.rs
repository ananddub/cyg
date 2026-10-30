//! `[control.iap]`: receipt verification for in-app purchases.
//!
//! A purchase is only recorded once the store's own API confirms it. Apple is
//! checked against `verifyReceipt` (and the sandbox endpoint when Apple answers
//! 21007), Google against the Android Publisher API with a service-account
//! JWT. Stores with no verifier (Huawei, Facebook Instant, Samsung) answer 503
//! rather than trusting a token a client could have made up.
//!
//! `allow_unverified` is a local-development escape hatch: a test harness or a
//! sample project can record fake receipts without any store credentials. It
//! must never be set on a public deployment.

/// Store credentials and verification endpoints.
#[derive(Debug, Clone)]
pub struct IapConfig {
    /// App-specific shared secret for Apple `verifyReceipt`.
    pub apple_shared_secret: String,
    /// Production `verifyReceipt` endpoint.
    pub apple_verify_url: String,
    /// Sandbox endpoint, retried when production answers status 21007.
    pub apple_sandbox_url: String,
    /// Google service-account JSON (the whole file, as text).
    pub google_service_account: String,
    /// Android package name, e.g. `com.example.game`.
    pub google_package_name: String,
    /// OAuth token endpoint for the service-account JWT grant.
    pub google_token_url: String,
    /// Android Publisher API base, without a version suffix.
    pub google_api_base: String,
    /// Record purchases without contacting a store. Development only.
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
    /// Apple verification is possible once a shared secret is set.
    pub fn apple_configured(&self) -> bool {
        !self.apple_shared_secret.trim().is_empty()
    }

    /// Google verification needs both a service account and the package name.
    pub fn google_configured(&self) -> bool {
        !self.google_service_account.trim().is_empty()
            && !self.google_package_name.trim().is_empty()
    }
}
