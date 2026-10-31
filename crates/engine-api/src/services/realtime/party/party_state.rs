use super::*;

pub(super) const MAX_PARTY_SIZE: i32 = 4;
pub struct PartyState {
    pub id: Uuid,
    pub leader_sid: Uuid,
    pub open: bool,
    pub max_size: i32,
    pub members: HashMap<Uuid , Presence>,
}
pub struct RealtimePartyService {
    pub parties: FastMap<Uuid, PartyState>,
}
#[singleton]
impl RealtimePartyService {
    pub fn new() -> Self {
        Self {
            parties: new_fast_map(),
        }
    }
}
