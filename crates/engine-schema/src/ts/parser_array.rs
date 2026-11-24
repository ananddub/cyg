use super::ast::{ParsedPrimitive, ParsedType};
use super::parser::Parser;
use super::primitive::primitive_kind;
use super::token::{Token, TokenKind};
use crate::WireKind;

impl Parser {
    pub(super) fn parse_array_type(&mut self, type_token: &Token) -> Option<ParsedType> {
        if !self.eat_symbol('(') {
            self.error(
                type_token.line,
                type_token.column,
                "t.array(Element) needs an element type, e.g. t.array(t.uint32())",
            );
            return None;
        }
        let inner = self.current().clone();
        let element = if self.is_ident("t") {
            self.advance();
            if !self.eat_symbol('.') {
                self.error(
                    inner.line,
                    inner.column,
                    "expected '.' after `t`, e.g. t.array(t.u32())",
                );
                self.skip_balanced('(', ')');
                return None;
            }
            let elem_token = self.current().clone();
            let elem_name = match &elem_token.kind {
                TokenKind::Ident(name) => name.clone(),
                _ => {
                    self.error(
                        elem_token.line,
                        elem_token.column,
                        "expected an array element type after `t.`",
                    );
                    self.skip_balanced('(', ')');
                    return None;
                }
            };
            self.advance();
            let elem_kind = if elem_name == "ref" {
                if self.eat_symbol('(') {
                    if matches!(self.current().kind, TokenKind::Ident(_)) {
                        self.advance();
                    }
                    if !self.eat_symbol(')') {
                        self.skip_balanced('(', ')');
                    }
                }
                Some(WireKind::U32)
            } else {
                if self.eat_symbol('(') {
                    self.skip_balanced('(', ')');
                }
                primitive_kind(&elem_name)
            };
            match elem_kind {
                Some(WireKind::I64) => {
                    self.error(
                        elem_token.line,
                        elem_token.column,
                        "t.array(t.int64()) loses precision; use t.array(t.int32()) or split the value",
                    );
                    self.skip_balanced('(', ')');
                    return None;
                }
                Some(WireKind::String) | Some(WireKind::Array) | None => {
                    self.error(
                        elem_token.line,
                        elem_token.column,
                        "array elements must be numeric or boolean primitives, e.g. t.array(t.uint32())",
                    );
                    self.skip_balanced('(', ')');
                    return None;
                }
                Some(kind) => kind,
            }
        } else if let TokenKind::Ident(target) = inner.kind.clone() {
            // `t.array(Item)` names a declared schema: it becomes a child
            // collection keyed by owner + slot, not a scalar wire array.
            self.advance();
            if !self.eat_symbol(')') {
                let token = self.current().clone();
                self.error(
                    token.line,
                    token.column,
                    "expected ')' after the nested array schema name",
                );
                self.skip_balanced('(', ')');
            }
            return Some(ParsedType::NestedArray(target));
        } else {
            self.error(
                inner.line,
                inner.column,
                "t.array(...) expects a t.* element type or a declared schema, e.g. t.array(Item)",
            );
            self.skip_balanced('(', ')');
            return None;
        };
        if !self.eat_symbol(')') {
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected ')' after the t.array element type",
            );
            self.skip_balanced('(', ')');
        }
        Some(ParsedType::Primitive(ParsedPrimitive {
            kind: WireKind::Array,
            element: Some(element),
            ref_target: None,
            source_type: format!("t.array({})", element.builder_method()),
        }))
    }
}
