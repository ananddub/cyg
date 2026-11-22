use super::*;

impl RealtimeMatchmakerService {
    pub fn try_match(&self) {
        if self.tickets.is_empty() {
            return;
        }

        let mut modes: Vec<String> = Vec::new();
        self.mode_index.iter_sync(|mode, _| {
            modes.push(mode.clone());
            true
        });

        if modes.is_empty() {
            return;
        }

        let mut all_formed: Vec<Vec<Ticket>> = Vec::new();

        for mode in modes {
            let Some(bucket) = self.mode_index.get_sync(&mode) else {
                continue;
            };
            let mut tickets: Vec<Ticket> = Vec::new();
            bucket.iter_sync(|id, _| {
                if let Some(t) = self.tickets.read_sync(id, |_, t| t.clone()) {
                    tickets.push(t);
                }
                true
            });

            if tickets.len() < 2 {
                continue;
            }
            all_formed.extend(self.form_groups(&tickets));
        }

        if all_formed.is_empty() {
            return;
        }

        for group in &all_formed {
            for member in group {
                if let Some((_, t)) = self.tickets.remove_sync(&member.id) {
                    self.unindex_ticket(&t);
                }
            }
        }

        for group in all_formed {
            let tokens: Vec<String> = group
                .iter()
                .map(|_| Uuid::new_v4().simple().to_string())
                .collect();
            let match_id = RealtimeMatchService::create_match_with_tokens(0, tokens.clone());
            for (i, member) in group.iter().enumerate() {
                let Some(sess) = session::find(member.sid) else {
                    continue;
                };
                let users: Vec<MatchmakerUser> = group
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, other)| MatchmakerUser {
                        presence: Presence {
                            user_id: other.uid.simple().to_string(),
                            session_id: other.sid.simple().to_string(),
                            username: other.username.clone(),
                            node: None,
                        },
                        party_id: other.party_id.map(|p| p.simple().to_string()),
                        string_properties: other.string_props.clone(),
                        numeric_properties: other.numeric_props.clone(),
                    })
                    .collect();
                let self_presence = MatchmakerUser {
                    presence: RealtimePresenceService::get_presence_of(&sess),
                    party_id: member.party_id.map(|p| p.simple().to_string()),
                    string_properties: member.string_props.clone(),
                    numeric_properties: member.numeric_props.clone(),
                };
                sess.send_message(
                    None,
                    EnvelopeMessage::MatchmakerMatched(MatchmakerMatched {
                        ticket: member.id.simple().to_string(),
                        match_id: match_id.simple().to_string(),
                        token: tokens[i].clone(),
                        users,
                        self_presence,
                    }),
                );
            }
        }
    }
    /// Packs tickets into groups without ever splitting a party. A party is
    /// all-or-nothing: every member carries the same `party_id`, and a group
    /// either admits the whole party or none of it. Solo tickets form a party
    /// of one keyed by their own ticket id.
    pub(super) fn form_groups(&self, tickets: &[Ticket]) -> Vec<Vec<Ticket>> {
        let mut sorted = tickets.to_vec();
        sorted.sort_by_key(|t| t.created_at);
        let mut used: HashSet<Uuid> = HashSet::new();
        let mut groups: Vec<Vec<Ticket>> = Vec::new();

        for seed in &sorted {
            if used.contains(&seed.id) {
                continue;
            }
            let seed_party = Uuid::from(seed.party_id.unwrap_or(seed.id));
            let mut group: Vec<Ticket> = sorted
                .iter()
                .filter(|t| Uuid::from(t.party_id.unwrap_or(t.id)) == seed_party)
                .cloned()
                .collect();
            // A partially consumed party can never be admitted — the earlier
            // group would have taken only some members.
            if group.iter().any(|t| used.contains(&t.id)) {
                continue;
            }
            let mut group_parties: HashSet<Uuid> = HashSet::from([seed_party]);

            let wait_secs = (Utc::now() - seed.created_at).num_seconds().max(0) as f64;
            // A queue is a negotiation: the longer a ticket waits, the more
            // rating distance it accepts. 100 points at t=0, +10/s, capped at
            // 2000 so a pool cannot eventually pair a rookie with a pro.
            let window = (100.0 + wait_secs * 10.0).min(2000.0);

            for candidate in &sorted {
                if group.len() as i32 >= seed.max_count {
                    break;
                }
                let party = Uuid::from(candidate.party_id.unwrap_or(candidate.id));
                if group_parties.contains(&party) {
                    continue;
                }
                let all_members: Vec<Ticket> = sorted
                    .iter()
                    .filter(|t| Uuid::from(t.party_id.unwrap_or(t.id)) == party)
                    .cloned()
                    .collect();
                // A party that is partly used (or partly outside this mode
                // bucket) stays queued as a unit rather than being broken up.
                if all_members.is_empty() || all_members.iter().any(|t| used.contains(&t.id)) {
                    continue;
                }
                let total = group.len() + all_members.len();
                if total > seed.max_count as usize {
                    continue;
                }
                if !group
                    .iter()
                    .chain(all_members.iter())
                    .all(|t| total <= t.max_count as usize)
                {
                    continue;
                }
                let compatible = group.iter().all(|g| {
                    all_members.iter().all(|member| {
                        self.is_compatible(
                            &g.compiled_terms,
                            &member.string_props,
                            &member.numeric_props,
                        ) && self.is_compatible(
                            &member.compiled_terms,
                            &g.string_props,
                            &g.numeric_props,
                        ) && Self::rank_compatible(g, member, window)
                    })
                });
                if compatible {
                    group.extend(all_members);
                    group_parties.insert(party);
                }
            }

            let min_needed = group.iter().map(|t| t.min_count).max().unwrap_or(99);
            if group.len() as i32 >= min_needed {
                for t in &group {
                    used.insert(t.id);
                }
                groups.push(group);
            }
        }
        groups
    }

    pub(super) fn rank_compatible(a: &Ticket, b: &Ticket, window: f64) -> bool {
        match (a.rating(), b.rating()) {
            (Some(a), Some(b)) => (a - b).abs() <= window,
            _ => true,
        }
    }

    pub async fn add(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let add: MatchmakerAdd = msg.payload()?;
        self.add_ticket_internal(
            &session_of(conn),
            add.min_count,
            add.max_count,
            add.query,
            add.string_properties,
            add.numeric_properties,
        )
        .map(|ticket| Some(EnvelopeMessage::MatchmakerTicket { ticket }))
        .map_err(unwrap_env_err)
    }
    pub async fn remove(&self, conn: &Conn, msg: &Msg) -> Dispatched {
        let req: MatchmakerRemove = msg.payload()?;
        if self.remove_ticket_internal(&session_of(conn), &req.ticket) {
            Ok(Some(EnvelopeMessage::MatchmakerDone))
        } else {
            Err(V2Error::not_found("matchmaker ticket not found"))
        }
    }
}
