use super::*;

impl Queries {
    pub async fn create_report(&self, params: &CreateReportParams) -> Result<ContentReport, DbError> {
        let now = Utc::now();
        let report = ContentReport {
            id: params.id,
            reporter_id: params.reporter_id,
            reported_id: params.reported_id,
            reason: params.reason.clone(),
            details: params.details.clone(),
            path: params.path.clone(),
            status: "open".to_owned(),
            create_time: now,
            update_time: now,
        };
        self.create("content_report", Key::Uuid(params.id), &report).await
    }

    pub async fn get_report(&self, id: Uuid) -> Result<Option<ContentReport>, DbError> {
        self.opt(
            "SELECT * FROM type::record('content_report', <uuid>$id) LIMIT 1",
            params!(id = id.to_string()),
        )
        .await
    }

    pub async fn list_reports_paged(
        &self,
        params: &ListReportsPagedParams,
    ) -> Result<Vec<ContentReport>, DbError> {
        self.all(
            "SELECT * FROM content_report \
               WHERE create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('content_report', <uuid>$id_start)) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                create_time = time::encode(params.create_time),
                id_start = params.id_start.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }

    pub async fn list_reports_by_status_paged(
        &self,
        params: &ListReportsByStatusPagedParams,
    ) -> Result<Vec<ContentReport>, DbError> {
        self.all(
            "SELECT * FROM content_report WHERE status = $status \
               AND (create_time > $create_time \
                    OR (create_time = $create_time \
                        AND id > type::record('content_report', <uuid>$id_start))) \
             ORDER BY create_time ASC, id ASC LIMIT $limit",
            params!(
                status = params.status.clone(),
                create_time = time::encode(params.create_time),
                id_start = params.id_start.to_string(),
                limit = params.limit,
            ),
        )
        .await
    }

    pub async fn update_report_status(
        &self,
        id: Uuid,
        status: &str,
    ) -> Result<Option<ContentReport>, DbError> {
        self.opt(
            "UPDATE type::record('content_report', <uuid>$id) \
             SET status = $status, update_time = $now RETURN AFTER",
            params!(
                id = id.to_string(),
                status = status.to_owned(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }

    pub async fn create_infraction(
        &self,
        params: &CreateInfractionParams,
    ) -> Result<UserInfraction, DbError> {
        let infraction = UserInfraction {
            id: params.id,
            user_id: params.user_id,
            kind: params.kind.clone(),
            note: params.note.clone(),
            expires_at: params.expires_at,
            issued_by: params.issued_by,
            create_time: Utc::now(),
        };
        self.create("user_infraction", Key::Uuid(params.id), &infraction)
            .await
    }

    pub async fn list_infractions(&self, user_id: Uuid) -> Result<Vec<UserInfraction>, DbError> {
        self.all(
            "SELECT * FROM user_infraction WHERE user_id = $user_id ORDER BY create_time DESC",
            params!(user_id = user_id.to_string()),
        )
        .await
    }

    pub async fn active_infraction(
        &self,
        user_id: Uuid,
        kind: &str,
    ) -> Result<Option<UserInfraction>, DbError> {
        self.opt(
            "SELECT * FROM user_infraction \
               WHERE user_id = $user_id AND kind = $kind AND expires_at > $now \
             ORDER BY expires_at DESC LIMIT 1",
            params!(
                user_id = user_id.to_string(),
                kind = kind.to_owned(),
                now = time::encode(Utc::now()),
            ),
        )
        .await
    }
}
