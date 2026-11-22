use super::ast::ParsedType;
use super::default::validate_default;
use super::parser::Parser;
use super::token::TokenKind;
use crate::{
    FIELD_OPTIONAL, FIELD_PRIVATE, FIELD_STATIC, FIELD_TRANSIENT, FIELD_UNRELIABLE, FieldEncoding,
    WireKind,
};

impl Parser {
    pub(super) fn parse_modifiers(
        &mut self,
        field_name: &str,
        parsed: Option<&ParsedType>,
        default: &mut Option<String>,
        flags: &mut u8,
        encoding: &mut Option<FieldEncoding>,
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
                "range" => {
                    self.parse_range_modifier(field_name, parsed, encoding);
                }
                "optional" | "static" | "transient" | "private" | "unreliable" => {
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
                        "private" => FIELD_PRIVATE,
                        "unreliable" => FIELD_UNRELIABLE,
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

    /// `.range(min, max)` or `.range(min, max, bits)` — the float encoding on the bit-packed lane.
    ///
    /// Only meaningful for numeric fields: an integer already has an exact width, and a string has
    /// no range. The default width is 16 bits, which is the point where a range stops being a
    /// rounding trick and starts being a real decision — so a schema that cares says so.
    fn parse_range_modifier(
        &mut self,
        field_name: &str,
        parsed: Option<&ParsedType>,
        encoding: &mut Option<FieldEncoding>,
    ) {
        let line = self.current().line;
        let kind = match parsed {
            Some(ParsedType::Primitive(primitive)) => Some(primitive.kind),
            _ => None,
        };
        let numeric = matches!(
            kind,
            Some(
                WireKind::F32
                    | WireKind::F64
                    | WireKind::U8
                    | WireKind::I8
                    | WireKind::U16
                    | WireKind::I16
                    | WireKind::U32
                    | WireKind::I32
            )
        );
        if !numeric {
            self.skip_balanced('(', ')');
            self.error(
                line,
                self.current().column,
                format!(".range() needs a numeric field; '{field_name}' is not one"),
            );
            return;
        }

        let mut values: Vec<f32> = Vec::new();
        while !self.at_eof() && !self.peek_symbol(0, ')') {
            match self.parse_literal() {
                Some(super::ast::Literal::Number(text)) => match text.parse::<f32>() {
                    Ok(value) if value.is_finite() => values.push(value),
                    _ => {
                        self.skip_balanced('(', ')');
                        self.error(line, self.current().column, ".range() needs finite numbers");
                        return;
                    }
                },
                _ => {
                    self.skip_balanced('(', ')');
                    self.error(
                        line,
                        self.current().column,
                        ".range() takes .range(min, max) or .range(min, max, bits)",
                    );
                    return;
                }
            }
            if !self.eat_symbol(',') {
                break;
            }
        }
        if !self.eat_symbol(')') {
            self.skip_balanced('(', ')');
        }

        let bits = match values.len() {
            2 => 16,
            3 => match values[2] {
                b if b >= 1.0 && b <= 32.0 && b.fract() == 0.0 => b as u8,
                _ => {
                    self.error(line, self.current().column, ".range() bits must be 1..=32");
                    return;
                }
            },
            _ => {
                self.error(
                    line,
                    self.current().column,
                    ".range() takes .range(min, max) or .range(min, max, bits)",
                );
                return;
            }
        };
        if !(values[1] > values[0]) {
            self.error(
                line,
                self.current().column,
                ".range() needs max > min as its first two arguments",
            );
            return;
        }
        let mut encoding_out = FieldEncoding::range(bits, values[0], values[1]);
        if matches!(
            kind,
            Some(
                WireKind::Bool
                    | WireKind::U8
                    | WireKind::I8
                    | WireKind::U16
                    | WireKind::I16
                    | WireKind::U32
                    | WireKind::I32
            )
        ) {
            encoding_out = encoding_out.as_integer();
        }
        *encoding = Some(encoding_out);
    }
}
