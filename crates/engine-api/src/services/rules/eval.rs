//! A rule is a small typed expression tree stored as JSON:
//!
//! ```json
//! { "all": [ { "field": "kills", "op": ">=", "value": 10 },
//!            { "any": [ { "field": "rank", "op": "eq", "value": "gold" },
//!                       { "field": "rank", "op": "eq", "value": "platinum" } ] } ] }
//! ```
//!
//! Evaluation walks the tree against a JSON context (a player's stat map); a
//! missing field is simply false for comparisons rather than an error, so a
//! rule keeps working for players who have never set a stat.

use serde_json::Value;

mod compare;
use compare::{compare, lookup};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    /// The dotted path inside the rule body the problem sits at, `all[2].op`.
    pub path: String,
    pub message: String,
}

impl RuleError {
    fn at(path: &str, message: impl Into<String>) -> Self {
        Self {
            path: path.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

const OPERATORS: [&str; 8] = ["eq", "ne", "gt", "gte", "lt", "lte", "in", "contains"];

/// Checks the whole body without evaluating it, so an operator console can
/// refuse a rule at write time with the exact field error.
pub fn validate(body: &Value) -> Result<(), RuleError> {
    parse(body, "").map(|_| ())
}

/// Evaluates the rule against `context` (a JSON object, usually a player's
/// stat map). A rule body that was written without validation still fails
/// here with the same field error instead of panicking.
pub fn evaluate(body: &Value, context: &Value) -> Result<bool, RuleError> {
    let expr = parse(body, "")?;
    Ok(expr.matches(context))
}

#[derive(Debug)]
enum Expr {
    All(Vec<Expr>),
    Any(Vec<Expr>),
    Not(Box<Expr>),
    Cmp {
        field: String,
        op: String,
        value: Value,
    },
    Truth(bool),
}

impl Expr {
    fn matches(&self, context: &Value) -> bool {
        match self {
            Expr::All(children) => children.iter().all(|child| child.matches(context)),
            Expr::Any(children) => children.iter().any(|child| child.matches(context)),
            Expr::Not(child) => !child.matches(context),
            Expr::Truth(value) => *value,
            Expr::Cmp { field, op, value } => match lookup(context, field) {
                // A field the context never set is not null — only `ne` can
                // succeed against it, every other comparison is false.
                None => op == "ne",
                Some(actual) => compare(actual, op, value),
            },
        }
    }
}

fn parse(body: &Value, path: &str) -> Result<Expr, RuleError> {
    let object = body
        .as_object()
        .ok_or_else(|| RuleError::at(path, "a rule is an object with one of all/any/not/field"))?;
    let groups = ["all", "any", "not", "field"]
        .iter()
        .filter(|key| object.contains_key(**key))
        .count();
    if groups > 1 {
        return Err(RuleError::at(
            path,
            "a rule holds exactly one of all/any/not/field",
        ));
    }
    if let Some(children) = object.get("all") {
        return Ok(Expr::All(parse_children(children, path, "all")?));
    }
    if let Some(children) = object.get("any") {
        return Ok(Expr::Any(parse_children(children, path, "any")?));
    }
    if let Some(inner) = object.get("not") {
        return Ok(Expr::Not(Box::new(parse(inner, &child_path(path, "not"))?)));
    }
    if let Some(field) = object.get("field") {
        return parse_comparison(object, field, path);
    }
    if object.get("value") == Some(&Value::Bool(true)) && object.len() == 1 {
        return Ok(Expr::Truth(true));
    }
    Err(RuleError::at(
        if path.is_empty() { "body" } else { path },
        "a rule needs one of all/any/not or a field comparison",
    ))
}

fn parse_children(children: &Value, path: &str, group: &str) -> Result<Vec<Expr>, RuleError> {
    let items = children.as_array().ok_or_else(|| {
        RuleError::at(&child_path(path, group), "a group holds an array of rules")
    })?;
    if items.is_empty() {
        return Err(RuleError::at(
            &child_path(path, group),
            "a group needs at least one rule",
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse(item, &format!("{}[{}]", child_path(path, group), index)))
        .collect()
}

fn parse_comparison(
    object: &serde_json::Map<String, Value>,
    field: &Value,
    path: &str,
) -> Result<Expr, RuleError> {
    let field = field.as_str().ok_or_else(|| {
        RuleError::at(&child_path(path, "field"), "field is a dotted name string")
    })?;
    if field.trim().is_empty() {
        return Err(RuleError::at(&child_path(path, "field"), "field is empty"));
    }
    let op = object
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| RuleError::at(&child_path(path, "op"), "op is one of the operator names"))?;
    if !OPERATORS.contains(&op) {
        return Err(RuleError::at(
            &child_path(path, "op"),
            format!("op is one of {}", OPERATORS.join(", ")),
        ));
    }
    let value = object
        .get("value")
        .ok_or_else(|| RuleError::at(&child_path(path, "value"), "a comparison needs a value"))?;
    if op == "in" && !value.is_array() {
        return Err(RuleError::at(
            &child_path(path, "value"),
            "in compares against an array of values",
        ));
    }
    Ok(Expr::Cmp {
        field: field.trim().to_owned(),
        op: op.to_owned(),
        value: value.clone(),
    })
}

fn child_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_owned()
    } else {
        format!("{path}.{key}")
    }
}

#[cfg(test)]
mod tests;
