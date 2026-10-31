use super::*;

impl Queries {
    pub async fn get_leaderboard(&self, id: &str) -> Result<Leaderboard, DbError> {
        self.one(
            "SELECT * FROM type::record('leaderboard', $id) LIMIT 1",
            params!(id = id),
        )
        .await
    }
    pub async fn define_leaderboard(
        &self,
        params_in: &DefineLeaderboardParams,
    ) -> Result<Leaderboard, DbError> {
        let now = Utc::now();
        let mut row = match self
            .opt::<Leaderboard>(
                "SELECT * FROM type::record('leaderboard', $id) LIMIT 1",
                params!(id = &params_in.id),
            )
            .await?
        {
            Some(row) => row,
            None => new_leaderboard(&params_in.id, now),
        };
        row.title = params_in.title.clone();
        row.description = params_in.description.clone();
        row.operator = params_in.operator;
        row.sort_order = params_in.sort_order;
        row.reset_schedule = Some(params_in.reset_schedule.clone());
        row.metadata = params_in.metadata.clone();
        row.authoritative = params_in.authoritative;
        row.category = params_in.category;
        row.max_num_score = params_in.max_num_score;

        let mut data = serde_json::to_value(&row)?;
        if let Value::Object(map) = &mut data {
            map.remove("id");
        }
        self.one(
            "UPSERT type::record('leaderboard', $id) CONTENT $data RETURN AFTER",
            params!(id = &params_in.id, data = data),
        )
        .await
    }
    pub async fn get_leaderboard_record(
        &self,
        params_in: &GetLeaderboardRecordParams,
    ) -> Result<LeaderboardRecord, DbError> {
        self.one(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id \
                   AND expiry_time = $expiry_time LIMIT 1"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                owner_id = params_in.owner_id.to_string(),
                expiry_time = ts(params_in.expiry_time),
            ),
        )
        .await
    }
    pub async fn get_leaderboard_record_rank_desc(
        &self,
        params_in: &GetLeaderboardRecordRankDescParams,
    ) -> Result<i64, DbError> {
        self.scalar(
            "RETURN array::len((SELECT id FROM leaderboard_record \
             WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
               AND (score > $score OR (score = $score AND subscore > $subscore) \
                    OR (score = $score AND subscore = $subscore AND owner_id > $owner_id))))",
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                score = params_in.score,
                subscore = params_in.subscore,
                owner_id = params_in.owner_id.to_string(),
            ),
        )
        .await
    }
    pub async fn get_leaderboard_record_rank_asc(
        &self,
        params_in: &GetLeaderboardRecordRankAscParams,
    ) -> Result<i64, DbError> {
        self.scalar(
            "RETURN array::len((SELECT id FROM leaderboard_record \
             WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
               AND (score < $score OR (score = $score AND subscore < $subscore) \
                    OR (score = $score AND subscore = $subscore AND owner_id < $owner_id))))",
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                score = params_in.score,
                subscore = params_in.subscore,
                owner_id = params_in.owner_id.to_string(),
            ),
        )
        .await
    }
    pub async fn count_leaderboard_records(
        &self,
        leaderboard_id: &str,
        expiry_time: DateTime<Utc>,
    ) -> Result<i64, DbError> {
        self.scalar(
            "RETURN array::len((SELECT id FROM leaderboard_record \
             WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time))",
            params!(
                leaderboard_id = leaderboard_id,
                expiry_time = ts(expiry_time),
            ),
        )
        .await
    }
    pub async fn first_leaderboard_records_asc(
        &self,
        params_in: &FirstLeaderboardRecordsAscParams,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
                 ORDER BY score ASC, subscore ASC, owner_id ASC LIMIT $limit"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                limit = params_in.limit,
            ),
        )
        .await
    }
    pub async fn first_leaderboard_records_desc(
        &self,
        params_in: &FirstLeaderboardRecordsDescParams,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
                 ORDER BY score DESC, subscore DESC, owner_id DESC LIMIT $limit"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                limit = params_in.limit,
            ),
        )
        .await
    }
}

