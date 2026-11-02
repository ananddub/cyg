use super::parser::Parser;
use super::token::TokenKind;

impl Parser {
    pub(super) fn skip_to_field_separator(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.current().kind {
                TokenKind::Symbol('(') | TokenKind::Symbol('{') | TokenKind::Symbol('[') => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Symbol(')') | TokenKind::Symbol('}') | TokenKind::Symbol(']') => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Symbol(',') if depth == 0 => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    pub(super) fn skip_expression(&mut self) {
        let mut depth = 0usize;
        while !self.at_eof() {
            match self.current().kind {
                TokenKind::Symbol('(') | TokenKind::Symbol('{') | TokenKind::Symbol('[') => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Symbol(')') | TokenKind::Symbol('}') | TokenKind::Symbol(']') => {
                    if depth == 0 {
                        return;
                    }
                    depth -= 1;
                    self.advance();
                }
                TokenKind::Symbol(',') if depth == 0 => return,
                _ => {
                    self.advance();
                }
            }
        }
    }

    pub(super) fn skip_balanced(&mut self, _open: char, _close: char) {
        let mut depth = 1usize;
        while !self.at_eof() {
            match self.current().kind {
                TokenKind::Symbol('(') | TokenKind::Symbol('{') | TokenKind::Symbol('[') => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::Symbol(')') | TokenKind::Symbol('}') | TokenKind::Symbol(']') => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 {
                        return;
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
    }
}
