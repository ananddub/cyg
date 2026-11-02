use super::lex::Lexer;
use super::token::{Token, TokenKind};
use crate::SchemaDiagnostic;

impl Lexer<'_> {
    pub(super) fn read_block_comment(&mut self) -> Result<(), SchemaDiagnostic> {
        let start_line = self.line;
        let start_column = self.column;
        self.i += 2;
        self.column += 2;
        let mut closed = false;
        while self.i < self.chars.len() {
            if self.chars[self.i] == '*' && self.chars.get(self.i + 1) == Some(&'/') {
                self.i += 2;
                self.column += 2;
                closed = true;
                break;
            }
            if self.chars[self.i] == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.i += 1;
        }
        if !closed {
            return Err(SchemaDiagnostic {
                line: start_line,
                column: start_column,
                message: "unterminated block comment".into(),
            });
        }
        Ok(())
    }

    pub(super) fn read_string(&mut self) -> Result<(), SchemaDiagnostic> {
        let quote = self.chars[self.i];
        let start_line = self.line;
        let start_column = self.column;
        self.i += 1;
        self.column += 1;
        let mut value = String::new();
        let mut closed = false;
        while self.i < self.chars.len() {
            let ch = self.chars[self.i];
            if ch == '\\' {
                if let Some(escaped) = self.chars.get(self.i + 1).copied() {
                    value.push(match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        other => other,
                    });
                    self.i += 2;
                    self.column += 2;
                    continue;
                }
            }
            if ch == quote {
                self.i += 1;
                self.column += 1;
                closed = true;
                break;
            }
            if ch == '\n' {
                if quote != '`' {
                    break;
                }
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            value.push(ch);
            self.i += 1;
        }
        if !closed {
            return Err(SchemaDiagnostic {
                line: start_line,
                column: start_column,
                message: "unterminated string literal".into(),
            });
        }
        self.tokens.push(Token {
            kind: TokenKind::String(value),
            line: start_line,
            column: start_column,
        });
        Ok(())
    }
}
