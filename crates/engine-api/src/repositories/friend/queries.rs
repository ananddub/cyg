use super::*;

impl Queries {
    pub async fn add_friend_edge(
        &self,
        params: &AddFriendEdgeParams,
    ) -> Result<(), DbError> {
        let edge = UserEdge {
            source_id: params.source_id,
            position: params.position,
            update_time: Utc::now(),
            destination_id: params.destination_id,
            state: params.state,
            metadata: serde_json::json!({}),
        };
        match self
            .create("user_edge", Key::Uuid(Uuid::new_v4()), &edge)
            .await
        {
            Ok(_) => Ok(()),
            Err(DbError::Unique(_)) => {
                self.exec(
                    "UPDATE user_edge SET state = $state, update_time = $now \
                     WHERE source_id = $source_id AND destination_id = $destination_id",
                    params!(
                        state = params.state,
                        now = time::encode(Utc::now()),
                        source_id = params.source_id.to_string(),
                        destination_id = params.destination_id.to_string(),
                    ),
                )
                .await
            }
            Err(err) => Err(err),
        }
    }
    pub async fn delete_friend_edges(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE user_edge \
             WHERE (source_id = $source_id AND destination_id = $destination_id) \
                OR (source_id = $destination_id AND destination_id = $source_id)",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn block_user_edges(
        &self,
        params: &BlockUserEdgesParams,
    ) -> Result<(), DbError> {
        let edge = UserEdge {
            source_id: params.source_id,
            position: params.position,
            update_time: Utc::now(),
            destination_id: params.destination_id,
            state: 3,
            metadata: serde_json::json!({}),
        };
        match self
            .create("user_edge", Key::Uuid(Uuid::new_v4()), &edge)
            .await
        {
            Ok(_) => Ok(()),
            Err(DbError::Unique(_)) => {
                self.exec(
                    "UPDATE user_edge SET state = 3, update_time = $now \
                     WHERE source_id = $source_id AND destination_id = $destination_id",
                    params!(
                        now = time::encode(Utc::now()),
                        source_id = params.source_id.to_string(),
                        destination_id = params.destination_id.to_string(),
                    ),
                )
                .await
            }
            Err(err) => Err(err),
        }
    }
    pub async fn delete_blocked_edges(
        &self,
        destination_id: Uuid,
        destination_id2: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE user_edge \
             WHERE ((source_id = $destination_id AND destination_id = $destination_id2) \
                OR (source_id = $destination_id2 AND destination_id = $destination_id)) \
               AND state = 3",
            params!(
                destination_id = destination_id.to_string(),
                destination_id2 = destination_id2.to_string(),
            ),
        )
        .await
    }
    pub async fn list_user_edges_by_state(
        &self,
        params: &ListUserEdgesByStateParams,
    ) -> Result<Vec<ListUserEdgesByStateRow>, DbError> {
        let edges: Vec<UserEdge> = self
            .all(
                "SELECT * FROM user_edge WHERE source_id = $source_id AND state = $state",
                params!(
                    source_id = params.source_id.to_string(),
                    state = params.state,
                ),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.destination_id).collect();
        let users = self.users_by_id(&ids).await?;
        let mut rows: Vec<ListUserEdgesByStateRow> = edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.destination_id)?;
                Some(ListUserEdgesByStateRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    lang_tag: user.lang_tag.clone(),
                    location: user.location.clone(),
                    timezone: user.timezone.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_update_time: Some(edge.update_time),
                })
            })
            .collect();
        rows.sort_by(|a, b| a.username.cmp(&b.username));
        rows.truncate(params.limit.max(0) as usize);
        Ok(rows)
    }
}

