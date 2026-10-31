use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::ApiState;
use crate::db::{Datastore, KvBudget, time};
use crate::dto::console::games::{
    CreateGameRequest, ENGINE_TYPES, GAME_BACKEND_MODES, GAME_STATUSES, GameConfig,
    UpdateGameRequest,
};
use crate::error::{INVALID_ARGUMENT, NOT_FOUND, PERMISSION_DENIED, UNAUTHENTICATED, UNAVAILABLE};
use crate::infra::console::{self, ConsoleRole};
use crate::services::games::service::{GamesService, backend_mode_for_engine};
use crate::services::games::{API_KEY_PREFIX, CLIENT_KEY_PREFIX, GAME_KEY_PREFIX};

use super::server::V2Server;
use super::v2_state;
use super::wasm_upload::empty_wasm;

mod wasm_bytes;
mod a_create_request_takes_the_config_flat_or_nested_and_no_credential;
mod the_games_writes_are_gated_on_the_admin_and_write_roles;
mod a_malformed_game_id_is_a_bad_request_naming_the_game;
mod the_asset_template_is_a_scriptable_object_document_of_the_sdk_class;
mod db_state;
mod create_game_mints_the_prefixed_keys_and_defaults_the_config;
mod a_flat_config_field_overrides_the_same_key_spelled_nested;
mod a_game_key_is_unique_and_a_name_is_not;
mod update_game_changes_only_the_fields_a_patch_names;
mod the_module_aggregates_read_the_room_modules_rows;
mod the_detail_view_counts_are_the_service_reads;
mod the_unity_asset_route_answers_with_the_attached_scriptable_object;
mod an_unknown_unity_settings_format_is_refused;

use wasm_bytes::*;
use a_malformed_game_id_is_a_bad_request_naming_the_game::*;
pub(crate) use db_state::db;
pub(crate) use db_state::skipped_without_database;
use the_detail_view_counts_are_the_service_reads::*;
