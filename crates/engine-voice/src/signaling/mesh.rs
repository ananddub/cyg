use std::collections::HashMap;

use super::BROADCAST;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignalTarget<'a> {
    pub entity_id: u32,
    pub session_id: &'a str,
}

#[derive(Debug, Default)]
pub struct VoiceMesh {
    by_entity: HashMap<u32, Box<str>>,
    by_session: HashMap<Box<str>, u32>,
}

impl VoiceMesh {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn join(&mut self, entity_id: u32, session_id: &str) -> bool {
        if let Some(previous) = self.by_entity.get(&entity_id) {
            self.by_session.remove(&**previous);
        }
        if let Some(&previous_entity) = self.by_session.get(session_id) {
            self.by_entity.remove(&previous_entity);
        }
        let is_new = self
            .by_entity
            .insert(entity_id, session_id.into())
            .is_none();
        self.by_session.insert(session_id.into(), entity_id);
        is_new
    }

    pub fn leave(&mut self, entity_id: u32) -> Option<String> {
        let session = self.by_entity.remove(&entity_id)?;
        self.by_session.remove(&*session);
        Some(session.into())
    }

    pub fn leave_session(&mut self, session_id: &str) -> Option<u32> {
        let entity = self.by_session.remove(session_id)?;
        self.by_entity.remove(&entity);
        Some(entity)
    }

    #[inline]
    pub fn session_of(&self, entity_id: u32) -> Option<&str> {
        self.by_entity.get(&entity_id).map(|s| &**s)
    }

    #[inline]
    pub fn entity_of(&self, session_id: &str) -> Option<u32> {
        self.by_session.get(session_id).copied()
    }

    #[inline]
    pub fn contains(&self, entity_id: u32) -> bool {
        self.by_entity.contains_key(&entity_id)
    }

    #[inline]
    pub fn peer_count(&self) -> usize {
        self.by_entity.len()
    }

    #[inline]
    pub fn should_initiate(from: u32, to: u32) -> bool {
        from < to
    }

    pub fn for_each_peer(&self, except: u32, mut f: impl FnMut(SignalTarget<'_>)) {
        for (&entity_id, session) in &self.by_entity {
            if entity_id != except {
                f(SignalTarget {
                    entity_id,
                    session_id: session,
                });
            }
        }
    }

    #[inline]
    pub fn route(&self, from: u32, to: u32, mut f: impl FnMut(SignalTarget<'_>)) -> bool {
        if to == BROADCAST {
            let mut found = false;
            self.for_each_peer(from, |target| {
                found = true;
                f(target);
            });
            return found;
        }
        match self.session_of(to) {
            Some(session_id) if to != from => {
                f(SignalTarget {
                    entity_id: to,
                    session_id,
                });
                true
            }
            _ => false,
        }
    }

    pub fn peers_sorted(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.by_entity.keys().copied().collect();
        ids.sort_unstable();
        ids
    }
}
