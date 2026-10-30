use auto_di::singleton;

pub mod deliver;
mod drafting;
pub mod endpoints;
pub mod outbox;
pub mod worker;

pub(crate) use outbox::emit;
pub use worker::delivery_loop;

/// The facts the control plane pushes. An endpoint names the ones it wants, or
/// `*` for all of them.
pub const EVENTS: &[&str] = &[
    "account.created",
    "account.deleted",
    "iap.purchase.verified",
    "subscription.state_changed",
    "match.ended",
    "report.created",
    "quest.completed",
    "marketplace.trade.settled",
];

pub struct WebhookService;

#[singleton]
impl WebhookService {
    fn new() -> Self {
        Self
    }
}
