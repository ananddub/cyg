//! The `/ws` socket: one `#[on]` handler per inbound event tag.
//!
//! Handlers are injected through `auto_di` singletons with zero global mutex contention.

use std::sync::Arc;

use auto_ws::{Conn, Msg, auto_ws};

use crate::infra::ws::envelope::EnvelopeMessage;
use crate::infra::ws::session;

use super::Dispatched;
use crate::services::realtime::{
    RealtimeChatService, RealtimeMatchService, RealtimeMatchmakerService,
    RealtimeNotificationService, RealtimePartyService, RealtimePresenceService, RealtimeSessionHub,
};

struct RealtimeSocket {
    sessions: Arc<RealtimeSessionHub>,
    matches: Arc<RealtimeMatchService>,
    chat: Arc<RealtimeChatService>,
    party: Arc<RealtimePartyService>,
    matchmaker: Arc<RealtimeMatchmakerService>,
    presence: Arc<RealtimePresenceService>,
    notifications: Arc<RealtimeNotificationService>,
}

#[auto_ws("/ws")]
impl RealtimeSocket {
    fn new(
        sessions: Arc<RealtimeSessionHub>,
        matches: Arc<RealtimeMatchService>,
        chat: Arc<RealtimeChatService>,
        party: Arc<RealtimePartyService>,
        matchmaker: Arc<RealtimeMatchmakerService>,
        presence: Arc<RealtimePresenceService>,
        notifications: Arc<RealtimeNotificationService>,
    ) -> Self {
        Self {
            sessions,
            matches,
            chat,
            party,
            matchmaker,
            presence,
            notifications,
        }
    }

    #[connect]
    async fn connect(&self, conn: &Conn) {
        let session = session::Session::new(
            conn.id(),
            conn.user_id(),
            conn.username().to_string(),
            conn.sender(),
        );
        self.sessions.register(session.clone());
        conn.set(session);
    }

    #[disconnect]
    async fn disconnect(&self, conn: &Conn) {
        let Some(session) = conn.get::<Arc<session::Session>>() else {
            return;
        };
        // The other planes resolve presences through the session, so it leaves last.
        self.presence.on_disconnect(conn.id());
        self.matches.cleanup(&session);
        self.chat.cleanup(&session);
        self.party.cleanup(&session);
        self.matchmaker.drop_tickets(conn.id());
        self.sessions.unregister(conn.id());
    }

    #[on("ping")]
    async fn ping(&self, _conn: &Conn, _msg: &Msg) -> Dispatched {
        Ok(Some(EnvelopeMessage::Pong))
    }

    #[on("matchmaker_add")]
    async fn matchmaker_add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matchmaker.add(conn, msg).await
    }

    #[on("matchmaker_remove")]
    async fn matchmaker_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matchmaker.remove(conn, msg).await
    }

    #[on("match_create")]
    async fn match_create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.create(conn, msg).await
    }

    #[on("match_join")]
    async fn match_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.join(conn, msg).await
    }

    #[on("match_leave")]
    async fn match_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.leave(conn, msg).await
    }

    #[on("match_data_send")]
    async fn match_data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.data_send(conn, msg).await
    }

    #[on("match_signal")]
    async fn match_signal(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.signal(conn, msg).await
    }

    #[on("party_create")]
    async fn party_create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.create(conn, msg).await
    }

    #[on("party_join")]
    async fn party_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.join(conn, msg).await
    }

    #[on("party_leave")]
    async fn party_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.leave(conn, msg).await
    }

    #[on("party_promote")]
    async fn party_promote(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.promote(conn, msg).await
    }

    #[on("party_update")]
    async fn party_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.update(conn, msg).await
    }

    #[on("party_close")]
    async fn party_close(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.close(conn, msg).await
    }

    #[on("party_data_send")]
    async fn party_data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.data_send(conn, msg).await
    }

    #[on("party_matchmaker_add")]
    async fn party_matchmaker_add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.matchmaker_add(conn, msg).await
    }

    #[on("party_matchmaker_remove")]
    async fn party_matchmaker_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.matchmaker_remove(conn, msg).await
    }

    #[on("channel_join")]
    async fn channel_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.join(conn, msg).await
    }

    #[on("channel_leave")]
    async fn channel_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.leave(conn, msg).await
    }

    #[on("channel_message_send")]
    async fn channel_message_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_send(conn, msg).await
    }

    #[on("channel_message_update")]
    async fn channel_message_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_update(conn, msg).await
    }

    #[on("channel_message_remove")]
    async fn channel_message_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_remove(conn, msg).await
    }

    #[on("stream_data")]
    async fn stream_data(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.stream_data(conn, msg).await
    }

    #[on("status_follow")]
    async fn status_follow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.follow(conn, msg).await
    }

    #[on("status_unfollow")]
    async fn status_unfollow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.unfollow(conn, msg).await
    }

    #[on("status_update")]
    async fn status_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.update(conn, msg).await
    }

    #[on("notifications")]
    async fn notifications_list(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.notifications.list(conn, msg).await
    }
}
