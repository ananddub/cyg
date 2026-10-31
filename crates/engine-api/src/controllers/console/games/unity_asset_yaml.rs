use super::*;

pub(super) fn unity_asset_yaml(connection: &UnityConnectionDto) -> String {
    format!(
        r#"%YAML 1.1
%TAG !u! tag:unity3d.com,2011:
--- !u!114 &11400000
MonoBehaviour:
  m_ObjectHideFlags: 0
  m_CorrespondingSourceObject: {{fileID: 0}}
  m_PrefabInstance: {{fileID: 0}}
  m_PrefabAsset: {{fileID: 0}}
  m_GameObject: {{fileID: 0}}
  m_Enabled: 1
  m_EditorHideFlags: 0
  m_Script: {{fileID: 11500000, guid: {SERVER_SETTINGS_SCRIPT_GUID}, type: 3}}
  m_Name: CYGServerSettings
  serverHttpUrl: {server_http_url}
  serverWsPath: {server_ws_path}
  roomName: {room_name}
  maxPlayers: {max_players}
  protocolVersion: {PROTOCOL_VERSION}
  matchmakeMethod: {MATCHMAKE_METHOD}
"#,
        server_http_url = connection.server_http_url,
        server_ws_path = connection.server_ws_path,
        room_name = connection.default_room,
        max_players = connection.max_players,
    )
}
pub(super) fn parse_id(raw: &str) -> Result<Uuid, V2Error> {
    Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument("Invalid game id."))
}
pub(super) fn answer<T: Serialize>(result: Result<T, V2Error>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => err.into_response(),
    }
}
