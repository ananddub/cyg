use super::*;

pub(super) struct RealtimeSocket {
    pub(super) sessions: Arc<RealtimeSessionHub>,
    pub(super) matches: Arc<RealtimeMatchService>,
    pub(super) chat: Arc<RealtimeChatService>,
    pub(super) party: Arc<RealtimePartyService>,
    pub(super) matchmaker: Arc<RealtimeMatchmakerService>,
    pub(super) presence: Arc<RealtimePresenceService>,
    pub(super) notifications: Arc<RealtimeNotificationService>,
}
