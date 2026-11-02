use super::ast::Declaration;
use super::parser::Parser;
use super::token::TokenKind;
use crate::{SchemaDef, check_schema_name};

impl Parser {
    pub(super) fn parse_call(&mut self) {
        let keyword = self.advance();
        let schema_pos = self.pos - 1;
        let inferred_name = self.infer_const_name(schema_pos);
        self.advance();

        if !self.eat_symbol('{') {
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected '{' with schema fields, e.g. schema({ x: t.number() }, \"Player\")",
            );
            self.skip_balanced('(', ')');
            return;
        }

        let Some((fields, maps)) = self.parse_schema_fields(&keyword) else {
            return;
        };

        let mut name = None;
        let mut extends = Vec::new();
        let mut version = 1u32;
        if self.eat_symbol(',') {
            if let TokenKind::String(value) = self.current().kind.clone() {
                name = Some(value);
                self.advance();
            } else if self.current().kind == TokenKind::Symbol(',') {
            } else {
                let token = self.current().clone();
                self.skip_expression();
                self.error(
                    token.line,
                    token.column,
                    "schema() needs a string name as the second argument, e.g. schema({...}, \"Player\")",
                );
            }
            if self.eat_symbol(',') {
                let token = self.current().clone();
                match self.current().kind.clone() {
                    TokenKind::Ident(base) => {
                        self.advance();
                        extends.push(base);
                    }
                    _ => {
                        self.skip_expression();
                        self.error(
                            token.line,
                            token.column,
                            "the third schema() argument must be a base schema name, e.g. schema({...}, \"Child\", Base)",
                        );
                    }
                }
            }
        }

        self.eat_symbol(')');

        self.parse_option_chain(&mut extends, &mut version);

        let name = match name.or(inferred_name) {
            Some(name) => name,
            None => {
                self.error(
                    keyword.line,
                    keyword.column,
                    "schema needs a name; pass it as `schema({...}, \"Player\")` or assign it to `const Player = schema({...})`",
                );
                return;
            }
        };

        if fields.is_empty() && maps.is_empty() && extends.is_empty() {
            self.error(
                keyword.line,
                keyword.column,
                format!("schema '{name}' must declare at least one field"),
            );
        }
        check_schema_name(&name, keyword.line, keyword.column, &mut self.diagnostics);

        self.declarations.push(Declaration {
            schema: SchemaDef {
                name,
                version,
                fields,
                line: keyword.line,
                column: keyword.column,
            },
            maps,
            extends,
        });
    }
}
