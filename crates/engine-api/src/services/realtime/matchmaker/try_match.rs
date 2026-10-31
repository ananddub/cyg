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
        let mut all_used: HashSet<Uuid> = HashSet::new();

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

            tickets.sort_by_key(|t| t.created_at);

            let mut used: HashSet<Uuid> = HashSet::new();
            for seed in &tickets {
                if used.contains(&seed.id) {
                    continue;
                }
                let mut group: Vec<Ticket> = vec![seed.clone()];
                used.insert(seed.id);
                for candidate in &tickets {
                    if group.len() as i32 >= seed.max_count {
                        break;
                    }
                    if used.contains(&candidate.id) {
                        continue;
                    }
                    let compatible = group.iter().all(|g| {
                        self.is_compatible(
                            &g.compiled_terms,
                            &candidate.string_props,
                            &candidate.numeric_props,
                        ) && self.is_compatible(
                            &candidate.compiled_terms,
                            &g.string_props,
                            &g.numeric_props,
                        )
                    }) && (group.len() as i32 + 1) <= candidate.max_count;
                    if compatible {
                        group.push(candidate.clone());
                        used.insert(candidate.id);
                    }
                }
                if group.len() as i32 >= group.iter().map(|t| t.min_count).max().unwrap_or(99) {
                    all_formed.push(group);
                } else {
                    for t in &group {
                        used.remove(&t.id);
                    }
                }
            }
            all_used.extend(used);
        }

        if all_formed.is_empty() {
            return;
        }

        for t_id in &all_used {
            if let Some((_, t)) = self.tickets.remove_sync(t_id) {
                self.unindex_ticket(&t);
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
