use super::ast::Declaration;
use super::token::{Token, TokenKind};
use crate::SchemaDiagnostic;

pub(super) struct Parser {
    pub(super) tokens: Vec<Token>,
    pub(super) pos: usize,
    pub(super) diagnostics: Vec<SchemaDiagnostic>,
    pub(super) declarations: Vec<Declaration>,
}

impl Parser {
    pub(super) fn parse_all(&mut self) {
        while !self.at_eof() {
            if self.is_ident("schema") && self.peek_symbol(1, '(') {
                self.parse_call();
            } else {
                self.advance();
            }
        }
    }

    pub(super) fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    pub(super) fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    pub(super) fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    pub(super) fn is_ident(&self, text: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(name) if name == text)
    }

    pub(super) fn peek_symbol(&self, offset: usize, symbol: char) -> bool {
        matches!(
            self.tokens.get(self.pos + offset).map(|token| &token.kind),
            Some(TokenKind::Symbol(found)) if *found == symbol
        )
    }

    pub(super) fn eat_symbol(&mut self, symbol: char) -> bool {
        if self.current().kind == TokenKind::Symbol(symbol) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn error(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.diagnostics.push(SchemaDiagnostic {
            line,
            column,
            message: message.into(),
        });
    }

    pub(super) fn infer_const_name(&self, schema_pos: usize) -> Option<String> {
        if schema_pos < 3 {
            return None;
        }
        if self.tokens[schema_pos - 1].kind != TokenKind::Symbol('=') {
            return None;
        }
        let name = match &self.tokens[schema_pos - 2].kind {
            TokenKind::Ident(name) => name.clone(),
            _ => return None,
        };
        match &self.tokens[schema_pos - 3].kind {
            TokenKind::Ident(keyword)
                if keyword == "const" || keyword == "let" || keyword == "var" =>
            {
                Some(name)
            }
            _ => None,
        }
    }
}
