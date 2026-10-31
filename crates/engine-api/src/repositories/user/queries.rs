use super::*;

impl Queries {
    pub async fn get_user_by_username_single(&self, username: &str) -> Result<Users, DbError> {
        self.one(
            "SELECT * FROM users WHERE username = $username AND disable_time = $epoch LIMIT 1",
            params!(username = username, epoch = time::EPOCH),
        )
        .await
    }

    pub async fn search_users_by_name(
        &self,
        username: &str,
        limit: i64,
    ) -> Result<Vec<Users>, DbError> {
        self.all(
            "SELECT * FROM users \
             WHERE string::contains(string::lowercase(username), $needle) \
               AND disable_time = $epoch \
             ORDER BY username ASC LIMIT $limit",
            params!(
                needle = username.to_lowercase(),
                epoch = time::EPOCH,
                limit = limit,
            ),
        )
        .await
    }

    pub async fn users_by_id(&self, ids: &[Uuid]) -> Result<HashMap<Uuid, Users>, DbError> {
        let mut unique = ids.to_vec();
        unique.sort();
        unique.dedup();
        if unique.is_empty() {
            return Ok(HashMap::new());
        }
        let mut bound = BTreeMap::new();
        let list = unique
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let name = format!("k{index}");
                bound.insert(name.clone(), serde_json::Value::String(id.to_string()));
                format!("type::record('users', <uuid>${name})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let users: Vec<Users> = self
            .all(
                &format!("SELECT * FROM users WHERE id INSIDE [{list}]"),
                bound,
            )
            .await?;
        Ok(users.into_iter().map(|user| (user.id, user)).collect())
    }
}
