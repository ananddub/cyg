use axum::Json;
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::ApiState;
use crate::app::middleware::ConsoleAuthed;
use crate::dto::console::rules::{RuleDryRun, RuleDryRunResult, RuleRequest, RuleRow};
use crate::error::V2Error;
use crate::repositories::models::RulesDefinition;
use crate::repositories::rules::rules_definition_id;
use crate::services::deps::q;
use crate::services::quest::catalog::definition_key;
use crate::services::rules::{evaluate, validate};

impl super::ConsoleService {
    pub async fn list_rules(&self, state: &ApiState) -> Result<Json<Vec<RuleRow>>, V2Error> {
        let rows = q(state).await?.list_rules().await?;
        Ok(Json(rows.into_iter().map(RuleRow::from).collect()))
    }

    pub async fn rule_versions(
        &self,
        state: &ApiState,
        key: &str,
    ) -> Result<Json<Vec<RuleRow>>, V2Error> {
        let rows = q(state).await?.list_rule_versions(key).await?;
        if rows.is_empty() {
            return Err(V2Error::not_found(format!("No rule named '{key}' exists.")));
        }
        Ok(Json(rows.into_iter().map(RuleRow::from).collect()))
    }

    /// Appends a new draft version of one rule set. An empty body clones the
    /// newest stored body, so a bump is one keystroke in the editor.
    pub async fn create_rule(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        body: RuleRequest,
    ) -> Result<Json<RuleRow>, V2Error> {
        claims.require_write()?;
        let key = definition_key(&body.key, "rule")?;
        let queries = q(state).await?;

        let body_value = match body.body {
            Some(body) => body,
            None => {
                let latest = queries.latest_rule_version(&key).await?;
                if latest == 0 {
                    return Err(V2Error::invalid_argument(
                        "A new rule needs a body to validate.",
                    ));
                }
                queries
                    .get_rule(&key, latest)
                    .await?
                    .map(|row| row.body)
                    .unwrap_or(Value::Null)
            }
        };
        if let Err(err) = validate(&body_value) {
            return Err(V2Error::invalid_argument(format!(
                "Invalid rule body at {err}"
            )));
        }

        let version = queries.latest_rule_version(&key).await? + 1;
        let draft = RulesDefinition {
            id: rules_definition_id(&key, version),
            game_id: crate::services::games::tenant::game_id(state).await?,
            key: key.clone(),
            version,
            sha256: hex::encode(Sha256::digest(body_value.to_string().as_bytes())),
            is_active: false,
            body: body_value,
            create_time: Utc::now(),
        };
        if !queries.create_rule(&draft).await? {
            return Err(V2Error::already_exists("That rule version already exists."));
        }
        super::audit(
            state,
            claims,
            "rule.create",
            "rule",
            None,
            &format!("Drafted rule {key} version {version}."),
        )
        .await;
        Ok(Json(RuleRow::from(draft)))
    }

    /// Publish = mark one stored version active; older versions are kept so a
    /// publish is always reversible.
    pub async fn publish_rule(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        key: &str,
        version: i64,
    ) -> Result<Json<RuleRow>, V2Error> {
        claims.require_write()?;
        let queries = q(state).await?;
        if queries.get_rule(key, version).await?.is_none() {
            return Err(V2Error::not_found(format!(
                "Rule {key} version {version} not found."
            )));
        }
        queries.activate_rule(key, version).await?;
        let row = queries
            .get_rule(key, version)
            .await?
            .expect("the version existed a moment ago");
        super::audit(
            state,
            claims,
            "rule.publish",
            "rule",
            None,
            &format!("Published rule {key} version {version}."),
        )
        .await;
        Ok(Json(RuleRow::from(row)))
    }

    pub async fn delete_rule(
        &self,
        state: &ApiState,
        claims: &ConsoleAuthed,
        key: &str,
    ) -> Result<Json<serde_json::Value>, V2Error> {
        claims.require_write()?;
        let removed = q(state)
            .await?
            .delete_rule_versions(key)
            .await?
            .rows_affected();
        if removed == 0 {
            return Err(V2Error::not_found(format!("No rule named '{key}' exists.")));
        }
        super::audit(
            state,
            claims,
            "rule.delete",
            "rule",
            None,
            &format!("Deleted rule {key} and its {removed} version(s)."),
        )
        .await;
        Ok(Json(serde_json::json!({ "deleted": removed })))
    }

    /// Dry-runs a rule (stored by key or inline body) against a player's stat
    /// map, or against a context supplied inline.
    pub async fn dry_run_rule(
        &self,
        state: &ApiState,
        body: RuleDryRun,
    ) -> Result<Json<RuleDryRunResult>, V2Error> {
        let rule_body = if let Some(inline) = body.body {
            Some(inline)
        } else {
            match body.key.as_deref() {
                Some(key) if !key.trim().is_empty() => q(state)
                    .await?
                    .active_rule(key.trim())
                    .await?
                    .map(|row| row.body),
                _ => None,
            }
        };
        let Some(rule_body) = rule_body else {
            return Err(V2Error::invalid_argument(
                "A dry run needs a rule key or an inline body.",
            ));
        };

        let context = match body.context {
            Some(context) => context,
            None => match body.user_id {
                Some(user_id) => q(state).await?.player_stat_context(user_id).await?,
                None => Value::Object(Default::default()),
            },
        };

        let (matched, error) =
            match validate(&rule_body).and_then(|_| evaluate(&rule_body, &context)) {
                Ok(matched) => (matched, None),
                Err(err) => (false, Some(err.to_string())),
            };
        Ok(Json(RuleDryRunResult {
            matched,
            evaluated_against: context,
            error,
        }))
    }
}
