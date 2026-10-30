use serde::Serialize;

pub mod account;
pub mod auth;
pub mod channel;
pub mod console;
pub mod friend;
pub mod group;
pub mod iap;
pub mod inventory;
pub mod leaderboard;
pub mod link;
#[path = "match.rs"]
pub mod matches;
pub mod matchmake;
pub mod notification;
pub mod quest;
pub mod session;
pub mod signaling;
pub mod storage;
pub mod storage_index;
pub mod tournament;
pub mod user;

#[derive(Debug, Default, Serialize)]
pub struct Empty {}

#[cfg(test)]
mod tests {
    use super::Empty;

    #[test]
    fn empty_is_an_object_with_no_members() {
        assert_eq!(serde_json::to_string(&Empty {}).unwrap(), "{}");
    }
}
