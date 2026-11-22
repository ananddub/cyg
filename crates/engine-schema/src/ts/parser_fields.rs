use super::ast::ParsedType;
use super::parser::Parser;
use super::token::{Token, TokenKind};
use crate::{
    SchemaField, default_encoding as engine_schema_default_encoding, is_reserved_field_name,
};

impl Parser {
    pub(super) fn parse_schema_fields(
        &mut self,
        keyword: &Token,
    ) -> Option<(Vec<SchemaField>, Vec<(String, String)>, Vec<crate::NestedArray>)> {
        let mut fields = Vec::new();
        let mut maps = Vec::new();
        let mut nested_arrays = Vec::new();
        loop {
            if self.at_eof() {
                self.error(keyword.line, keyword.column, "unterminated schema object");
                return None;
            }
            if self.eat_symbol('}') {
                break;
            }

            let key_token = self.current().clone();
            let field_name = match &key_token.kind {
                TokenKind::Ident(name) | TokenKind::String(name) => name.clone(),
                _ => {
                    self.error(
                        key_token.line,
                        key_token.column,
                        "expected a schema field name",
                    );
                    self.skip_to_field_separator();
                    if self.eat_symbol(',') {
                        continue;
                    }
                    self.eat_symbol('}');
                    break;
                }
            };
            self.advance();

            if !self.eat_symbol(':') {
                let token = self.current().clone();
                self.error(
                    token.line,
                    token.column,
                    format!("expected ':' after schema field '{field_name}'"),
                );
                self.skip_to_field_separator();
                if self.eat_symbol(',') {
                    continue;
                }
                self.eat_symbol('}');
                break;
            }

            let parsed = self.parse_type(&field_name);
            let mut default = None;
            let mut flags = 0u8;
            let mut encoding = None;
            self.parse_modifiers(
                &field_name,
                parsed.as_ref(),
                &mut default,
                &mut flags,
                &mut encoding,
            );

            if let Some(parsed) = parsed {
                let reserved = is_reserved_field_name(&field_name);
                let duplicate = fields
                    .iter()
                    .any(|field: &SchemaField| field.name == field_name)
                    || maps.iter().any(|(name, _)| *name == field_name)
                    || nested_arrays
                        .iter()
                        .any(|nested: &crate::NestedArray| nested.field == field_name);
                if reserved {
                    self.error(
                        key_token.line,
                        key_token.column,
                        format!(
                            "schema field name '{field_name}' is reserved by the generated code"
                        ),
                    );
                } else if duplicate {
                    self.error(
                        key_token.line,
                        key_token.column,
                        format!("schema declares field '{field_name}' more than once"),
                    );
                } else {
                    match parsed {
                        ParsedType::Map(target) => maps.push((field_name, target)),
                        ParsedType::Nested(target) => {
                            let definition = self
                                .declarations
                                .iter()
                                .find(|decl| decl.schema.name == target)
                                .cloned();
                            let Some(definition) = definition else {
                                self.error(
                                    key_token.line,
                                    key_token.column,
                                    format!(
                                        "nested schema '{target}' must be declared before field '{field_name}'"
                                    ),
                                );
                                if self.eat_symbol(',') { continue; }
                                if self.eat_symbol('}') { break; }
                                continue;
                            };
                            for child in definition.schema.fields {
                                let name = format!("{field_name}__{}", child.name);
                                if fields.iter().any(|f: &SchemaField| f.name == name) {
                                    self.error(
                                        key_token.line,
                                        key_token.column,
                                        format!("nested field '{name}' collides with another field"),
                                    );
                                    continue;
                                }
                                let mut field = child;
                                field.name = name;
                                field.nested_path = Some(field_name.clone());
                                field.source_type = format!("{target}.{}", field.source_type);
                                field.line = key_token.line;
                                field.column = key_token.column;
                                fields.push(field);
                            }
                        }
                        ParsedType::NestedArray(target) => {
                            nested_arrays.push(crate::NestedArray {
                                field: field_name,
                                schema: target,
                            });
                        }
                        ParsedType::Primitive(primitive) => fields.push(SchemaField {
                            name: field_name,
                            kind: primitive.kind,
                            element: primitive.element,
                            ref_target: primitive.ref_target,
                            flags,
                            source_type: primitive.source_type,
                            default,
                            encoding: encoding
                                .unwrap_or_else(|| engine_schema_default_encoding(primitive.kind)),
                            nested_path: None,
                            line: key_token.line,
                            column: key_token.column,
                        }),
                    }
                }
            }

            if self.eat_symbol(',') {
                continue;
            }
            if self.eat_symbol('}') {
                break;
            }
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected ',' or '}' after schema field",
            );
            self.skip_to_field_separator();
            if self.eat_symbol(',') {
                continue;
            }
            self.eat_symbol('}');
            break;
        }
        Some((fields, maps, nested_arrays))
    }
}
