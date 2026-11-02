use super::ast::Literal;
use super::parser::Parser;
use super::token::TokenKind;

impl Parser {
    pub(super) fn parse_default_literal(&mut self) -> Option<Literal> {
        if self.peek_symbol(0, '(')
            && self.peek_symbol(1, ')')
            && self.peek_symbol(2, '=')
            && matches!(
                self.tokens.get(self.pos + 3).map(|token| &token.kind),
                Some(TokenKind::Symbol('>'))
            )
        {
            self.pos += 4;
            return self.parse_literal();
        }
        self.parse_literal()
    }

    pub(super) fn parse_literal(&mut self) -> Option<Literal> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Number(value) => {
                self.advance();
                Some(Literal::Number(value.replace('_', "")))
            }
            TokenKind::Ident(value) if value == "true" || value == "false" => {
                self.advance();
                Some(Literal::Bool(value == "true"))
            }
            TokenKind::String(value) => {
                self.advance();
                Some(Literal::Str(value.clone()))
            }
            TokenKind::Symbol('-') => {
                self.advance();
                if let TokenKind::Number(value) = self.current().kind.clone() {
                    self.advance();
                    Some(Literal::Number(format!("-{}", value.replace('_', ""))))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
