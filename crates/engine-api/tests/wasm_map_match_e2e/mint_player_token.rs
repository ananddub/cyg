use super::*;

pub fn mint_player_token(uid: Uuid, username: &str) -> String {
    engine_api::infra::jwt::mint_token_pair(uid, username, HashMap::new()).0
}
