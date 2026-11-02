use super::ast::ParsedType;
use super::default::validate_default;
use super::parser::Parser;
use super::token::TokenKind;
use crate::{FIELD_OPTIONAL, FIELD_STATIC, FIELD_TRANSIENT, WireKind};

impl Parser {
    pub(super) fn parse_modifiers(
        &mut self,
        field_name: &str,
        parsed: Option<&ParsedType>,
        default: &mut Option<String>,
        flags: &mut u8,
    ) {
        while self.eat_symbol('.') {
            let token = self.current().clone();
            let modifier = match &token.kind {
                TokenKind::Ident(name) => name.clone(),
                _ => {
                    self.error(token.line, token.column, "expected a modifier name");
                    return;
                }
            };
            self.advance();
            if !self.eat_symbol('(') {
                self.error(
                    token.line,
                    token.column,
                    format!("expected '(' after .{modifier}()"),
                );
                return;
            }

            match modifier.as_str() {
                "default" => {
                    let literal_token = self.current().clone();
                    let literal = self.parse_default_literal();
                    if !self.eat_symbol(')') {
                        self.skip_balanced('(', ')');
                    }
                    let primitive = match parsed {
                        Some(ParsedType::Primitive(primitive)) => Some(primitive),
                        _ => None,
                    };
                    match (primitive, literal) {
                        (Some(primitive), Some(_literal)) if primitive.kind == WireKind::Array => {
                            self.error(
                                literal_token.line,
                                literal_token.column,
                                format!(
                                    ".default() is not supported on array field '{field_name}'; assign the array in the constructor instead"
                                ),
                            );
                        }
                        (Some(primitive), Some(literal)) => {
                            if let Some(normalized) = validate_default(primitive.kind, &literal) {
                                *default = Some(normalized);
                            } else {
                                self.error(
                                    literal_token.line,
                                    literal_token.column,
                                    format!(
                                        "default value for field '{field_name}' is out of range or has the wrong type"
                                    ),
                                );
                            }
                        }
                        _ => {
                            self.error(
                                literal_token.line,
                                literal_token.column,
                                format!(
                                    ".default() is only supported on primitive and string fields (field '{field_name}')"
                                ),
                            );
                        }
                    }
                }
                "optional" | "static" | "transient" => {
                    if !self.eat_symbol(')') {
                        self.skip_balanced('(', ')');
                        self.error(
                            token.line,
                            token.column,
                            format!(".{modifier}() takes no arguments"),
                        );
                    }
                    *flags |= match modifier.as_str() {
                        "optional" => FIELD_OPTIONAL,
                        "static" => FIELD_STATIC,
                        _ => FIELD_TRANSIENT,
                    };
                }
                "version" => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        ".version() applies to the whole schema; use schema({...}, \"Name\").version(n)",
                    );
                }
                "view" | "validate" => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        format!(
                            ".{modifier}() is not supported by OXS2 state sync; encode the value directly or validate it in the room"
                        ),
                    );
                }
                _ => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        format!("unknown schema modifier .{modifier}()"),
                    );
                }
            }
        }
    }
}
