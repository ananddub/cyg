use super::*;

impl Queries {
    pub async fn create_group(&self, params: &CreateGroupParams) -> Result<Groups, DbError> {
        let now = Utc::now();
        let group = Groups {
            id: params.id,
            creator_id: params.creator_id,
            name: params.name.clone(),
            description: Some(params.description.clone()),
            avatar_url: Some(params.avatar_url.clone()),
            lang_tag: params.lang_tag.clone(),
            metadata: params.metadata.clone(),
            state: params.state,
            edge_count: 1,
            max_count: params.max_count,
            create_time: now,
            update_time: now,
            disable_time: time::epoch(),
        };
        self.create("groups", Key::Uuid(params.id), &group).await
    }
    pub async fn get_group_by_id(&self, id: Uuid) -> Result<Groups, DbError> {
        self.one(
            "SELECT * FROM type::record('groups', <uuid>$id) \
             WHERE disable_time = $epoch LIMIT 1",
            params!(id = id.to_string(), epoch = time::EPOCH),
        )
        .await
    }
    pub async fn get_group_by_name(&self, name: &str) -> Result<Groups, DbError> {
        self.one(
            "SELECT * FROM groups WHERE name = $name LIMIT 1",
            params!(name = name),
        )
        .await
    }
    pub async fn update_group(&self, params: &UpdateGroupParams) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('groups', <uuid>$id) SET \
                name = $name, description = $description, avatar_url = $avatar_url, \
                lang_tag = $lang_tag, metadata = $metadata, max_count = $max_count, \
                update_time = $now",
            params!(
                name = params.name,
                description = params.description,
                avatar_url = params.avatar_url,
                lang_tag = params.lang_tag,
                metadata = params.metadata,
                max_count = params.max_count,
                now = time::encode(Utc::now()),
                id = params.id.to_string(),
            ),
        )
        .await
    }
    pub async fn set_group_state(&self, id: Uuid, state: i16) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('groups', <uuid>$id) \
             SET state = $state, update_time = $now",
            params!(
                state = state,
                now = time::encode(Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn delete_group(&self, id: Uuid) -> Result<(), DbError> {
        self.exec(
            "BEGIN TRANSACTION; \
             DELETE group_edge WHERE source_id = $id OR destination_id = $id; \
             UPDATE type::record('groups', <uuid>$id) \
             SET disable_time = $now, update_time = $now; \
             COMMIT;",
            params!(now = time::encode(Utc::now()), id = id.to_string()),
        )
        .await
    }
    pub async fn join_group_edge(&self, params: &JoinGroupEdgeParams) -> Result<(), DbError> {
        let edge = GroupEdge {
            source_id: params.source_id,
            position: params.position,
            update_time: Utc::now(),
            destination_id: params.destination_id,
            state: params.state,
        };
        match self
            .create("group_edge", Key::Uuid(Uuid::new_v4()), &edge)
            .await
        {
            Ok(_) => Ok(()),
            Err(DbError::Unique(_)) => {
                self.exec(
                    "UPDATE group_edge SET state = $state, update_time = $now \
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
    pub async fn leave_group_edge(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE group_edge WHERE source_id = $source_id \
             AND destination_id = $destination_id",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn update_group_edge_state(
        &self,
        params: &UpdateGroupEdgeStateParams,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE group_edge SET state = $state, update_time = $now \
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
}

impl Queries {
    pub async fn delete_group_edge(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE group_edge WHERE source_id = $source_id \
             AND destination_id = $destination_id",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn get_group_edge_state(
        &self,
        source_id: Uuid,
        destination_id: Uuid,
    ) -> Result<i16, DbError> {
        self.scalar::<i16>(
            "SELECT VALUE state FROM group_edge \
             WHERE source_id = $source_id AND destination_id = $destination_id \
             LIMIT 1",
            params!(
                source_id = source_id.to_string(),
                destination_id = destination_id.to_string(),
            ),
        )
        .await
    }
    pub async fn list_groups(&self, params: &ListGroupsParams) -> Result<Vec<Groups>, DbError> {
        self.all(
            "SELECT * FROM groups WHERE disable_time = $epoch \
               AND ($lang_tag = '' OR lang_tag = $lang_tag) \
               AND ($state < 0 OR state = $state) \
               AND ($edge_count < 0 OR edge_count <= $edge_count) \
             ORDER BY update_time ASC, edge_count ASC, id ASC \
             LIMIT $limit START $offset",
            params!(
                epoch = time::EPOCH,
                lang_tag = params.lang_tag,
                state = params.state,
                edge_count = params.edge_count,
                limit = params.limit,
                offset = params.offset,
            ),
        )
        .await
    }
    pub async fn list_groups_by_name(
        &self,
        params: &ListGroupsByNameParams,
    ) -> Result<Vec<Groups>, DbError> {
        let needle = params.name.trim_matches('%').to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }
        self.all(
            "SELECT * FROM groups \
             WHERE string::contains(string::lowercase(name), $needle) \
             ORDER BY name ASC LIMIT $limit START $offset",
            params!(
                needle = needle,
                limit = params.limit,
                offset = params.offset,
            ),
        )
        .await
    }
    pub async fn list_groups_by_size(
        &self,
        params: &ListGroupsBySizeParams,
    ) -> Result<Vec<Groups>, DbError> {
        self.all(
            "SELECT * FROM groups WHERE disable_time = $epoch \
               AND edge_count >= $edge_count_start AND edge_count <= $edge_count_end \
             ORDER BY edge_count DESC LIMIT $limit START $offset",
            params!(
                epoch = time::EPOCH,
                edge_count_start = params.edge_count_start,
                edge_count_end = params.edge_count_end,
                limit = params.limit,
                offset = params.offset,
            ),
        )
        .await
    }
    pub async fn list_groups_for_user(
        &self,
        source_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ListGroupsForUserRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE source_id = $source_id AND state < 3",
                params!(source_id = source_id.to_string()),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.destination_id).collect();
        let groups = groups_by_id(self, &ids).await?;
        let epoch = time::epoch();
        let mut rows: Vec<ListGroupsForUserRow> = edges
            .iter()
            .filter_map(|edge| {
                let group = groups.get(&edge.destination_id)?;
                if group.disable_time != epoch {
                    return None;
                }
                Some(ListGroupsForUserRow {
                    id: group.id,
                    creator_id: group.creator_id,
                    name: group.name.clone(),
                    description: group.description.clone(),
                    avatar_url: group.avatar_url.clone(),
                    lang_tag: group.lang_tag.clone(),
                    metadata: group.metadata.clone(),
                    state: group.state,
                    edge_count: group.edge_count,
                    max_count: group.max_count,
                    create_time: group.create_time,
                    update_time: group.update_time,
                    disable_time: group.disable_time,
                    member_state: Some(edge.state),
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            a.member_state
                .unwrap_or(0)
                .cmp(&b.member_state.unwrap_or(0))
                .then_with(|| a.name.cmp(&b.name))
        });
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }
}

impl Queries {
    pub async fn list_group_admins(
        &self,
        destination_id: Uuid,
        state: i16,
    ) -> Result<Vec<Uuid>, DbError> {
        self.all(
            "SELECT VALUE source_id FROM group_edge \
             WHERE destination_id = $destination_id AND state <= $state",
            params!(destination_id = destination_id.to_string(), state = state,),
        )
        .await
    }
    pub async fn list_groups_for_user_paged(
        &self,
        params: &ListGroupsForUserPagedParams,
    ) -> Result<Vec<ListGroupsForUserPagedRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE source_id = $source_id \
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
        let groups = groups_by_id(self, &ids).await?;
        let epoch = time::epoch();
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let group = groups.get(&edge.destination_id)?;
                if group.disable_time != epoch {
                    return None;
                }
                Some(group_row(group, edge.state, Some(edge.position)))
            })
            .collect())
    }
    pub async fn list_groups_for_user_by_state_paged(
        &self,
        params: &ListGroupsForUserByStatePagedParams,
    ) -> Result<Vec<ListGroupsForUserByStatePagedRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE source_id = $source_id \
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
        let groups = groups_by_id(self, &ids).await?;
        let epoch = time::epoch();
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let group = groups.get(&edge.destination_id)?;
                if group.disable_time != epoch {
                    return None;
                }
                Some(group_row(group, edge.state, Some(edge.position)))
            })
            .collect())
    }
}

