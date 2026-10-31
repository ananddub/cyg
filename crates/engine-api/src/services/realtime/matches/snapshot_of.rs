use super::*;

impl RealtimeMatchService {
    fn snapshot_of(&self, m: &Match) -> MatchSnapshot {
        MatchSnapshot {
            id: m.id,
            label: m.label.clone(),
            open: m.open,
            tick_rate: m.tick_rate,
            members: m.members.values().cloned().collect(),
        }
    }
    pub(super) fn presence_of(&self, s: &Session) -> Presence {
        Presence {
            user_id: s.uid.simple().to_string(),
            session_id: s.sid.simple().to_string(),
            username: s.username.clone(),
            node: None,
        }
    }
    pub(super) fn notify(&self, user_ids: &[Uuid], except_sid: Uuid, event: &Envelope) {
        let mut seen = HashSet::new();
        for uid in user_ids {
            if !seen.insert(*uid) {
                continue;
            }
            for sess in session::sessions_of(*uid) {
                if sess.sid != except_sid {
                    sess.send(event);
                }
            }
        }
    }
    pub fn match_exists(&self, id: Uuid) -> bool {
        self.matches.contains_sync(&id)
    }
    pub fn list_snapshots(&self) -> Vec<MatchSnapshot> {
        let mut list = Vec::new();
        self.matches.iter_sync(|_, m| {
            list.push(self.snapshot_of(m));
            true
        });
        list
    }
    pub fn snapshot(&self, id: Uuid) -> Option<MatchSnapshot> {
        self.matches.read_sync(&id, |_, m| self.snapshot_of(m))
    }
    pub fn create_match(
        &self,
        _s: &Arc<Session>,
        open: bool,
        tick_rate: i32,
        label: String,
    ) -> Uuid {
        let id = Uuid::new_v4();
        let _ = self.matches.insert_sync(
            id,
            Match {
                id,
                label,
                open,
                tick_rate,
                members: HashMap::new(),
                tokens: HashSet::new(),
                ephemeral: false,
            },
        );
        id
    }
    pub fn create_with_tokens(&self, tick_rate: i32, tokens: Vec<String>) -> Uuid {
        let id = Uuid::new_v4();
        for tok in &tokens {
            let _ = self.tokens_to_match.insert_sync(tok.clone(), id);
        }
        let _ = self.matches.insert_sync(
            id,
            Match {
                id,
                label: String::new(),
                open: false,
                tick_rate,
                members: HashMap::new(),
                tokens: tokens.into_iter().collect(),
                ephemeral: true,
            },
        );
        id
    }
}
