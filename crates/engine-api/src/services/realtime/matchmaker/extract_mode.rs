use super::*;

impl RealtimeMatchmakerService {
    pub fn extract_mode(&self, ticket: &Ticket) -> String {
        if let Some(m) = ticket
            .string_props
            .get("mode")
            .or_else(|| ticket.string_props.get("game_mode"))
        {
            if !m.is_empty() {
                return m.clone();
            }
        }
        for term in ticket.query.split_whitespace() {
            if let Some(rest) = term.strip_prefix("+properties.mode:") {
                let clean = rest.trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
            if let Some(rest) = term.strip_prefix("+properties.game_mode:") {
                let clean = rest.trim();
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
        "default".to_string()
    }
    pub(super) fn index_ticket(&self, ticket: &Ticket) {
        let mode = self.extract_mode(ticket);
        if let Some(bucket) = self.mode_index.get_sync(&mode) {
            let _ = bucket.insert_sync(ticket.id, ());
        } else {
            let bucket = new_fast_map();
            let _ = bucket.insert_sync(ticket.id, ());
            let _ = self.mode_index.insert_sync(mode, bucket);
        }

        if let Some(rank) = ticket
            .numeric_props
            .get("rank")
            .or_else(|| ticket.numeric_props.get("mmr"))
        {
            let _ = self.rank_index.insert_sync((*rank as i64, ticket.id), ());
        }
    }
    pub(super) fn unindex_ticket(&self, ticket: &Ticket) {
        let mode = self.extract_mode(ticket);
        if let Some(bucket) = self.mode_index.get_sync(&mode) {
            bucket.remove_sync(&ticket.id);
        }
        if let Some(rank) = ticket
            .numeric_props
            .get("rank")
            .or_else(|| ticket.numeric_props.get("mmr"))
        {
            let _ = self.rank_index.remove_sync(&((*rank as i64), ticket.id));
        }
    }
    pub fn find_tickets_in_rank_range(&self, min_rank: i64, max_rank: i64) -> Vec<Uuid> {
        let guard = Guard::new();
        self.rank_index
            .range((min_rank, Uuid::nil())..=(max_rank, Uuid::max()), &guard)
            .map(|((_, id), _)| *id)
            .collect()
    }
    pub fn add_ticket_internal(
        &self,
        s: &Arc<Session>,
        min_count: i32,
        max_count: i32,
        query: String,
        string_props: HashMap<String, String>,
        numeric_props: HashMap<String, f64>,
    ) -> Result<String, EnvelopeMessage> {
        if !(2..=8).contains(&min_count) || !(2..=8).contains(&max_count) || min_count > max_count {
            return Err(EnvelopeMessage::Error(ErrorMsg {
                message: "min_count and max_count must be between 2 and 8, and min <= max".into(),
            }));
        }
        let compiled_terms = self.compile_query(&query);
        let ticket = Ticket {
            id: Uuid::new_v4(),
            sid: s.sid,
            uid: s.uid,
            username: s.username.clone(),
            min_count,
            max_count,
            query,
            compiled_terms,
            string_props,
            numeric_props,
            party_id: s.party_id(),
            created_at: Utc::now(),
        };
        let id_str = ticket.id.simple().to_string();
        self.index_ticket(&ticket);
        let _ = self.tickets.insert_sync(ticket.id, ticket);
        self.spawn_tick();
        Ok(id_str)
    }
    pub fn remove_ticket_internal(&self, s: &Arc<Session>, ticket: &str) -> bool {
        let Ok(id) = Uuid::parse_str(ticket) else {
            return false;
        };
        let should_remove = self
            .tickets
            .read_sync(&id, |_, t| t.sid == s.sid)
            .unwrap_or(false);
        if should_remove {
            if let Some((_, t)) = self.tickets.remove_sync(&id) {
                self.unindex_ticket(&t);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
    pub fn drop_tickets(&self, sid: Uuid) {
        let mut to_remove = Vec::new();
        self.tickets.iter_sync(|id, t| {
            if t.sid == sid {
                to_remove.push(*id);
            }
            true
        });
        for id in to_remove {
            if let Some((_, t)) = self.tickets.remove_sync(&id) {
                self.unindex_ticket(&t);
            }
        }
    }
}
