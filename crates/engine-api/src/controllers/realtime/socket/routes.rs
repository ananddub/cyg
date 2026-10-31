use super::*;

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
    pub(super) async fn connect(&self, conn: &Conn) {
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
    pub(super) async fn disconnect(&self, conn: &Conn) {
        let Some(session) = conn.get::<Arc<session::Session>>() else {
            return;
        };
        self.presence.on_disconnect(conn.id());
        self.matches.cleanup(&session);
        self.chat.cleanup(&session);
        self.party.cleanup(&session);
        self.matchmaker.drop_tickets(conn.id());
        self.sessions.unregister(conn.id());
    }

    #[on("ping")]
    pub(super) async fn ping(&self, _conn: &Conn, _msg: &Msg) -> Dispatched {
        Ok(Some(EnvelopeMessage::Pong))
    }

    #[on("matchmaker_add")]
    pub(super) async fn matchmaker_add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matchmaker.add(conn, msg).await
    }

    #[on("matchmaker_remove")]
    pub(super) async fn matchmaker_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matchmaker.remove(conn, msg).await
    }

    #[on("match_create")]
    pub(super) async fn match_create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.create(conn, msg).await
    }

    #[on("match_join")]
    pub(super) async fn match_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.join(conn, msg).await
    }

    #[on("match_leave")]
    pub(super) async fn match_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.leave(conn, msg).await
    }

    #[on("match_data_send")]
    pub(super) async fn match_data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.data_send(conn, msg).await
    }

    #[on("match_signal")]
    pub(super) async fn match_signal(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.matches.signal(conn, msg).await
    }

    #[on("party_create")]
    pub(super) async fn party_create(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.create(conn, msg).await
    }

    #[on("party_join")]
    pub(super) async fn party_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.join(conn, msg).await
    }

    #[on("party_leave")]
    pub(super) async fn party_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.leave(conn, msg).await
    }

    #[on("party_promote")]
    pub(super) async fn party_promote(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.promote(conn, msg).await
    }

    #[on("party_update")]
    pub(super) async fn party_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.update(conn, msg).await
    }

    #[on("party_close")]
    pub(super) async fn party_close(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.close(conn, msg).await
    }

    #[on("party_data_send")]
    pub(super) async fn party_data_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.data_send(conn, msg).await
    }

    #[on("party_matchmaker_add")]
    pub(super) async fn party_matchmaker_add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.matchmaker_add(conn, msg).await
    }

    #[on("party_matchmaker_remove")]
    pub(super) async fn party_matchmaker_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.party.matchmaker_remove(conn, msg).await
    }

    #[on("channel_join")]
    pub(super) async fn channel_join(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.join(conn, msg).await
    }

    #[on("channel_leave")]
    pub(super) async fn channel_leave(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.leave(conn, msg).await
    }

    #[on("channel_message_send")]
    pub(super) async fn channel_message_send(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_send(conn, msg).await
    }

    #[on("channel_message_update")]
    pub(super) async fn channel_message_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_update(conn, msg).await
    }

    #[on("channel_message_remove")]
    pub(super) async fn channel_message_remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.message_remove(conn, msg).await
    }

    #[on("stream_data")]
    pub(super) async fn stream_data(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.chat.stream_data(conn, msg).await
    }

    #[on("status_follow")]
    pub(super) async fn status_follow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.follow(conn, msg).await
    }

    #[on("status_unfollow")]
    pub(super) async fn status_unfollow(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.unfollow(conn, msg).await
    }

    #[on("status_update")]
    pub(super) async fn status_update(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.presence.update(conn, msg).await
    }

    #[on("notifications")]
    pub(super) async fn notifications_list(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        self.notifications.list(conn, msg).await
    }
}
