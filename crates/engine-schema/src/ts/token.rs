#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TokenKind {
    Ident(String),
    Number(String),
    String(String),
    Symbol(char),
    Eof,
}

#[derive(Clone, Debug)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) line: usize,
    pub(super) column: usize,
}