impl Queries {
    pub async fn delete_reverse_edge_not_blocked(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE user_edge WHERE source_id = $source_id \
             AND destination_id = $destination_id AND state != 3",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn delete_unblocked_edges(
        &self,
        destination_id: Uuid,
        destination_id2: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE user_edge \
             WHERE ((source_id = $destination_id AND destination_id = $destination_id2) \
                OR (source_id = $destination_id2 AND destination_id = $destination_id)) \
               AND state != 3",
            params!(
                destination_id = destination_id.to_string(),
                destination_id2 = destination_id2.to_string(),
            ),
        )
        .await
    }
    pub async fn list_user_edges_paged(
        &self,
        params: &ListUserEdgesPagedParams,
    ) -> Result<Vec<ListUserEdgesPagedRow>, DbError> {
        let edges: Vec<UserEdge> = self
            .all(
                "SELECT * FROM user_edge WHERE source_id = $source_id \
                   AND (state > $state OR (state = $state AND position > $position)) \
                 ORDER BY state ASC, position ASC LIMIT $limit",
                params!(
                    source_id = params.source_id.to_string(),
                    state = params.state,
                    position = params.position,
                    limit = params.limit,
                ),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.destination_id).collect();
        let users = self.users_by_id(&ids).await?;
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.destination_id)?;
                Some(ListUserEdgesPagedRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    lang_tag: user.lang_tag.clone(),
                    location: user.location.clone(),
                    timezone: user.timezone.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_position: Some(edge.position),
                    edge_update_time: Some(edge.update_time),
                    edge_metadata: Some(edge.metadata.clone()),
                })
            })
            .collect())
    }
    pub async fn list_user_edges_by_state_paged(
        &self,
        params: &ListUserEdgesByStatePagedParams,
    ) -> Result<Vec<ListUserEdgesByStatePagedRow>, DbError> {
        let edges: Vec<UserEdge> = self
            .all(
                "SELECT * FROM user_edge WHERE source_id = $source_id \
                   AND state = $state AND position > $position \
                 ORDER BY position ASC LIMIT $limit",
                params!(
                    source_id = params.source_id.to_string(),
                    state = params.state,
                    position = params.position,
                    limit = params.limit,
                ),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.destination_id).collect();
        let users = self.users_by_id(&ids).await?;
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.destination_id)?;
                Some(ListUserEdgesByStatePagedRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    lang_tag: user.lang_tag.clone(),
                    location: user.location.clone(),
                    timezone: user.timezone.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_position: Some(edge.position),
                    edge_update_time: Some(edge.update_time),
                    edge_metadata: Some(edge.metadata.clone()),
                })
            })
            .collect())
    }
    pub async fn accept_friend_invite(
        &self,
        destination_id: Uuid,
        destination_id2: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE user_edge SET state = 0, update_time = $now \
             WHERE ((source_id = $destination_id AND destination_id = $destination_id2) \
                OR (source_id = $destination_id2 AND destination_id = $destination_id)) \
               AND (state = 1 OR state = 2)",
            params!(
                now = time::encode(Utc::now()),
                destination_id = destination_id.to_string(),
                destination_id2 = destination_id2.to_string(),
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn get_friend_edge_state(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<i16, DbError> {
        self.scalar::<i16>(
            "SELECT VALUE state FROM user_edge \
             WHERE source_id = $source_id AND destination_id = $destination_id \
             LIMIT 1",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn increment_user_edge_count(
        &self,
        edge_count_delta: i32,
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('users', <uuid>$id) \
             SET edge_count = edge_count + $edge_count_delta, update_time = $now",
            params!(
                edge_count_delta = edge_count_delta,
                now = time::encode(Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn list_friends_of_friends(
        &self,
        params: &ListFriendsOfFriendsParams,
    ) -> Result<Vec<ListFriendsOfFriendsRow>, DbError> {
        let direct: Vec<UserEdge> = self
            .all(
                "SELECT * FROM user_edge WHERE source_id = $source_id AND state = 0",
                params!(source_id = params.source_id.to_string()),
            )
            .await?;
        let mut referrers: Vec<Uuid> = direct.iter().map(|edge| edge.destination_id).collect();
        referrers.sort();
        referrers.dedup();
        if referrers.is_empty() {
            return Ok(Vec::new());
        }
        let refs: Vec<String> = referrers.iter().map(Uuid::to_string).collect();
        let friends: Vec<String> = referrers.iter().map(Uuid::to_string).collect();

        let edges: Vec<UserEdge> = self
            .all(
                "SELECT * FROM user_edge WHERE state = 0 \
                   AND source_id INSIDE $refs \
                   AND destination_id != $source_id \
                   AND NOT (destination_id INSIDE $friends) \
                   AND (source_id > $source_id2 \
                        OR (source_id = $source_id2 AND destination_id > $destination_id_start)) \
                 ORDER BY source_id ASC, destination_id ASC LIMIT $limit",
                params!(
                    refs = refs,
                    friends = friends,
                    source_id = params.source_id.to_string(),
                    source_id2 = params.source_id2.to_string(),
                    destination_id_start = params.destination_id_start.to_string(),
                    limit = params.limit,
                ),
            )
            .await?;

        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.destination_id).collect();
        let users = self.users_by_id(&ids).await?;
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.destination_id)?;
                Some(ListFriendsOfFriendsRow {
                    referrer: Some(edge.source_id),
                    id: Some(edge.destination_id),
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                })
            })
            .collect())
    }
    pub async fn update_friend_edge_metadata(
        &self,
        params: &UpdateFriendEdgeMetadataParams,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE user_edge SET metadata = $metadata, update_time = $now \
             WHERE source_id = $source_id AND destination_id = $destination_id",
            params!(
                metadata = params.metadata,
                now = time::encode(Utc::now()),
                source_id = params.source_id.to_string(),
                destination_id = params.destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn insert_friend_edge(
        &self,
        params: &InsertFriendEdgeParams,
    ) -> Result<(), DbError> {
        let edge = UserEdge {
            source_id: params.source_id,
            position: params.position,
            update_time: Utc::now(),
            destination_id: params.destination_id,
            state: params.state,
            metadata: params.metadata.clone(),
        };
        match self
            .create("user_edge", Key::Uuid(Uuid::new_v4()), &edge)
            .await
        {
            Ok(_) | Err(DbError::Unique(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }
    pub async fn accept_friend_edges(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE user_edge SET state = 0, update_time = $now \
             WHERE (source_id = $source_id AND destination_id = $destination_id) \
                OR (source_id = $destination_id AND destination_id = $source_id)",
            params!(
                now = time::encode(Utc::now()),
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn increment_pair_edge_count(
        &self,
        params: &IncrementPairEdgeCountParams,
    ) -> Result<(), DbError> {
        let pair: Option<UserEdge> = self
            .opt(
                "SELECT * FROM user_edge \
                 WHERE ((source_id = $id AND destination_id = $id2) \
                    OR (source_id = $id2 AND destination_id = $id)) \
                   AND position = $position LIMIT 1",
                params!(
                    id = params.id.to_string(),
                    id2 = params.id2.to_string(),
                    position = params.position,
                ),
            )
            .await?;
        if pair.is_none() {
            return Ok(());
        }
        self.exec(
            "UPDATE users SET edge_count = edge_count + 1, update_time = $now \
             WHERE id = type::record('users', <uuid>$id) \
                OR id = type::record('users', <uuid>$id2)",
            params!(
                now = time::encode(Utc::now()),
                id = params.id.to_string(),
                id2 = params.id2.to_string(),
            ),
        )
        .await
    }
    pub async fn delete_user_edge(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE user_edge WHERE source_id = $source_id \
             AND destination_id = $destination_id",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
}
