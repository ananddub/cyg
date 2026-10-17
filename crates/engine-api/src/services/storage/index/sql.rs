//! The per-page statement: the permission gate, the key filter, every parsed term,
//! the sort, and the window the cursor asked for — assembled as data, not SQL.

use sqlx::Arguments;
use sqlx::postgres::PgArguments;

use crate::error::V2Error;

use super::IndexRow;
use super::parse::{Bound, Term};
use super::statements;

const ORDER_MESSAGE: &str = "This index cannot sort results by the field named in the order.";

/// A resolved sort: a column of the row, or a key of the object value.
pub(super) enum Sort {
    Column(&'static str),
    ValueField(String),
}

/// `-field` sorts descending; a plain name or a `value.`-prefixed one is a sortable
/// field of the index; anything the index never declared for sorting is refused.
pub(super) fn resolve_order(def: &IndexRow, order: &[String]) -> Result<Vec<(Sort, bool)>, V2Error> {
    let sortable = IndexRow::strings(&def.sortable_fields);
    let mut sorts = Vec::with_capacity(order.len());
    for raw in order {
        let (name, descending) = match raw.strip_prefix('-') {
            Some(rest) => (rest, true),
            None => (raw.as_str(), false),
        };
        let field = name.strip_prefix("value.").unwrap_or(name);
        sorts.push(match statements::sort_column(field) {
            Some(column) => (Sort::Column(column), descending),
            None if sortable.iter().any(|declared| declared == field) => {
                (Sort::ValueField(field.to_owned()), descending)
            }
            None => return Err(V2Error::invalid_argument(ORDER_MESSAGE)),
        });
    }
    Ok(sorts)
}

pub(super) fn build_query(
    def: &IndexRow,
    terms: &[Term],
    sorts: &[(Sort, bool)],
    caller: uuid::Uuid,
    page: i64,
    offset: i64,
    args: &mut PgArguments,
) -> String {
    let mut sql = String::from(statements::INDEX_READ_HEAD);
    if def.index_only {
        sql.push_str(
            "COALESCE((SELECT jsonb_object_agg(e.k, e.v) FROM jsonb_each(s.value) e(k, v) WHERE e.k = ANY(",
        );
        bind(IndexRow::strings(&def.fields), args, &mut sql);
        sql.push_str(")), '{}'::jsonb)::text AS value");
    } else {
        sql.push_str("s.value::text AS value");
    }
    sql.push_str(statements::INDEX_READ_TAIL);

    bind(def.collection.clone(), args, &mut sql);
    sql.push_str(" AND (s.read = 2 OR (s.read = 1 AND s.user_id = ");
    bind(caller, args, &mut sql);
    sql.push_str("))");
    if !def.key.is_empty() {
        sql.push_str(" AND s.key = ");
        bind(def.key.clone(), args, &mut sql);
    }
    for term in terms {
        sql.push_str(" AND (");
        term_sql(term, args, &mut sql);
        sql.push(')');
    }

    sql.push_str(" ORDER BY ");
    for (index, (sort, descending)) in sorts.iter().enumerate() {
        if index > 0 {
            sql.push_str(", ");
        }
        let direction = if *descending { " DESC" } else { " ASC" };
        match sort {
            Sort::Column(column) => sql.push_str(&format!("s.{column}{direction}")),
            Sort::ValueField(field) => {
                sql.push_str("CASE WHEN jsonb_typeof(s.value -> ");
                bind(field.clone(), args, &mut sql);
                sql.push_str(") = 'number' THEN (s.value ->> ");
                bind(field.clone(), args, &mut sql);
                sql.push_str(")::numeric ELSE NULL END");
                push_direction(direction, &mut sql);
                sql.push_str(", CASE WHEN jsonb_typeof(s.value -> ");
                bind(field.clone(), args, &mut sql);
                sql.push_str(") IN ('string', 'boolean') THEN s.value ->> ");
                bind(field.clone(), args, &mut sql);
                sql.push_str(" ELSE NULL END");
                push_direction(direction, &mut sql);
            }
        }
    }
    if !sorts.is_empty() {
        sql.push_str(", ");
    }
    sql.push_str("s.collection ASC, s.key ASC, s.user_id ASC");

    sql.push_str(" LIMIT ");
    bind(page, args, &mut sql);
    sql.push_str(" OFFSET ");
    bind(offset, args, &mut sql);
    sql
}

fn push_direction(direction: &str, sql: &mut String) {
    sql.push_str(direction);
    sql.push_str(" NULLS LAST");
}

/// One term as SQL: text equality is a `->>` compare; numeric equality and every
/// range ride a `CASE` so a non-number in the column never reaches a cast.
fn term_sql(term: &Term, args: &mut PgArguments, sql: &mut String) {
    match term {
        Term::Eq { field, text, number } => {
            sql.push_str("(s.value ->> ");
            bind(field.clone(), args, sql);
            sql.push_str(" = ");
            bind(text.clone(), args, sql);
            if let Some(number) = number {
                sql.push_str(" OR ");
                numeric_of(field, args, sql);
                sql.push_str(" = ");
                bind(number.clone(), args, sql);
                sql.push_str("::numeric");
            }
            sql.push(')');
        }
        Term::Range { field, lower, upper } => {
            let bounds: Vec<(&Bound, bool)> = lower
                .iter()
                .map(|b| (b, true))
                .chain(upper.iter().map(|b| (b, false)))
                .collect();
            let numeric = bounds.iter().all(|(b, _)| b.number.is_some());
            if numeric {
                sql.push_str("jsonb_typeof(s.value -> ");
                bind(field.clone(), args, sql);
                sql.push_str(") = 'number'");
                for (bound, lower_side) in bounds {
                    sql.push_str(" AND ");
                    numeric_of(field, args, sql);
                    operator(bound, lower_side, sql);
                    bind(bound.number.clone().unwrap_or_default(), args, sql);
                    sql.push_str("::numeric");
                }
            } else {
                for (index, (bound, lower_side)) in bounds.iter().enumerate() {
                    if index > 0 {
                        sql.push_str(" AND ");
                    }
                    sql.push_str("s.value ->> ");
                    bind(field.clone(), args, sql);
                    operator(bound, *lower_side, sql);
                    bind(bound.text.clone(), args, sql);
                }
            }
        }
    }
}

/// Push one owned value as the next `$n` placeholder.
fn bind<T>(value: T, args: &mut PgArguments, sql: &mut String)
where
    T: for<'q> sqlx::Encode<'q, sqlx::postgres::Postgres>
        + sqlx::Type<sqlx::postgres::Postgres>
        + Send,
{
    let index = args.len() + 1;
    sqlx::Arguments::add(args, value).expect("bindable index parameter");
    sql.push('$');
    sql.push_str(&index.to_string());
}

fn numeric_of(field: &str, args: &mut PgArguments, sql: &mut String) {
    sql.push_str("CASE WHEN jsonb_typeof(s.value -> ");
    bind(field.to_owned(), args, sql);
    sql.push_str(") = 'number' THEN (s.value ->> ");
    bind(field.to_owned(), args, sql);
    sql.push_str(")::numeric ELSE NULL END");
}

fn operator(bound: &Bound, lower: bool, sql: &mut String) {
    sql.push_str(match (lower, bound.inclusive) {
        (true, true) => " >= ",
        (true, false) => " > ",
        (false, true) => " <= ",
        (false, false) => " < ",
    });
}
