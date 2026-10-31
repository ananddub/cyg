use super::*;

impl Queries {
    pub async fn get_tournament(&self, id: &str) -> Result<Leaderboard, DbError> {
        self.one(
            "SELECT * FROM type::record('leaderboard', $id) WHERE duration != 0 LIMIT 1",
            params!(id = id),
        )
        .await
    }
}