impl Queries {
    pub async fn list_group_users(
        &self,
        destination_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ListGroupUsersRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE destination_id = $destination_id",
                params!(destination_id = destination_id.to_string()),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.source_id).collect();
        let users = self.users_by_id(&ids).await?;
        let mut rows: Vec<ListGroupUsersRow> = edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.source_id)?;
                Some(ListGroupUsersRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_update_time: Some(edge.update_time),
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            a.state
                .unwrap_or(0)
                .cmp(&b.state.unwrap_or(0))
                .then_with(|| a.username.cmp(&b.username))
        });
        rows.truncate(limit.max(0) as usize);
        Ok(rows)
    }
    pub async fn increment_group_edge_count(
        &self,
        edge_count_delta: i32,
        id: Uuid,
    ) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('groups', <uuid>$id) \
             SET edge_count = edge_count + $edge_count_delta, update_time = $now",
            params!(
                edge_count_delta = edge_count_delta,
                now = time::encode(Utc::now()),
                id = id.to_string(),
            ),
        )
        .await
    }
    pub async fn expand_group_edge_count(&self, id: Uuid) -> Result<i32, DbError> {
        let group: Groups = self
            .one(
                "UPDATE type::record('groups', <uuid>$id) \
                 SET edge_count = edge_count + 1, update_time = $now \
                 WHERE edge_count + 1 <= max_count RETURN AFTER",
                params!(now = time::encode(Utc::now()), id = id.to_string()),
            )
            .await?;
        Ok(group.edge_count)
    }
    pub async fn shrink_group_edge_count(&self, id: Uuid) -> Result<(), DbError> {
        self.exec(
            "UPDATE type::record('groups', <uuid>$id) \
             SET edge_count = edge_count - 1, update_time = $now \
             WHERE edge_count > 1",
            params!(now = time::encode(Utc::now()), id = id.to_string()),
        )
        .await
    }
    pub async fn count_group_role_holders(
        &self,
        destination_id: Uuid,
        state: i16,
    ) -> Result<i64, DbError> {
        Ok(self
            .opt::<i64>(
                "RETURN array::len((SELECT id FROM group_edge \
                 WHERE destination_id = $destination_id AND state = $state))",
                params!(destination_id = destination_id.to_string(), state = state,),
            )
            .await?
            .unwrap_or(0))
    }
    pub async fn promote_group_edge(&self, params: &PromoteGroupEdgeParams) -> Result<(), DbError> {
        self.shift_group_edge_state(
            "state = state - 1",
            "AND state > 0 AND state > $state AND state <= 2",
            params.destination_id,
            params.source_id,
            params.state,
        )
        .await
    }
    pub async fn demote_group_edge(&self, params: &DemoteGroupEdgeParams) -> Result<(), DbError> {
        self.shift_group_edge_state(
            "state = state + 1",
            "AND state >= $state AND state < 2",
            params.destination_id,
            params.source_id,
            params.state,
        )
        .await
    }
}

