use super::ast::Declaration;
use crate::{SchemaDef, SchemaDiagnostic, SchemaField};

pub(super) fn resolve_declarations(
    declarations: &[Declaration],
    diagnostics: &mut Vec<SchemaDiagnostic>,
) -> Vec<Declaration> {
    struct Resolver<'a> {
        declarations: &'a [Declaration],
        index: std::collections::HashMap<&'a str, usize>,
        state: Vec<u8>,
        resolved: Vec<Option<Declaration>>,
        diagnostics: &'a mut Vec<SchemaDiagnostic>,
    }

    impl Resolver<'_> {
        fn resolve(&mut self, index: usize) -> Option<Declaration> {
            match self.state[index] {
                2 => return self.resolved[index].clone(),
                1 => {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' inherits from itself (cycle)",
                            declaration.schema.name
                        ),
                    });
                    return None;
                }
                _ => {}
            }
            self.state[index] = 1;

            let extends = self.declarations[index].extends.clone();
            let own_fields = self.declarations[index].schema.fields.clone();
            let own_nested = self.declarations[index].schema.nested_arrays.clone();
            let maps = self.declarations[index].maps.clone();
            let mut fields: Vec<SchemaField> = Vec::new();
            let mut nested_arrays: Vec<crate::NestedArray> = Vec::new();

            for base_name in &extends {
                let Some(&base_index) = self.index.get(base_name.as_str()) else {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' extends unknown schema '{}'",
                            declaration.schema.name, base_name
                        ),
                    });
                    continue;
                };
                let Some(base) = self.resolve(base_index) else {
                    continue;
                };
                if !base.maps.is_empty() {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' cannot extend root schema '{}'",
                            declaration.schema.name, base_name
                        ),
                    });
                    continue;
                }
                for nested in base.schema.nested_arrays {
                    if nested_arrays.iter().any(|candidate| candidate.field == nested.field) {
                        self.diagnostics.push(SchemaDiagnostic {
                            line: self.declarations[index].schema.line,
                            column: self.declarations[index].schema.column,
                            message: format!(
                                "nested array '{}' is already inherited",
                                nested.field
                            ),
                        });
                    } else {
                        nested_arrays.push(nested);
                    }
                }
                for field in base.schema.fields {
                    if let Some(existing) =
                        fields.iter().find(|candidate| candidate.name == field.name)
                    {
                        self.diagnostics.push(SchemaDiagnostic {
                            line: field.line,
                            column: field.column,
                            message: format!(
                                "inherited field '{}' conflicts with a field inherited from {}:{}",
                                field.name, existing.line, existing.column
                            ),
                        });
                    } else {
                        fields.push(field);
                    }
                }
            }

            for nested in own_nested {
                if nested_arrays.iter().any(|candidate| candidate.field == nested.field) {
                    self.diagnostics.push(SchemaDiagnostic {
                        line: self.declarations[index].schema.line,
                        column: self.declarations[index].schema.column,
                        message: format!(
                            "nested array '{}' conflicts with an inherited nested array",
                            nested.field
                        ),
                    });
                } else {
                    nested_arrays.push(nested);
                }
            }

            for field in own_fields {
                if let Some(existing) = fields.iter().find(|candidate| candidate.name == field.name)
                {
                    self.diagnostics.push(SchemaDiagnostic {
                        line: field.line,
                        column: field.column,
                        message: format!(
                            "field '{}' is already inherited from {}:{}",
                            field.name, existing.line, existing.column
                        ),
                    });
                } else {
                    fields.push(field);
                }
            }

            self.state[index] = 2;
            let schema = SchemaDef {
                name: self.declarations[index].schema.name.clone(),
                version: self.declarations[index].schema.version,
                fields,
                nested_arrays,
                line: self.declarations[index].schema.line,
                column: self.declarations[index].schema.column,
            };
            let resolved = Declaration {
                schema,
                maps,
                extends: Vec::new(),
            };
            self.resolved[index] = Some(resolved.clone());
            Some(resolved)
        }
    }

    let index = declarations
        .iter()
        .enumerate()
        .map(|(i, declaration)| (declaration.schema.name.as_str(), i))
        .collect();
    let mut resolver = Resolver {
        declarations,
        index,
        state: vec![0; declarations.len()],
        resolved: vec![None; declarations.len()],
        diagnostics,
    };
    let mut out = Vec::new();
    for i in 0..declarations.len() {
        if let Some(declaration) = resolver.resolve(i) {
            out.push(declaration);
        }
    }
    out
}
