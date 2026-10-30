use super::*;

pub(super) async fn groups_by_id(
    queries: &Queries,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, Groups>, DbError> {
    let mut unique = ids.to_vec();
    unique.sort();
    unique.dedup();
    if unique.is_empty() {
        return Ok(HashMap::new());
    }
    let (list, bound) = record_list(&unique, "groups");
    let groups: Vec<Groups> = queries
        .all(
            &format!("SELECT * FROM groups WHERE id INSIDE [{list}]"),
            bound,
        )
        .await?;
    Ok(groups.into_iter().map(|group| (group.id, group)).collect())
}

pub(super) fn record_list(ids: &[Uuid], table: &str) -> (String, BTreeMap<String, Value>) {
    let mut bound = BTreeMap::new();
    let list = ids
        .iter()
        .enumerate()
        .map(|(index, id)| {
            let name = format!("k{index}");
            bound.insert(name.clone(), Value::String(id.to_string()));
            format!("type::record('{table}', <uuid>${name})")
        })
        .collect::<Vec<_>>()
        .join(", ");
    (list, bound)
}

pub(super) fn group_row(
    group: &Groups,
    member_state: i16,
    member_position: Option<i64>,
) -> ListGroupsForUserPagedRow {
    ListGroupsForUserPagedRow {
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
        member_state: Some(member_state),
        member_position,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGroupsForUserRow {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub state: i16,
    pub edge_count: i32,
    pub max_count: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub disable_time: DateTime<Utc>,
    pub member_state: Option<i16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGroupUsersRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub state: Option<i16>,
    pub edge_update_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGroupUsersPagedRow {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub metadata: serde_json::Value,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub state: Option<i16>,
    pub edge_position: Option<i64>,
    pub edge_update_time: Option<DateTime<Utc>>,
}

pub type ListGroupUsersByStatePagedRow = ListGroupUsersPagedRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListGroupsForUserPagedRow {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: Option<String>,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub state: i16,
    pub edge_count: i32,
    pub max_count: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub disable_time: DateTime<Utc>,
    pub member_state: Option<i16>,
    pub member_position: Option<i64>,
}

pub type ListGroupsForUserByStatePagedRow = ListGroupsForUserPagedRow;

pub struct CreateGroupParams {
    pub id: Uuid,
    pub creator_id: Uuid,
    pub name: String,
    pub description: String,
    pub avatar_url: String,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub max_count: i32,
    pub state: i16,
}

pub struct UpdateGroupParams {
    pub name: String,
    pub description: String,
    pub avatar_url: String,
    pub lang_tag: String,
    pub metadata: serde_json::Value,
    pub max_count: i32,
    pub id: Uuid,
}

pub type JoinGroupEdgeParams = AddFriendEdgeParams;

pub struct UpdateGroupEdgeStateParams {
    pub state: i16,
    pub source_id: Uuid,
    pub destination_id: Uuid,
}

pub struct ListGroupsParams {
    pub lang_tag: String,
    pub state: i16,
    pub edge_count: i32,
    pub limit: i64,
    pub offset: i64,
}

pub struct ListGroupsByNameParams {
    pub name: String,
    pub limit: i64,
    pub offset: i64,
}

pub struct ListGroupsBySizeParams {
    pub edge_count_start: i32,
    pub edge_count_end: i32,
    pub limit: i64,
    pub offset: i64,
}

pub struct PromoteGroupEdgeParams {
    pub destination_id: Uuid,
    pub source_id: Uuid,
    pub state: i16,
}

pub type DemoteGroupEdgeParams = PromoteGroupEdgeParams;

pub type BanGroupEdgeParams = BlockUserEdgesParams;

#[derive(Debug, Clone, Deserialize, somnia::SurrealRecord)]
#[table("group_edge", schemaless)]
pub(super) struct GroupEdgeRef {
    #[field(thing)]
    pub(super) id: Uuid,
}

pub struct ListGroupUsersPagedParams {
    pub destination_id: Uuid,
    pub state: i16,
    pub position: i64,
    pub limit: i64,
}

pub type ListGroupUsersByStatePagedParams = ListGroupUsersPagedParams;

pub type ListGroupsForUserPagedParams = ListUserEdgesPagedParams;

pub type ListGroupsForUserByStatePagedParams = ListUserEdgesPagedParams;
