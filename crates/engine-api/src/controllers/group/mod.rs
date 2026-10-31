use std::sync::Arc;

use auto_route::controller;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};

use crate::app::middleware::Authed;
use crate::app::params::{Body, Params};
use crate::current_state;

use crate::dto::group::{CreateGroupRequest, ListQuery, UpdateGroupRequest, UsersQuery};
use crate::services::group::{GroupMemberService, GroupService, UserGroupService};

mod group_controller;
mod user_group_controller;

pub use group_controller::*;
pub use user_group_controller::*;
