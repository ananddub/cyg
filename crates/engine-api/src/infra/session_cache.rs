use ahash::RandomState;
use scc::HashMap as FastMap;
use std::collections::HashMap;
use uuid::Uuid;

pub struct SessionCache {
    token_expiry_secs: i64,
    refresh_expiry_secs: i64,
    users: FastMap<Uuid, UserEntry, RandomState>,
}

#[derive(Default)]
struct UserEntry {
    last_invalidation: i64,
    session_tokens: HashMap<String, i64>,
    refresh_tokens: HashMap<String, i64>,
}

impl SessionCache {
    pub fn new(token_expiry_secs: i64, refresh_expiry_secs: i64) -> SessionCache {
        SessionCache {
            token_expiry_secs,
            refresh_expiry_secs,
            users: FastMap::with_hasher(RandomState::new()),
        }
    }

    pub fn is_valid_session(&self, uid: Uuid, exp: i64, token_id: &str) -> bool {
        self.users
            .read_sync(&uid, |_, entry| {
                valid(entry, exp, token_id, self.token_expiry_secs, true)
            })
            .unwrap_or(true)
    }

    pub fn is_valid_refresh(&self, uid: Uuid, exp: i64, token_id: &str) -> bool {
        self.users
            .read_sync(&uid, |_, entry| {
                valid(entry, exp, token_id, self.refresh_expiry_secs, false)
            })
            .unwrap_or(true)
    }

    pub fn remove(
        &self,
        uid: Uuid,
        session_exp: i64,
        session_id: &str,
        refresh_exp: i64,
        refresh_id: &str,
    ) {
        let mut entry = self.users.entry_sync(uid).or_default();
        if !session_id.is_empty() {
            entry
                .session_tokens
                .insert(session_id.to_owned(), session_exp + 1);
        }
        if !refresh_id.is_empty() {
            entry
                .refresh_tokens
                .insert(refresh_id.to_owned(), refresh_exp + 1);
        }
        drop(entry);
        self.sweep();
    }

    pub fn remove_all(&self, uid: Uuid) {
        let ts = chrono::Utc::now().timestamp();
        let mut entry = self.users.entry_sync(uid).or_default();
        if ts > entry.last_invalidation {
            entry.last_invalidation = ts;
            entry.session_tokens.clear();
            entry.refresh_tokens.clear();
        }
    }

    fn sweep(&self) {
        let ts = chrono::Utc::now().timestamp();
        let token_expiry = self.token_expiry_secs;
        let refresh_expiry = self.refresh_expiry_secs;
        self.users.retain_sync(|_, entry| {
            entry.session_tokens.retain(|_, exp| *exp > ts);
            entry.refresh_tokens.retain(|_, exp| *exp > ts);
            let stamped = entry.last_invalidation > 0;
            let aged = entry.last_invalidation < ts.saturating_sub(token_expiry)
                && entry.last_invalidation < ts.saturating_sub(refresh_expiry);
            !(entry.session_tokens.is_empty()
                && entry.refresh_tokens.is_empty()
                && (!stamped || aged))
        });
    }
}

fn valid(entry: &UserEntry, exp: i64, token_id: &str, expiry_secs: i64, session: bool) -> bool {
    if entry.last_invalidation > 0 && exp.saturating_sub(expiry_secs) < entry.last_invalidation {
        return false;
    }
    let tokens = if session {
        &entry.session_tokens
    } else {
        &entry.refresh_tokens
    };
    !tokens.contains_key(token_id)
}
