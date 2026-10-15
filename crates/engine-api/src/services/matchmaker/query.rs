//! the `+properties.k:v` / `>= <= > <` / `-` query term language.

use std::collections::HashMap;

/// the property term language: `+properties.k:v` include-equal,
/// `+properties.k:>=n` include-compare, `-properties.k:v` exclude. Empty matches all.
pub fn query_matches(
    query: &str,
    strings: &HashMap<String, String>,
    numerics: &HashMap<String, f64>,
) -> bool {
    for term in query.split_whitespace() {
        let Some(colon) = term.find(':') else {
            continue;
        };
        let (lhs, value) = (&term[..colon], &term[colon + 1..]);
        let negated = lhs.starts_with('-');
        let key = lhs
            .trim_start_matches(['+', '-'])
            .strip_prefix("properties.")
            .unwrap_or("");
        if value.is_empty() || key.is_empty() {
            continue;
        }
        if let Some(oppos) = value.find(|c| matches!(c, '>' | '<')) {
            let (cmp_op, rhs) = split_operator(&value[oppos..]);
            let Ok(target) = rhs.trim().parse::<f64>() else {
                continue;
            };
            let Some(actual) = numerics
                .get(key)
                .copied()
                .or_else(|| strings.get(key).and_then(|v| v.parse().ok()))
            else {
                if negated {
                    return false;
                }
                continue;
            };
            let ok = match cmp_op {
                ">=" => actual >= target,
                "<=" => actual <= target,
                ">" => actual > target,
                "<" => actual < target,
                "==" => actual == target,
                _ => actual == target,
            };
            if negated {
                if ok {
                    return false;
                }
            } else if !ok {
                return false;
            }
        } else {
            let actual_string = strings.get(key).cloned();
            let actual_numeric = numerics.get(key).map(|n| n.to_string());
            let equal =
                actual_string.as_deref() == Some(value) || actual_numeric.as_deref() == Some(value);
            if negated {
                if equal {
                    return false;
                }
            } else if !equal {
                return false;
            }
        }
    }
    true
}

fn split_operator(s: &str) -> (&str, &str) {
    if s.starts_with(">=") {
        (">=", &s[2..])
    } else if s.starts_with("<=") {
        ("<=", &s[2..])
    } else if s.starts_with('>') {
        (">", &s[1..])
    } else if s.starts_with('<') {
        ("<", &s[1..])
    } else {
        ("==", s)
    }
}
