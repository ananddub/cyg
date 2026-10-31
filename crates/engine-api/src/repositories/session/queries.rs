use super::*;

impl Queries {
    pub async fn get_user_tombstone(&self, user_id: Uuid) -> Result<UserTombstone, DbError> {
        self.one(
            "SELECT * FROM type::record('user_tombstone', <uuid>$user_id)",
            params!(user_id = user_id.to_string()),
        )
        .await
    }

    pub async fn delete_user_tombstone(&self, user_id: Uuid) -> Result<(), DbError> {
        self.exec(
            "DELETE type::record('user_tombstone', <uuid>$user_id)",
            params!(user_id = user_id.to_string()),
        )
        .await
    }
}
