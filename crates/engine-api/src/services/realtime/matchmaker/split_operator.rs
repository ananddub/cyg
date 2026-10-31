use super::*;

impl RealtimeMatchmakerService {
    fn split_operator<'a>(&self, s: &'a str) -> (&'a str, &'a str) {
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
    pub fn compile_query(&self, query: &str) -> Vec<CompiledTerm> {
        let mut terms = Vec::new();
        for term in query.split_whitespace() {
            let Some(colon) = term.find(':') else {
                continue;
            };
            let (lhs, value) = (&term[..colon], &term[colon + 1..]);
            let negated = lhs.starts_with('-');
            let key = lhs
                .trim_start_matches(['+', '-'])
                .strip_prefix("properties.")
                .unwrap_or("")
                .to_string();
            if value.is_empty() || key.is_empty() {
                continue;
            }
            if let Some(oppos) = value.find(|c| matches!(c, '>' | '<')) {
                let (cmp_op, rhs) = self.split_operator(&value[oppos..]);
                let Ok(target) = rhs.trim().parse::<f64>() else {
                    continue;
                };
                let cmp = match cmp_op {
                    ">=" => NumericCmp::Gte(target),
                    "<=" => NumericCmp::Lte(target),
                    ">" => NumericCmp::Gt(target),
                    "<" => NumericCmp::Lt(target),
                    _ => NumericCmp::Eq(target),
                };
                terms.push(CompiledTerm {
                    key,
                    negated,
                    matcher: TermMatcher::Numeric(cmp),
                });
            } else {
                terms.push(CompiledTerm {
                    key,
                    negated,
                    matcher: TermMatcher::Exact(value.to_string()),
                });
            }
        }
        terms
    }
    pub fn is_compatible(
        &self,
        terms: &[CompiledTerm],
        strings: &HashMap<String, String>,
        numerics: &HashMap<String, f64>,
    ) -> bool {
        for term in terms {
            match &term.matcher {
                TermMatcher::Numeric(cmp) => {
                    let actual = numerics
                        .get(&term.key)
                        .copied()
                        .or_else(|| strings.get(&term.key).and_then(|v| v.parse::<f64>().ok()));
                    let Some(val) = actual else {
                        if term.negated {
                            return false;
                        }
                        continue;
                    };
                    let ok = match cmp {
                        NumericCmp::Gte(target) => val >= *target,
                        NumericCmp::Lte(target) => val <= *target,
                        NumericCmp::Gt(target) => val > *target,
                        NumericCmp::Lt(target) => val < *target,
                        NumericCmp::Eq(target) => val == *target,
                    };
                    if term.negated {
                        if ok {
                            return false;
                        }
                    } else if !ok {
                        return false;
                    }
                }
                TermMatcher::Exact(target_str) => {
                    let equal = strings.get(&term.key).map(|s| s.as_str())
                        == Some(target_str.as_str())
                        || numerics.get(&term.key).map(|n| n.to_string()).as_deref()
                            == Some(target_str.as_str());
                    if term.negated {
                        if equal {
                            return false;
                        }
                    } else if !equal {
                        return false;
                    }
                }
            }
        }
        true
    }
    pub fn query_matches(
        &self,
        query: &str,
        strings: &HashMap<String, String>,
        numerics: &HashMap<String, f64>,
    ) -> bool {
        let terms = self.compile_query(query);
        self.is_compatible(&terms, strings, numerics)
    }
    pub(super) fn spawn_tick(&self) {
        TICKER_STARTED.get_or_init(|| {
            tokio::spawn(async {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    interval.tick().await;
                    Self::global().try_match();
                }
            });
        });
    }
}
