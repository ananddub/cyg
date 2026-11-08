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
    validate_input_frame(&schemas, root.as_ref(), &mut diagnostics);
    validate_join_options(&schemas, root.as_ref(), &mut diagnostics);

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(TsSchema { schemas, root })
}

/// The reserved `JoinOptions` def types the join options. Strings are fine
/// here (the host encodes them length-prefixed), but the name may not double
/// as a state entity.
fn validate_join_options(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    diagnostics: &mut Vec<SchemaDiagnostic>,
) {
    use crate::JOIN_OPTIONS_SCHEMA_NAME;
    let Some(def) = schemas
        .iter()
        .find(|schema| schema.name == JOIN_OPTIONS_SCHEMA_NAME)
    else {
        return;
    };
    if let Some(root) = root {
        if root
            .collections
            .iter()
            .any(|collection| collection.schema == JOIN_OPTIONS_SCHEMA_NAME)
        {
            diagnostics.push(SchemaDiagnostic {
                line: def.line,
                column: def.column,
                message: format!(
                    "`{JOIN_OPTIONS_SCHEMA_NAME}` is reserved for the typed join options and cannot also be a state collection — rename the entity"
                ),
            });
        }
    }
    if def.fields.is_empty() {
        diagnostics.push(SchemaDiagnostic {
            line: def.line,
            column: def.column,
            message: format!("`{JOIN_OPTIONS_SCHEMA_NAME}` must declare at least one field"),
        });
    }
}

/// The reserved `Input` def is the per-tick input frame. Its field order is
/// the wire order and the client packs each field at its fixed size, so it
/// can only hold fixed-size kinds and cannot double as a state entity.
fn validate_input_frame(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    diagnostics: &mut Vec<SchemaDiagnostic>,
) {
    use crate::INPUT_SCHEMA_NAME;
    let Some(input) = schemas
        .iter()
        .find(|schema| schema.name == INPUT_SCHEMA_NAME)
    else {
        return;
    };
    if let Some(root) = root {
        if root
            .collections
            .iter()
            .any(|collection| collection.schema == crate::INPUT_SCHEMA_NAME)
        {
            diagnostics.push(SchemaDiagnostic {
                line: input.line,
                column: input.column,
                message: format!(
                    "`{INPUT_SCHEMA_NAME}` is reserved for the per-tick input frame and cannot also be a state collection — rename the entity"
                ),
            });
        }
    }
    for field in &input.fields {
        if matches!(
            field.kind,
            crate::wire::WireKind::String | crate::wire::WireKind::Array
        ) {
            diagnostics.push(SchemaDiagnostic {
                line: input.line,
                column: input.column,
                message: format!(
                    "input frame field `{}` must be numeric or boolean (fixed-size on the wire); strings and arrays are state-only",
                    field.name
                ),
            });
        }
    }
}
