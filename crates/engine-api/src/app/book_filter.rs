//! The book's uniform `filter` query (`API Design Patterns`, ch. 22).
//!
//! A list route already has its own query parameters; `filter` is the one
//! standard expression every list accepts. The grammar is deliberately small:
//!
//! ```text
//! filter=status=active AND score>=100
//! filter=(tier=gold OR tier=silver) AND name~"a b"
//! ```
//!
//! It is evaluated against the JSON representation the route already produced,
//! so a new resource gets a working filter without a second field map.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Compare {
        path: Vec<String>,
        op: Op,
        value: Literal,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Text(String),
    Number(f64),
    Bool(bool),
    Null,
}

pub fn parse(raw: &str) -> Result<Expr, String> {
    let mut parser = Parser {
        chars: raw.chars().collect(),
        at: 0,
    };
    let expr = parser.expression()?;
    parser.whitespace();
    if parser.at != parser.chars.len() {
        return Err(format!(
            "unexpected `{}` after the filter expression",
            parser.rest().trim()
        ));
    }
    Ok(expr)
}

pub fn matches(expr: &Expr, value: &Value) -> bool {
    match expr {
        Expr::And(left, right) => matches(left, value) && matches(right, value),
        Expr::Or(left, right) => matches(left, value) || matches(right, value),
        Expr::Compare {
            path,
            op,
            value: literal,
        } => {
            let Some(found) = resolve(value, path) else {
                return false;
            };
            compare(found, *op, literal)
        }
    }
}

fn resolve<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(part)?;
    }
    Some(current)
}

fn compare(found: &Value, op: Op, literal: &Literal) -> bool {
    match op {
        Op::Eq => equal(found, literal),
        Op::Ne => !equal(found, literal),
        Op::Contains => {
            let Literal::Text(needle) = literal else {
                return false;
            };
            let haystack = match found {
                Value::String(text) => text,
                Value::Array(values) => {
                    return values.iter().any(|value| {
                        value.as_str().is_some_and(|text| {
                            text.to_lowercase().contains(&needle.to_lowercase())
                        })
                    });
                }
                _ => return false,
            };
            haystack.to_lowercase().contains(&needle.to_lowercase())
        }
        Op::Gt | Op::Ge | Op::Lt | Op::Le => {
            let Some(ordering) = ordering(found, literal) else {
                return false;
            };
            match op {
                Op::Gt => ordering > 0.0,
                Op::Ge => ordering >= 0.0,
                Op::Lt => ordering < 0.0,
                Op::Le => ordering <= 0.0,
                _ => unreachable!(),
            }
        }
    }
}

fn equal(found: &Value, literal: &Literal) -> bool {
    match (found, literal) {
        (Value::String(left), Literal::Text(right)) => left == right,
        (Value::Number(left), Literal::Number(right)) => {
            left.as_f64().is_some_and(|left| left == *right)
        }
        (Value::Bool(left), Literal::Bool(right)) => left == right,
        (Value::Null, Literal::Null) => true,
        _ => false,
    }
}

