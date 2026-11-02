mod ast;
mod default;
mod lex;
mod lex_scan;
mod parser;
mod parser_array;
mod parser_call;
mod parser_chain;
mod parser_fields;
mod parser_literal;
mod parser_modifiers;
mod parser_refmap;
mod parser_skip;
mod parser_type;
mod primitive;
mod resolve;
mod token;
mod validate;

#[cfg(test)]
mod tests;

use crate::{
    MAX_COLLECTIONS, RootCollection, RootState, SchemaDef, SchemaDiagnostic, check_unique_names,
};
use lex::lex;
use parser::Parser;
use resolve::resolve_declarations;
use validate::validate;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsSchema {
    pub schemas: Vec<SchemaDef>,
    pub root: Option<RootState>,
}

pub fn parse(source: &str) -> Result<TsSchema, Vec<SchemaDiagnostic>> {
    let tokens = lex(source).map_err(|diagnostic| vec![diagnostic])?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
        declarations: Vec::new(),
    };
    parser.parse_all();

    let mut diagnostics = std::mem::take(&mut parser.diagnostics);
    if diagnostics.is_empty() {
        let raw: Vec<SchemaDef> = parser
            .declarations
            .iter()
            .map(|declaration| declaration.schema.clone())
            .collect();
        check_unique_names(&raw, &mut diagnostics);
    }

    let declarations = if diagnostics.is_empty() {
        resolve_declarations(&parser.declarations, &mut diagnostics)
    } else {
        Vec::new()
    };

    let mut schemas = Vec::new();
    let mut root: Option<RootState> = None;
    for declaration in &declarations {
        if declaration.maps.is_empty() {
            schemas.push(declaration.schema.clone());
            continue;
        }

        if root.is_some() {
            diagnostics.push(SchemaDiagnostic {
                line: declaration.schema.line,
                column: declaration.schema.column,
                message: "only one root `State` schema is supported".into(),
            });
            continue;
        }
        if declaration.maps.len() > MAX_COLLECTIONS {
            diagnostics.push(SchemaDiagnostic {
                line: declaration.schema.line,
                column: declaration.schema.column,
                message: format!(
                    "root state declares {} collections; the sync wire format allows at most {MAX_COLLECTIONS}",
                    declaration.maps.len()
                ),
            });
            continue;
        }

        root = Some(RootState {
            name: declaration.schema.name.clone(),
            version: declaration.schema.version,
            scalars: declaration.schema.fields.clone(),
            collections: declaration
                .maps
                .iter()
                .map(|(name, target)| RootCollection {
                    name: name.clone(),
                    schema: target.clone(),
                })
                .collect(),
        });
    }

    if diagnostics.is_empty() {
        validate(&schemas, root.as_ref(), &mut diagnostics);
    }

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(TsSchema { schemas, root })
}
