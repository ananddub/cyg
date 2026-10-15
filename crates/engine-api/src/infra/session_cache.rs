//! Token revocation for stateless sessions: a port of the blacklist-only
//! `LocalSessionCache`. Two levels, exactly as BaaS has them: per token id
//! (the `tid` both tokens of a pair share) and per user (`last_invalidation`).
//! The expired-entry sweep runs at the end of each write rather than on a ticker;
//! equivalent, since a token past `exp` is already refused by signature validation.

use std::collections::HashMap;
use std::sync::RwLock;

use uuid::Uuid;

/// Revoked session and refresh tokens, by user.
pub struct SessionCache {
    token_expiry_secs: i64,
    refresh_expiry_secs: i64,
    users: RwLock<HashMap<Uuid, UserEntry>>,
}

#[derive(Default)]
struct UserEntry {
    last_invalidation: i64,
    session_tokens: HashMap<String, i64>,
    refresh_tokens: HashMap<String, i64>,
}

impl SessionCache {
    /// The windows must match the ones [`crate::infra::jwt`] signs into the tokens:
    /// a stamp is compared against `exp - window` to learn when a token was issued.
    pub fn new(token_expiry_secs: i64, refresh_expiry_secs: i64) -> SessionCache {
        SessionCache {
            token_expiry_secs,
            refresh_expiry_secs,
            users: RwLock::new(HashMap::new()),
        }
    }

    pub fn is_valid_session(&self, uid: Uuid, exp: i64, token_id: &str) -> bool {
        let users = self.users.read().expect("session cache read lock");
        let Some(entry) = users.get(&uid) else {
            return true;
        };
        valid(entry, exp, token_id, self.token_expiry_secs, true)
    }

    pub fn is_valid_refresh(&self, uid: Uuid, exp: i64, token_id: &str) -> bool {
        let users = self.users.read().expect("session cache read lock");
        let Some(entry) = users.get(&uid) else {
            return true;
        };
        valid(entry, exp, token_id, self.refresh_expiry_secs, false)
    }

    /// Blacklist one session/refresh pair; empty ids are skipped so a logout naming
    /// only the refresh token leaves the already-expired session alone.
    pub fn remove(
        &self,
        uid: Uuid,
        session_exp: i64,
        session_id: &str,
        refresh_exp: i64,
        refresh_id: &str,
    ) {
        let mut users = self.users.write().expect("session cache write lock");
        let entry = users.entry(uid).or_default();
        // `exp + 1` keeps the entry alive one tick past the token it revokes, which is
        // what makes the sweep's `exp <= now` safe.
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
        sweep(
            &mut users,
            chrono::Utc::now().timestamp(),
            self.token_expiry_secs,
            self.refresh_expiry_secs,
        );
    }

    /// Invalidate everything this user holds, issued up to this instant.
    pub fn remove_all(&self, uid: Uuid) {
        let ts = chrono::Utc::now().timestamp();
        let mut users = self.users.write().expect("session cache write lock");
        let entry = users.entry(uid).or_default();
        if ts > entry.last_invalidation {
            entry.last_invalidation = ts;
            entry.session_tokens.clear();
            entry.refresh_tokens.clear();
        }
    }
}

/// `last_invalidation > 0` and an issue time before the stamp both mean revoked;
/// otherwise only this exact token id is.
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

/// Drop token ids whose revocation has expired, then the user record once both
/// lists are empty and its stamp is older than both windows.
fn sweep(users: &mut HashMap<Uuid, UserEntry>, ts: i64, token_expiry: i64, refresh_expiry: i64) {
    users.retain(|_, entry| {
        entry.session_tokens.retain(|_, exp| *exp > ts);
        entry.refresh_tokens.retain(|_, exp| *exp > ts);
        let stamped = entry.last_invalidation > 0;
        let aged = entry.last_invalidation < ts.saturating_sub(token_expiry)
            && entry.last_invalidation < ts.saturating_sub(refresh_expiry);
        !(entry.session_tokens.is_empty() && entry.refresh_tokens.is_empty() && (!stamped || aged))
    });
}