fn ordering(found: &Value, literal: &Literal) -> Option<f64> {
    match (found, literal) {
        (Value::Number(left), Literal::Number(right)) => Some(left.as_f64()? - right),
        (Value::String(left), Literal::Text(right)) => Some(if left < right {
            -1.0
        } else if left > right {
            1.0
        } else {
            0.0
        }),
        _ => None,
    }
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn expression(&mut self) -> Result<Expr, String> {
        let mut left = self.term()?;
        loop {
            self.whitespace();
            if self.take_word("OR") {
                let right = self.term()?;
                left = Expr::Or(Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut left = self.factor()?;
        loop {
            self.whitespace();
            if self.take_word("AND") {
                let right = self.factor()?;
                left = Expr::And(Box::new(left), Box::new(right));
            } else {
                return Ok(left);
            }
        }
    }

    fn factor(&mut self) -> Result<Expr, String> {
        self.whitespace();
        if self.peek() == Some('(') {
            self.at += 1;
            let expr = self.expression()?;
            self.whitespace();
            if self.peek() != Some(')') {
                return Err("a filter group is missing `)`".into());
            }
            self.at += 1;
            return Ok(expr);
        }
        self.comparison()
    }

    fn comparison(&mut self) -> Result<Expr, String> {
        self.whitespace();
        let path = self.path()?;
        self.whitespace();
        let op = self.op()?;
        self.whitespace();
        let value = self.literal()?;
        Ok(Expr::Compare { path, op, value })
    }

    fn path(&mut self) -> Result<Vec<String>, String> {
        let mut parts = vec![self.ident()?];
        while self.peek() == Some('.') {
            self.at += 1;
            parts.push(self.ident()?);
        }
        Ok(parts)
    }

    fn ident(&mut self) -> Result<String, String> {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                self.at += 1;
            } else {
                break;
            }
        }
        if start == self.at {
            return Err(format!(
                "expected a field name near `{}`",
                self.rest().trim()
            ));
        }
        Ok(self.chars[start..self.at].iter().collect())
    }

    fn op(&mut self) -> Result<Op, String> {
        for (text, op) in [
            (">=", Op::Ge),
            ("<=", Op::Le),
            ("!=", Op::Ne),
            ("=", Op::Eq),
            (">", Op::Gt),
            ("<", Op::Lt),
            ("~", Op::Contains),
        ] {
            if self.rest().starts_with(text) {
                self.at += text.chars().count();
                return Ok(op);
            }
        }
        Err(format!(
            "expected one of = != >= <= > < ~ near `{}`",
            self.rest().trim()
        ))
    }

    fn literal(&mut self) -> Result<Literal, String> {
        if self.peek() == Some('"') || self.peek() == Some('\'') {
            let quote = self.peek().expect("checked");
            self.at += 1;
            let mut text = String::new();
            while let Some(c) = self.peek() {
                self.at += 1;
                if c == '\\' {
                    if let Some(next) = self.peek() {
                        text.push(next);
                        self.at += 1;
                    }
                } else if c == quote {
                    return Ok(Literal::Text(text));
                } else {
                    text.push(c);
                }
            }
            return Err("a quoted filter value is not closed".into());
        }
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ')' {
                break;
            }
            self.at += 1;
        }
        let raw: String = self.chars[start..self.at].iter().collect();
        if raw.eq_ignore_ascii_case("null") {
            return Ok(Literal::Null);
        }
        if raw.eq_ignore_ascii_case("true") {
            return Ok(Literal::Bool(true));
        }
        if raw.eq_ignore_ascii_case("false") {
            return Ok(Literal::Bool(false));
        }
        if let Ok(number) = raw.parse::<f64>() {
            return Ok(Literal::Number(number));
        }
        if raw.is_empty() {
            return Err("a comparison is missing its value".into());
        }
        Ok(Literal::Text(raw))
    }

    fn take_word(&mut self, word: &str) -> bool {
        let rest = self.rest();
        let Some(prefix) = rest.get(..word.len()) else {
            return false;
        };
        if !prefix.eq_ignore_ascii_case(word) {
            return false;
        }
        if rest[word.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            return false;
        }
        self.at += word.len();
        true
    }

    fn whitespace(&mut self) -> usize {
        let start = self.at;
        while self.peek().is_some_and(char::is_whitespace) {
            self.at += 1;
        }
        self.at - start
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn rest(&self) -> String {
        self.chars[self.at..].iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn and_or_and_comparisons_work() {
        let expr = parse("status=active AND (score>=100 OR tier=gold)").unwrap();
        assert!(matches(
            &expr,
            &json!({"status":"active","score":120,"tier":"bronze"})
        ));
        assert!(!matches(
            &expr,
            &json!({"status":"active","score":10,"tier":"bronze"})
        ));
    }

    #[test]
    fn quoted_contains_is_case_insensitive() {
        let expr = parse("name~\"rush\"").unwrap();
        assert!(matches(&expr, &json!({"name":"Coin RUSH"})));
        assert!(!matches(&expr, &json!({"name":"sky"})));
    }

    #[test]
    fn a_malformed_group_is_refused() {
        assert!(parse("(a=1").is_err());
        assert!(parse("a").is_err());
        assert!(parse("a=1 AND").is_err());
    }
}
