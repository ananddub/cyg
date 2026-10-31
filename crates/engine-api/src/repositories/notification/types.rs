use super::*;

pub struct CreateNotificationParams {
    pub id: Uuid,
    pub user_id: Uuid,
    pub subject: String,
    pub content: serde_json::Value,
    pub sender_id: Uuid,
    pub code: i16,
}

pub struct ListNotificationsParams {
    pub user_id: Uuid,
    pub create_time: DateTime<Utc>,
    pub limit: i64,
}

pub struct ListNotificationsBySenderParams {
    pub user_id: Uuid,
    pub sender_id: Uuid,
    pub create_time: DateTime<Utc>,
    pub limit: i64,
}

pub struct ListNotificationsByCodeParams {
    pub user_id: Uuid,
    pub code: i16,
    pub create_time: DateTime<Utc>,
    pub limit: i64,
}

pub struct ListNotificationsPagedParams {
    pub user_id: Uuid,
    pub create_time: DateTime<Utc>,
    pub id_start: Uuid,
    pub limit: i64,
}

pub struct ListNotificationsBySenderPagedParams {
    pub user_id: Uuid,
    pub sender_id: Uuid,
    pub create_time: DateTime<Utc>,
    pub id_start: Uuid,
    pub limit: i64,
}

pub struct ListNotificationsByCodePagedParams {
    pub user_id: Uuid,
    pub code: i16,
    pub create_time: DateTime<Utc>,
    pub id_start: Uuid,
    pub limit: i64,
}
