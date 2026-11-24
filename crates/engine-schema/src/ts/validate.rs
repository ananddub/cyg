use super::primitive::is_map_member_name;
use crate::{MAX_FIELDS, RootState, SchemaDef, SchemaDiagnostic};

pub(super) fn validate(
    schemas: &[SchemaDef],
    root: Option<&RootState>,
    diagnostics: &mut Vec<SchemaDiagnostic>,
) {
    if schemas.is_empty() {
        diagnostics.push(SchemaDiagnostic {
            line: 1,
            column: 1,
            message:
                "no entity schema found; add `export const Player = schema({ x: t.number() }, \"Player\")`"
                    .into(),
        });
    } else {
        for schema in schemas {
            if schema.fields.len() > MAX_FIELDS {
                diagnostics.push(SchemaDiagnostic {
                    line: schema.line,
                    column: schema.column,
                    message: format!(
                        "schema '{}' has {} fields; the sync wire format allows at most {MAX_FIELDS}",
                        schema.name,
                        schema.fields.len()
                    ),
                });
            }
            for field in &schema.fields {
                if let Some(target) = &field.ref_target {
                    if schemas.iter().all(|candidate| candidate.name != *target) {
                        diagnostics.push(SchemaDiagnostic {
                            line: field.line,
                            column: field.column,
                            message: format!(
                                "field '{}' references unknown schema '{}'",
                                field.name, target
                            ),
                        });
                    }
                }
            }
        }
        if let Some(root) = root {
            for field in &root.scalars {
                if let Some(target) = &field.ref_target {
                    if schemas.iter().all(|candidate| candidate.name != *target) {
                        diagnostics.push(SchemaDiagnostic {
                            line: field.line,
                            column: field.column,
                            message: format!(
                                "root field '{}' references unknown schema '{}'",
                                field.name, target
                            ),
                        });
                    }
                }
            }
        }
        if let Some(root) = root {
            if root.name == "GameState" {
                diagnostics.push(SchemaDiagnostic {
                    line: 1,
                    column: 1,
                    message: "the root schema cannot be named 'GameState'; that name is reserved for the generated world class".into(),
                });
            }
            if schemas.iter().any(|schema| schema.name == root.name) {
                diagnostics.push(SchemaDiagnostic {
                    line: 1,
                    column: 1,
                    message: format!(
                        "the root schema '{}' conflicts with an entity schema of the same name",
                        root.name
                    ),
                });
            }
            if schemas.iter().any(|schema| schema.name == "State") {
                diagnostics.push(SchemaDiagnostic {
                    line: 1,
                    column: 1,
                    message: "an entity schema cannot be named 'State'; it is reserved for the root collection".into(),
                });
            }
            for field in &root.scalars {
                if is_map_member_name(&field.name) {
                    diagnostics.push(SchemaDiagnostic {
                        line: field.line,
                        column: field.column,
                        message: format!(
                            "root state field '{}' shadows a Map member; rename it",
                            field.name
                        ),
                    });
                }
            }
            for collection in &root.collections {
                if is_map_member_name(&collection.name) {
                    diagnostics.push(SchemaDiagnostic {
                        line: 1,
                        column: 1,
                        message: format!(
                            "root collection '{}' shadows a Map member; rename it",
                            collection.name
                        ),
                    });
                }
                if schemas
                    .iter()
                    .all(|schema| schema.name != collection.schema)
                {
                    diagnostics.push(SchemaDiagnostic {
                        line: 1,
                        column: 1,
                        message: format!(
                            "state collection '{}' references unknown schema '{}'",
                            collection.name, collection.schema
                        ),
                    });
                }
            }
        }
    }
}
