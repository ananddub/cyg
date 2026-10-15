//! In-app purchases and subscriptions (`/v2/iap*`).
//!
//! BaaS parity: `server/api_purchase.go`, `api_subscription.go`. Vendor validation
//! credentials are absent, so receipts are recorded verbatim (row key = MD5 of the receipt).

use auto_di::singleton;

pub mod purchases;
pub mod subscriptions;

pub struct IapService;

#[singleton]
impl IapService {
    fn new() -> Self {
        Self
    }
}
