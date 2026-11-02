use super::token::{Token, TokenKind};
use crate::SchemaDiagnostic;

pub(super) fn lex(source: &str) -> Result<Vec<Token>, SchemaDiagnostic> {
    let chars: Vec<char> = source.chars().collect();
    let mut lexer = Lexer {
        chars: &chars,
        i: 0,
        line: 1,
        column: 1,
        tokens: Vec::new(),
    };
    lexer.run()?;
    Ok(lexer.tokens)
}

pub(super) struct Lexer<'a> {
    pub(super) chars: &'a [char],
    pub(super) i: usize,
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) tokens: Vec<Token>,
}

impl Lexer<'_> {
    pub(super) fn run(&mut self) -> Result<(), SchemaDiagnostic> {
        while self.i < self.chars.len() {
            let c = self.chars[self.i];
            if c == '\n' {
                self.i += 1;
                self.line += 1;
                self.column = 1;
                continue;
            }
            if c.is_whitespace() {
                self.i += 1;
                self.column += 1;
                continue;
            }
            if c == '/' && self.chars.get(self.i + 1) == Some(&'/') {
                self.read_line_comment();
                continue;
            }
            if c == '/' && self.chars.get(self.i + 1) == Some(&'*') {
                self.read_block_comment()?;
                continue;
            }
            if c == '"' || c == '\'' || c == '`' {
                self.read_string()?;
                continue;
            }
            if c == '_' || c == '$' || c.is_alphabetic() {
                self.read_ident();
                continue;
            }
            if c.is_ascii_digit() {
                self.read_number();
                continue;
            }
            self.tokens.push(Token {
                kind: TokenKind::Symbol(c),
                line: self.line,
                column: self.column,
            });
            self.i += 1;
            self.column += 1;
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.line,
            column: self.column,
        });
        Ok(())
    }

    pub(super) fn read_line_comment(&mut self) {
        while self.i < self.chars.len() && self.chars[self.i] != '\n' {
            self.i += 1;
            self.column += 1;
        }
    }

    pub(super) fn read_ident(&mut self) {
        let start = self.i;
        let start_column = self.column;
        while self.i < self.chars.len()
            && (self.chars[self.i] == '_'
                || self.chars[self.i] == '$'
                || self.chars[self.i].is_alphanumeric())
        {
            self.i += 1;
            self.column += 1;
        }
        self.tokens.push(Token {
            kind: TokenKind::Ident(self.chars[start..self.i].iter().collect()),
            line: self.line,
            column: start_column,
        });
    }

    pub(super) fn read_number(&mut self) {
        let start = self.i;
        let start_column = self.column;
        while self.i < self.chars.len()
            && (self.chars[self.i].is_ascii_digit()
                || self.chars[self.i] == '_'
                || self.chars[self.i] == '.'
                || self.chars[self.i] == 'e'
                || self.chars[self.i] == 'E'
                || ((self.chars[self.i] == '+' || self.chars[self.i] == '-')
                    && matches!(self.chars.get(self.i - 1), Some('e' | 'E'))))
        {
            self.i += 1;
            self.column += 1;
        }
        self.tokens.push(Token {
            kind: TokenKind::Number(self.chars[start..self.i].iter().collect()),
            line: self.line,
            column: start_column,
        });
    }
}
