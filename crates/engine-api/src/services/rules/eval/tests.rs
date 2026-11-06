use super::*;
use serde_json::json;

fn context() -> Value {
    json!({ "kills": 12, "rank": "gold", "wins": 3.5, "badges": ["sharpshooter", "veteran"] })
}

#[test]
fn comparison_operators_follow_their_truth_tables() {
    let ctx = context();
    let cases = [
        (json!({"field": "kills", "op": "gte", "value": 12}), true),
        (json!({"field": "kills", "op": "gt", "value": 12}), false),
        (json!({"field": "kills", "op": "lt", "value": 12.5}), true),
        (json!({"field": "kills", "op": "lte", "value": 12}), true),
        (json!({"field": "rank", "op": "eq", "value": "gold"}), true),
        (json!({"field": "rank", "op": "ne", "value": "gold"}), false),
        (
            json!({"field": "rank", "op": "lt", "value": "silver"}),
            true,
        ),
        (
            json!({"field": "missing", "op": "eq", "value": null}),
            false,
        ),
        (json!({"field": "missing", "op": "ne", "value": 1}), true),
        (
            json!({"field": "rank", "op": "in", "value": ["bronze", "gold"]}),
            true,
        ),
        (
            json!({"field": "rank", "op": "in", "value": ["bronze", "silver"]}),
            false,
        ),
        (
            json!({"field": "badges", "op": "contains", "value": "veteran"}),
            true,
        ),
        (
            json!({"field": "badges", "op": "contains", "value": "rookie"}),
            false,
        ),
    ];
    for (body, expected) in cases {
        assert_eq!(
            evaluate(&body, &ctx).expect("valid rule"),
            expected,
            "case {body}"
        );
    }
}

#[test]
fn groups_compose_and_not_negates() {
    let ctx = context();
    let all = json!({ "all": [
        {"field": "kills", "op": "gte", "value": 10},
        {"any": [
            {"field": "rank", "op": "eq", "value": "gold"},
            {"field": "rank", "op": "eq", "value": "platinum"}
        ]}
    ]});
    assert!(evaluate(&all, &ctx).unwrap());
    let not = json!({ "not": {"field": "kills", "op": "gte", "value": 100} });
    assert!(evaluate(&not, &ctx).unwrap());
    let failing = json!({ "all": [
        {"field": "kills", "op": "gte", "value": 10},
        {"field": "wins", "op": "gt", "value": 10}
    ]});
    assert!(!evaluate(&failing, &ctx).unwrap());
}

#[test]
fn invalid_bodies_fail_with_the_exact_field() {
    let err = validate(&json!({"field": "kills", "op": "between", "value": 1}))
        .expect_err("unknown op is refused");
    assert_eq!(err.path, "op");
    let err = validate(&json!({"field": "kills", "op": "in", "value": 3}))
        .expect_err("in needs an array");
    assert_eq!(err.path, "value");
    let err = validate(&json!({"all": []})).expect_err("empty group is refused");
    assert_eq!(err.path, "all");
    let err = validate(&json!({"not": {"field": "kills", "op": "eq", "value": 1}, "any": []}))
        .expect_err("not holds nothing else");
    assert!(err.message.contains("exactly one"));
    let err = validate(&json!("kills > 10")).expect_err("a bare string is not a rule");
    assert_eq!(err.path, "");
}

#[test]
fn a_valid_body_validates_cleanly() {
    let body = json!({ "all": [
        {"field": "kills", "op": "gte", "value": 1},
        {"not": {"field": "rank", "op": "contains", "value": "bot"}}
    ]});
    assert!(validate(&body).is_ok());
}
