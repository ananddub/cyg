use super::ast::{ParsedPrimitive, ParsedType};
use super::parser::Parser;
use super::primitive::primitive_kind;
use super::token::TokenKind;

impl Parser {
    pub(super) fn parse_type(&mut self, field_name: &str) -> Option<ParsedType> {
        let start = self.current().clone();
        if !self.is_ident("t") {
            // A bare identifier names another declared schema: `stats: Stats`.
            // The field is flattened into the parent row by `parse_schema_fields`.
            if let TokenKind::Ident(target) = start.kind.clone() {
                self.advance();
                return Some(ParsedType::Nested(target));
            }
            self.error(
                start.line,
                start.column,
                format!("field '{field_name}' must use a `t.*` type or a declared schema name"),
            );
            self.skip_expression();
            return None;
        }
        self.advance();
        if !self.eat_symbol('.') {
            self.error(
                start.line,
                start.column,
                "expected '.' after `t`, e.g. t.number()",
            );
            return None;
        }
        let type_token = self.current().clone();
        let type_name = match &type_token.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => {
                self.error(
                    type_token.line,
                    type_token.column,
                    "expected a type name after `t.`",
                );
                return None;
            }
        };
        self.advance();

        if type_name == "map" {
            return self.parse_map_type(&type_token);
        }

        if type_name == "ref" {
            return self.parse_ref_type(&type_token);
        }

        if type_name == "array" {
            return self.parse_array_type(&type_token);
        }

        let kind = primitive_kind(&type_name);
        if self.eat_symbol('(') {
            self.skip_balanced('(', ')');
        }
        if let Some(kind) = kind {
            return Some(ParsedType::Primitive(ParsedPrimitive {
                kind,
                element: None,
                ref_target: None,
                source_type: format!("t.{type_name}"),
            }));
        }

        let message = match type_name.as_str() {
            "set" | "collection" | "stream" => {
                "collections are not supported inside an OXS2 schema; use t.array(t.u32()) for flat values or t.map(Entity) for a root collection"
            }
            "uint64" | "biguint64" | "bigint64" => {
                "64-bit unsigned/BigInt fields are not supported by OXS2 state sync yet; use int64 or split the value"
            }
            "quantized" | "angle" => {
                "t.quantized()/t.angle() are not supported by OXS2 state sync yet"
            }
            _ => {
                "unknown schema type; use t.number, t.float32, t.int8/16/32/64, t.uint8/16/32, t.boolean, t.string, t.array(t.*) or t.ref"
            }
        };
        self.error(type_token.line, type_token.column, message);
        None
    }
}
