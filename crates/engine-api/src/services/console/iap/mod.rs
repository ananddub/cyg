//! `/v2/console/purchase`, `/v2/console/subscription` and `/v2/console/iap` —
//! the IAP ledger, the vendor validation an operator can run, the stored vendor
//! configuration a vendor switch needs cleared, and the product-level delete.
//!
//! The vendor names and what each one can do mirror the reference server: all
//! five validate purchases; subscriptions exist for Apple, Google and Huawei
//! only — the surface this crate's player API records.

use axum::Json;

use crate::dto::console::Page;
use crate::dto::console::iap::{PurchaseRow, SubscriptionRow};
use crate::error::V2Error;
use crate::ApiState;

pub mod maintenance;
pub mod validate;

/// A vendor the console can name in `/v2/console/iap/{vendor}/…`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Apple,
    Google,
    Huawei,
    FacebookInstant,
    Samsung,
}

impl Vendor {
    /// The path spelling, exactly as the reference server's routes name them.
    pub fn parse(name: &str) -> Option<Vendor> {
        match name {
            "apple" => Some(Vendor::Apple),
            "google" => Some(Vendor::Google),
            "huawei" => Some(Vendor::Huawei),
            "facebookinstant" => Some(Vendor::FacebookInstant),
            "samsung" => Some(Vendor::Samsung),
            _ => None,
        }
    }

    /// The store code the player routes record this vendor under.
    pub fn store(self) -> i16 {
        match self {
            Vendor::Apple => 0,
            Vendor::Google => 1,
            Vendor::Huawei => 2,
            Vendor::FacebookInstant => 3,
            Vendor::Samsung => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Vendor::Apple => "Apple",
            Vendor::Google => "Google",
            Vendor::Huawei => "Huawei",
            Vendor::FacebookInstant => "Facebook Instant",
            Vendor::Samsung => "Samsung",
        }
    }

    /// Whether this crate's player API records subscriptions for the vendor.
    pub fn supports_subscriptions(self) -> bool {
        matches!(self, Vendor::Apple | Vendor::Google | Vendor::Huawei)
    }
}

impl super::ConsoleService {
    pub async fn list_purchases(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<PurchaseRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_all_purchases(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(PurchaseRow::from).collect()))
    }

    pub async fn list_subscriptions(
        &self,
        state: &ApiState,
        page: Page,
    ) -> Result<Json<Vec<SubscriptionRow>>, V2Error> {
        let rows = super::queries(state)?
            .list_all_subscriptions(page.limit(), page.offset())
            .await?;
        Ok(Json(rows.into_iter().map(SubscriptionRow::from).collect()))
    }
}
