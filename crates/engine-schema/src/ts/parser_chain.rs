use super::parser::Parser;
use super::token::TokenKind;

impl Parser {
    pub(super) fn parse_option_chain(&mut self, extends: &mut Vec<String>, version: &mut u32) {
        loop {
            if !self.peek_symbol(0, '.') {
                break;
            }
            let option_token = self.tokens.get(self.pos + 1).cloned();
            let option = match option_token.as_ref().map(|token| &token.kind) {
                Some(TokenKind::Ident(name)) if name == "extend" || name == "version" => {
                    name.clone()
                }
                _ => break,
            };
            let token = option_token.unwrap();
            self.advance();
            self.advance();
            if !self.eat_symbol('(') {
                self.error(
                    token.line,
                    token.column,
                    format!("expected '(' after .{option}()"),
                );
                break;
            }
            match option.as_str() {
                "extend" => match self.current().kind.clone() {
                    TokenKind::Ident(base) => {
                        self.advance();
                        extends.push(base);
                    }
                    _ => {
                        let current = self.current().clone();
                        self.error(
                            current.line,
                            current.column,
                            ".extend(Base) expects a schema name",
                        );
                        self.skip_balanced('(', ')');
                        continue;
                    }
                },
                "version" => match self.current().kind.clone() {
                    TokenKind::Number(raw) => {
                        let number = self.advance();
                        let cleaned = raw.replace('_', "");
                        match cleaned.parse::<u64>() {
                            Ok(value) if value > 0 && value <= u32::MAX as u64 => {
                                *version = value as u32
                            }
                            Ok(0) => self.error(
                                number.line,
                                number.column,
                                "schema version must be at least 1",
                            ),
                            Ok(_) => self.error(
                                number.line,
                                number.column,
                                "schema version does not fit in a 32-bit unsigned integer",
                            ),
                            Err(_) => {
                                self.error(number.line, number.column, "invalid schema version")
                            }
                        }
                    }
                    _ => {
                        let current = self.current().clone();
                        self.error(
                            current.line,
                            current.column,
                            ".version(n) expects a positive integer",
                        );
                    }
                },
                _ => unreachable!(),
            }
            if !self.eat_symbol(')') {
                let current = self.current().clone();
                self.error(
                    current.line,
                    current.column,
                    format!("expected ')' after .{option}(...)"),
                );
                self.skip_balanced('(', ')');
            }
        }
    }
}
