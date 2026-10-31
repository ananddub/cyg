use crate::error::V2Error;
use crate::repositories::storage::{StorageIndexBound as Bound, StorageIndexTerm as Term};

pub(super) const PARSE_MESSAGE: &str = "Failed to parse the index query.";

fn as_number(text: &str) -> Option<String> {
    text.parse::<f64>().ok().map(|n| n.to_string())
}

pub(super) fn parse(query: &str) -> Result<Vec<Term>, V2Error> {
    let mut terms = Vec::new();
    for token in split_terms(query) {
        terms.push(parse_term(token)?);
    }
    Ok(terms)
}

fn split_terms(query: &str) -> Vec<&str> {
    let bytes = query.as_bytes();
    let mut tokens = Vec::new();
    let (mut start, mut depth) = (0usize, 0usize);
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'[' | b'{' => depth += 1,
            b']' | b'}' => depth = depth.saturating_sub(1),
            whitespace if whitespace.is_ascii_whitespace() && depth == 0 => {
                if index > start {
                    tokens.push(&query[start..index]);
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    if start < bytes.len() {
        tokens.push(&query[start..]);
    }
    tokens
}

fn parse_term(token: &str) -> Result<Term, V2Error> {
    let (field, value) = token
        .split_once(':')
        .filter(|(f, _)| !f.is_empty())
        .ok_or_else(|| V2Error::invalid_argument(PARSE_MESSAGE))?;

    let bytes: &[u8] = value.as_bytes();
    let opened = matches!(bytes.first(), Some(b'[' | b'{'));
    let closed = matches!(bytes.last(), Some(b']' | b'}'));
    if opened || closed {
        if !(opened && closed) {
            return Err(V2Error::invalid_argument(PARSE_MESSAGE));
        }
        return parse_range(field, value);
    }
    Ok(Term::Eq {
        field: field.to_owned(),
        text: value.to_owned(),
        number: as_number(value),
    })
}

fn parse_range(field: &str, value: &str) -> Result<Term, V2Error> {
    let lower_inclusive = value.starts_with('[');
    let upper_inclusive = value.ends_with(']');
    let body = &value[1..value.len() - 1];
    let (low, high) = if let Some(rest) = body.strip_prefix("TO ") {
        ("", rest)
    } else if let Some(rest) = body.strip_suffix(" TO") {
        (rest, "")
    } else {
        let (low, high) = body
            .split_once(" TO ")
            .ok_or_else(|| V2Error::invalid_argument(PARSE_MESSAGE))?;
        (low, high)
    };

    let bound = |text: &str, inclusive: bool| -> Option<Bound> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(Bound {
            text: text.to_owned(),
            number: as_number(text),
            inclusive,
        })
    };
    let both_empty =
        |lower: &Option<Bound>, upper: &Option<Bound>| lower.is_none() && upper.is_none();

    let lower = bound(low, lower_inclusive);
    let upper = bound(high, upper_inclusive);
    if both_empty(&lower, &upper) {
        return Err(V2Error::invalid_argument(PARSE_MESSAGE));
    }
    Ok(Term::Range {
        field: field.to_owned(),
        lower,
        upper,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_terms_read_both_as_text_and_as_number() {
        let terms = parse("level:12 name:sword").unwrap();
        assert_eq!(terms.len(), 2);
        match &terms[0] {
            Term::Eq {
                field,
                text,
                number,
            } => {
                assert_eq!((field.as_str(), text.as_str()), ("level", "12"));
                assert_eq!(number.as_deref(), Some("12"));
            }
            _ => panic!("expected equality"),
        }
        match &terms[1] {
            Term::Eq { number, .. } => assert!(number.is_none()),
            _ => panic!("expected equality"),
        }
    }

    #[test]
    fn ranges_keep_their_exclusivity_and_allow_one_sided_bounds() {
        let terms = parse("score:[10 TO 20}").unwrap();
        match &terms[0] {
            Term::Range { lower, upper, .. } => {
                assert_eq!(lower.as_ref().unwrap().text, "10");
                assert!(lower.as_ref().unwrap().inclusive);
                assert_eq!(upper.as_ref().unwrap().text, "20");
                assert!(!upper.as_ref().unwrap().inclusive);
            }
            _ => panic!("expected range"),
        }

        let terms = parse("score:[TO 5]").unwrap();
        match &terms[0] {
            Term::Range { lower, upper, .. } => {
                assert!(lower.is_none());
                assert_eq!(upper.as_ref().unwrap().text, "5");
            }
            _ => panic!("expected range"),
        }
    }

    #[test]
    fn malformed_terms_are_refused_as_parse_failures() {
        for bad in ["nocolon", ":value", "field:[10 TO", "field:[]"] {
            assert!(parse(bad).is_err(), "{bad} should not parse");
        }
    }
}