impl Queries {
    pub async fn greater_leaderboard_records_asc(
        &self,
        params_in: &GreaterLeaderboardRecordsAscParams,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
                   AND (score > $score OR (score = $score AND subscore > $subscore) \
                        OR (score = $score AND subscore = $subscore AND owner_id > $owner_id)) \
                 ORDER BY score ASC, subscore ASC, owner_id ASC LIMIT $limit"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                score = params_in.score,
                subscore = params_in.subscore,
                owner_id = params_in.owner_id.to_string(),
                limit = params_in.limit,
            ),
        )
        .await
    }
    pub async fn less_leaderboard_records_desc(
        &self,
        params_in: &LessLeaderboardRecordsDescParams,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        self.all(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
                   AND (score < $score OR (score = $score AND subscore < $subscore) \
                        OR (score = $score AND subscore = $subscore AND owner_id < $owner_id)) \
                 ORDER BY score DESC, subscore DESC, owner_id DESC LIMIT $limit"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                score = params_in.score,
                subscore = params_in.subscore,
                owner_id = params_in.owner_id.to_string(),
                limit = params_in.limit,
            ),
        )
        .await
    }
    pub async fn list_leaderboard_records_for_owner(
        &self,
        params_in: &ListLeaderboardRecordsForOwnerParams,
    ) -> Result<Vec<LeaderboardRecord>, DbError> {
        let owners: Vec<String> = params_in.owner_id.iter().map(Uuid::to_string).collect();
        self.all(
            &format!(
                "SELECT {RECORD_COLUMNS} FROM leaderboard_record \
                 WHERE leaderboard_id = $leaderboard_id AND expiry_time = $expiry_time \
                   AND owner_id INSIDE $owner_id"
            ),
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                expiry_time = ts(params_in.expiry_time),
                owner_id = owners,
            ),
        )
        .await
    }
    pub async fn delete_leaderboard_record(
        &self,
        params_in: &DeleteLeaderboardRecordParams,
    ) -> Result<(), DbError> {
        self.exec(
            "DELETE leaderboard_record WHERE leaderboard_id = $leaderboard_id \
               AND owner_id = $owner_id AND expiry_time = $expiry_time",
            params!(
                leaderboard_id = &params_in.leaderboard_id,
                owner_id = params_in.owner_id.to_string(),
                expiry_time = ts(params_in.expiry_time),
            ),
        )
        .await
    }
    pub async fn update_tournament(
        &self,
        params_in: &UpdateTournamentParams,
    ) -> Result<Leaderboard, DbError> {
        self.one(
            UPDATE_TOURNAMENT,
            params!(
                id = &params_in.id,
                title = &params_in.title,
                description = &params_in.description,
                operator = params_in.operator,
                sort_order = params_in.sort_order,
                reset_schedule = &params_in.reset_schedule,
                metadata = &params_in.metadata,
                authoritative = params_in.authoritative,
                duration = params_in.duration,
                start_time = ts(params_in.start_time),
                end_time = ts(params_in.end_time),
                join_required = params_in.join_required,
                max_size = params_in.max_size,
                max_num_score = params_in.max_num_score,
            ),
        )
        .await
    }
    pub async fn delete_board(&self, id: &str) -> Result<Vec<Leaderboard>, DbError> {
        self.all(
            "DELETE type::record('leaderboard', $id) RETURN BEFORE",
            params!(id = id),
        )
        .await
    }
    pub async fn delete_board_records(&self, id: &str) -> Result<(), DbError> {
        self.exec(
            "DELETE leaderboard_record WHERE leaderboard_id = $id",
            params!(id = id),
        )
        .await
    }
    pub async fn leaderboard_record_for_write(
        &self,
        leaderboard_id: &str,
        owner_id: Uuid,
        expiry_time: DateTime<Utc>,
    ) -> Result<Option<LeaderboardRecord>, DbError> {
        self.opt(
            "SELECT * FROM leaderboard_record WHERE leaderboard_id = $leaderboard_id \
               AND owner_id = $owner_id AND expiry_time = $expiry_time LIMIT 1",
            params!(
                leaderboard_id = leaderboard_id,
                owner_id = owner_id.to_string(),
                expiry_time = ts(expiry_time),
            ),
        )
        .await
    }
    pub async fn create_leaderboard_record(
        &self,
        row: &LeaderboardRecord,
    ) -> Result<LeaderboardRecord, DbError> {
        self.one(
            "CREATE leaderboard_record CONTENT $data RETURN AFTER",
            params!(data = row),
        )
        .await
    }
    pub async fn update_leaderboard_record(
        &self,
        row: &LeaderboardRecord,
    ) -> Result<Option<LeaderboardRecord>, DbError> {
        self.opt(
            "UPDATE leaderboard_record SET score = $score, subscore = $subscore, \
                num_score = $num_score, metadata = $metadata, username = $username, \
                update_time = $update_time \
             WHERE leaderboard_id = $leaderboard_id AND owner_id = $owner_id \
               AND expiry_time = $expiry_time RETURN AFTER",
            params!(
                score = row.score,
                subscore = row.subscore,
                num_score = row.num_score,
                metadata = row.metadata,
                username = row.username,
                update_time = ts(row.update_time),
                leaderboard_id = row.leaderboard_id,
                owner_id = row.owner_id.to_string(),
                expiry_time = ts(row.expiry_time),
            ),
        )
        .await
    }
}