impl Queries {
    pub(super) async fn shift_group_edge_state(
        &self,
        set: &str,
        test: &str,
        destination_id: Uuid,
        source_id: Uuid,
        state: i16,
    ) -> Result<(), DbError> {
        let matches: Vec<GroupEdgeRef> = self
            .all(
                &format!(
                    "SELECT id FROM group_edge \
                     WHERE destination_id = $destination_id AND source_id = $source_id {test}"
                ),
                params!(
                    destination_id = destination_id.to_string(),
                    source_id = source_id.to_string(),
                    state = state,
                ),
            )
            .await?;
        for edge in matches {
            self.exec(
                &format!(
                    "UPDATE type::record('group_edge', <uuid>$id) SET {set}, update_time = $now"
                ),
                params!(id = edge.id.to_string(), now = time::encode(Utc::now()),),
            )
            .await?;
        }
        Ok(())
    }
    pub async fn ban_group_edge(&self, params: &BanGroupEdgeParams) -> Result<(), DbError> {
        let edge = GroupEdge {
            source_id: params.source_id,
            position: params.position,
            update_time: Utc::now(),
            destination_id: params.destination_id,
            state: 4,
        };
        match self
            .create("group_edge", Key::Uuid(Uuid::new_v4()), &edge)
            .await
        {
            Ok(_) => Ok(()),
            Err(DbError::Unique(_)) => {
                self.exec(
                    "UPDATE group_edge SET state = 4, update_time = $now \
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
    pub async fn list_group_users_paged(
        &self,
        params: &ListGroupUsersPagedParams,
    ) -> Result<Vec<ListGroupUsersPagedRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE destination_id = $destination_id \
                   AND state < 3 \
                   AND (state > $state OR (state = $state AND position > $position)) \
                 ORDER BY state ASC, position ASC LIMIT $limit",
                params!(
                    destination_id = params.destination_id.to_string(),
                    state = params.state,
                    position = params.position,
                    limit = params.limit,
                ),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.source_id).collect();
        let users = self.users_by_id(&ids).await?;
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.source_id)?;
                Some(ListGroupUsersPagedRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_position: Some(edge.position),
                    edge_update_time: Some(edge.update_time),
                })
            })
            .collect())
    }
    pub async fn list_group_users_by_state_paged(
        &self,
        params: &ListGroupUsersByStatePagedParams,
    ) -> Result<Vec<ListGroupUsersByStatePagedRow>, DbError> {
        let edges: Vec<GroupEdge> = self
            .all(
                "SELECT * FROM group_edge WHERE destination_id = $destination_id \
                   AND state = $state AND position > $position \
                 ORDER BY position ASC LIMIT $limit",
                params!(
                    destination_id = params.destination_id.to_string(),
                    state = params.state,
                    position = params.position,
                    limit = params.limit,
                ),
            )
            .await?;
        let ids: Vec<Uuid> = edges.iter().map(|edge| edge.source_id).collect();
        let users = self.users_by_id(&ids).await?;
        Ok(edges
            .iter()
            .filter_map(|edge| {
                let user = users.get(&edge.source_id)?;
                Some(ListGroupUsersByStatePagedRow {
                    id: user.id,
                    username: user.username.clone(),
                    display_name: user.display_name.clone(),
                    avatar_url: user.avatar_url.clone(),
                    metadata: user.metadata.clone(),
                    create_time: user.create_time,
                    update_time: user.update_time,
                    state: Some(edge.state),
                    edge_position: Some(edge.position),
                    edge_update_time: Some(edge.update_time),
                })
            })
            .collect())
    }
}
