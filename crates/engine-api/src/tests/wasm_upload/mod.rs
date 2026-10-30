use crate::dto::console::wasm::ActivateRequest;
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED};
use crate::infra::console::{self, ConsoleRole};
use crate::services::wasm::REQUIRED_EXPORTS;
use crate::services::wasm::upload::WasmService;
use uuid::Uuid;

use super::games::db;
use super::server::V2Server;
use super::v2_state;

mod a_module_built_from_the_declared_abi_is_a_real_module;
mod abi_exports;
mod activate_module_moves_the_flag_and_rollback_is_the_same_flip;
mod delete_module_refuses_the_active_version;
mod ensure_valid_names_the_mistake_the_report_found;
mod every_wasm_route_promotes_a_relay_game_that_reaches_it;
mod load_boot_module_returns_the_active_version_and_its_bytes;
mod the_report_counts_the_exports_a_module_actually_has;

pub(crate) use abi_exports::empty_wasm;
use abi_exports::*;
use ensure_valid_names_the_mistake_the_report_found::*;
