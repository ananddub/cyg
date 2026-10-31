use super::*;

impl RealtimeMatchmakerService {
    pub fn stats(&self) -> MatchmakerStatsSnapshot {
        let now = Utc::now();
        let mut all: Vec<Ticket> = Vec::new();
        self.tickets.iter_sync(|_, t| {
            all.push(t.clone());
            true
        });
        let mut oldest: Option<DateTime<Utc>> = None;
        let mut groups: HashMap<String, Vec<i64>> = HashMap::new();
        for ticket in &all {
            let waited = (now - ticket.created_at).num_milliseconds();
            oldest = Some(oldest.map_or(ticket.created_at, |o| o.min(ticket.created_at)));
            groups.entry(ticket.query.clone()).or_default().push(waited);
        }
        let mut matchsets: Vec<MatchsetStats> = groups
            .into_iter()
            .map(|(matchset, waits)| MatchsetStats {
                matchset,
                ticket_count: waits.len() as i64,
                oldest_create_time: now
                    - chrono::Duration::milliseconds(waits.iter().copied().min().unwrap_or(0)),
                avg_wait_secs: waits.iter().copied().sum::<i64>() as f64
                    / waits.len() as f64
                    / 1000.0,
            })
            .collect();
        matchsets.sort_by(|a, b| a.matchset.cmp(&b.matchset));
        MatchmakerStatsSnapshot {
            ticket_count: all.len() as i64,
            oldest_ticket_create_time: oldest,
            matchsets,
        }
    }
}
