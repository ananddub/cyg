use super::*;

impl RealtimeMatchService {
    pub fn join_match(
        &self,
        s: &Arc<Session>,
        match_id: Option<String>,
        token: Option<String>,
        label: Option<String>,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let sid = s.sid;

        let target: Uuid = if let Some(raw) = match_id.filter(|m| !m.is_empty()) {
            let id = Uuid::parse_str(&raw).map_err(|_| V2Error::not_found("match not found"))?;
            if !self.matches.contains_sync(&id) {
                return Err(V2Error::not_found("match not found"));
            }
            id
        } else if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
            self.tokens_to_match
                .read_sync(tok, |_, id| *id)
                .ok_or_else(|| V2Error::not_found("match token invalid or expired"))?
        } else if let Some(lbl) = label.filter(|l| !l.is_empty()) {
            let mut found = None;
            self.matches.iter_sync(|id, m| {
                if m.label == lbl && m.open {
                    found = Some(*id);
                    false
                } else {
                    true
                }
            });
            match found {
                Some(id) => id,
                None => {
                    let id = Uuid::new_v4();
                    let _ = self.matches.insert_sync(
                        id,
                        Match {
                            id,
                            label: lbl,
                            open: true,
                            tick_rate: 0,
                            members: HashMap::new(),
                            tokens: HashSet::new(),
                            ephemeral: false,
                        },
                    );
                    id
                }
            }
        } else {
            return Err(V2Error::invalid_argument(
                "match_join needs match_id, token or label",
            ));
        };

        let mut match_ref = self
            .matches
            .get_sync(&target)
            .ok_or_else(|| V2Error::not_found("match not found"))?;
        if let Some(tok) = token.as_ref().filter(|t| !t.is_empty()) {
            match_ref.tokens.remove(tok);
            self.tokens_to_match.remove_sync(tok);
        } else if !match_ref.open || match_ref.ephemeral {
            return Err(V2Error::permission_denied("not permitted to join match"));
        }
        let mine = self.presence_of(s);
        let others: Vec<Presence> = match_ref.members.values().cloned().collect();
        match_ref.members.insert(sid, mine.clone());
        let member_ids: Vec<Uuid> = match_ref
            .members
            .values()
            .filter_map(|p| Uuid::parse_str(&p.user_id).ok())
            .collect();
        drop(match_ref);

        let _ = s.matches.insert_sync(target);
        s.send_message(
            cid.clone(),
            EnvelopeMessage::MatchJoin {
                match_id: Some(target.simple().to_string()),
                token: None,
                label: None,
                presences: others,
            },
        );
        let event = Envelope {
            cid: None,
            message: EnvelopeMessage::MatchPresenceEvent {
                match_id: target.simple().to_string(),
                joins: vec![mine],
                leaves: Vec::new(),
            },
        };
        self.notify(&member_ids, sid, &event);
        Ok(())
    }
    pub fn leave_match(
        &self,
        s: &Arc<Session>,
        raw_id: &str,
        cid: Option<String>,
    ) -> Result<(), V2Error> {
        let id = Uuid::parse_str(raw_id).map_err(|_| V2Error::not_found("match not found"))?;
        let (member_ids, gone) = {
            let match_ref = self.matches.get_sync(&id);
            let Some(mut m) = match_ref else {
                let _ = s.matches.remove_sync(&id);
                return Ok(());
            };
            let gone = m.members.remove(&s.sid);
            let member_ids: Vec<Uuid> = m
                .members
                .values()
                .filter_map(|p| Uuid::parse_str(&p.user_id).ok())
                .collect();
            let empty = m.members.is_empty() && (m.ephemeral || m.tokens.is_empty());
            drop(m);
            if empty {
                if let Some((_, removed)) = self.matches.remove_sync(&id) {
                    for tok in &removed.tokens {
                        self.tokens_to_match.remove_sync(tok);
                    }
                }
            }
            (member_ids, gone)
        };
        let _ = s.matches.remove_sync(&id);
        if let Some(presence) = gone {
            let event = Envelope {
                cid: None,
                message: EnvelopeMessage::MatchPresenceEvent {
                    match_id: id.simple().to_string(),
                    joins: Vec::new(),
                    leaves: vec![presence],
                },
            };
            self.notify(&member_ids, s.sid, &event);
        }
        s.send_message(
            cid,
            EnvelopeMessage::MatchLeave {
                match_id: raw_id.to_string(),
            },
        );
        Ok(())
    }
}
