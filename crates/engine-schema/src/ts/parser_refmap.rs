use super::ast::{ParsedPrimitive, ParsedType};
use super::parser::Parser;
use super::token::{Token, TokenKind};
use crate::WireKind;

impl Parser {
    pub(super) fn parse_map_type(&mut self, type_token: &Token) -> Option<ParsedType> {
        if !self.eat_symbol('(') {
            self.error(
                type_token.line,
                type_token.column,
                "t.map(Target) needs a target schema",
            );
            return None;
        }
        let target_token = self.current().clone();
        let target = match &target_token.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => {
                self.error(
                    target_token.line,
                    target_token.column,
                    "expected a schema name inside t.map(...)",
                );
                self.skip_balanced('(', ')');
                return None;
            }
        };
        self.advance();
        if !self.eat_symbol(')') {
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected ')' after the t.map target schema",
            );
            self.skip_balanced('(', ')');
        }
        Some(ParsedType::Map(target))
    }

    pub(super) fn parse_ref_type(&mut self, type_token: &Token) -> Option<ParsedType> {
        if !self.eat_symbol('(') {
            self.error(
                type_token.line,
                type_token.column,
                "t.ref(Target) needs a target schema",
            );
            return None;
        }
        let target_token = self.current().clone();
        let target = match &target_token.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => {
                self.error(
                    target_token.line,
                    target_token.column,
                    "expected a schema name inside t.ref(...)",
                );
                self.skip_balanced('(', ')');
                return None;
            }
        };
        self.advance();
        if !self.eat_symbol(')') {
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected ')' after the t.ref target schema",
            );
            self.skip_balanced('(', ')');
        }
        Some(ParsedType::Primitive(ParsedPrimitive {
            kind: WireKind::U32,
            element: None,
            ref_target: Some(target.clone()),
            source_type: format!("t.ref({target})"),
        }))
    }
}
