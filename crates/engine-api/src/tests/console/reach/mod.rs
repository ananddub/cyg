use uuid::Uuid;

use crate::error::{FAILED_PRECONDITION, INVALID_ARGUMENT, PERMISSION_DENIED, UNAUTHENTICATED};
use crate::infra::console::ConsoleRole;

use super::helpers::{NOBODY, V2Server, player_token, token_for, v2_state};

mod each_new_route_demands_a_console_token;
mod the_read_and_write_gates_on_group_export_audit_and_record_delete;

