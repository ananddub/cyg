//! Parses the Colyseus-v5-style schema declaration used by `schema.ts`:
//!
//! ```ts
//! export const Player = schema({
//!   x: t.number(),
//!   y: t.number(),
//!   flags: t.uint32(),
//! }, "Player");
//!
//! export const State = schema({
//!   players: t.map(Player),
//! }, "State");
//! ```
//!
//! Only the `schema({...}, "Name")` call is read. The file is **not** compiled
//! into the room; `ox-game run` uses this parser to generate the typed
//! AssemblyScript entity class and the binary descriptor the host sends on
//! join. Other TypeScript around the calls is ignored.

use super::{
    FIELD_OPTIONAL, FIELD_STATIC, FIELD_TRANSIENT, MAX_COLLECTIONS, MAX_FIELDS, RootCollection,
    RootState, SchemaDef, SchemaDiagnostic, SchemaField, WireKind, check_schema_name,
    check_unique_names, is_reserved_field_name,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsSchema {
    pub schemas: Vec<SchemaDef>,
    pub root: Option<RootState>,
}

/// Parse every `schema({...}, "Name")` call in a TypeScript source file.
pub fn parse(source: &str) -> Result<TsSchema, Vec<SchemaDiagnostic>> {
    let tokens = lex(source).map_err(|diagnostic| vec![diagnostic])?;
    let mut parser = Parser {
        tokens,
        pos: 0,
        diagnostics: Vec::new(),
        declarations: Vec::new(),
    };
    parser.parse_all();

    let mut schemas = Vec::new();
    let mut root: Option<RootState> = None;

    if parser.diagnostics.is_empty() {
        let raw: Vec<SchemaDef> = parser
            .declarations
            .iter()
            .map(|declaration| declaration.schema.clone())
            .collect();
        check_unique_names(&raw, &mut parser.diagnostics);
    }

    // Inheritance is resolved after the whole file is read so a schema can
    // extend one declared later. Fields are flattened base-first.
    let declarations = if parser.diagnostics.is_empty() {
        resolve_declarations(&parser.declarations, &mut parser.diagnostics)
    } else {
        Vec::new()
    };

    for declaration in &declarations {
        if declaration.maps.is_empty() {
            schemas.push(declaration.schema.clone());
            continue;
        }

        if root.is_some() {
            parser.diagnostics.push(SchemaDiagnostic {
                line: declaration.schema.line,
                column: declaration.schema.column,
                message: "only one root `State` schema is supported".into(),
            });
            continue;
        }
        if declaration.maps.len() > MAX_COLLECTIONS {
            parser.diagnostics.push(SchemaDiagnostic {
                line: declaration.schema.line,
                column: declaration.schema.column,
                message: format!(
                    "root state declares {} collections; the sync wire format allows at most {MAX_COLLECTIONS}",
                    declaration.maps.len()
                ),
            });
            continue;
        }

        root = Some(RootState {
            name: declaration.schema.name.clone(),
            version: declaration.schema.version,
            scalars: declaration.schema.fields.clone(),
            collections: declaration
                .maps
                .iter()
                .map(|(name, target)| RootCollection {
                    name: name.clone(),
                    schema: target.clone(),
                })
                .collect(),
        });
    }

    if parser.diagnostics.is_empty() {
        if schemas.is_empty() {
            parser.diagnostics.push(SchemaDiagnostic {
                line: 1,
                column: 1,
                message:
                    "no entity schema found; add `export const Player = schema({ x: t.number() }, \"Player\")`"
                        .into(),
            });
        } else {
            for schema in &schemas {
                if schema.fields.len() > MAX_FIELDS {
                    parser.diagnostics.push(SchemaDiagnostic {
                        line: schema.line,
                        column: schema.column,
                        message: format!(
                            "schema '{}' has {} fields; the sync wire format allows at most {MAX_FIELDS}",
                            schema.name,
                            schema.fields.len()
                        ),
                    });
                }
                for field in &schema.fields {
                    if let Some(target) = &field.ref_target {
                        if schemas.iter().all(|candidate| candidate.name != *target) {
                            parser.diagnostics.push(SchemaDiagnostic {
                                line: field.line,
                                column: field.column,
                                message: format!(
                                    "field '{}' references unknown schema '{}'",
                                    field.name, target
                                ),
                            });
                        }
                    }
                }
            }
            if let Some(root) = &root {
                for field in &root.scalars {
                    if let Some(target) = &field.ref_target {
                        if schemas.iter().all(|candidate| candidate.name != *target) {
                            parser.diagnostics.push(SchemaDiagnostic {
                                line: field.line,
                                column: field.column,
                                message: format!(
                                    "root field '{}' references unknown schema '{}'",
                                    field.name, target
                                ),
                            });
                        }
                    }
                }
            }
            if let Some(root) = &root {
                if root.name == "GameState" {
                    parser.diagnostics.push(SchemaDiagnostic {
                        line: 1,
                        column: 1,
                        message: "the root schema cannot be named 'GameState'; that name is reserved for the generated world class".into(),
                    });
                }
                if schemas.iter().any(|schema| schema.name == root.name) {
                    parser.diagnostics.push(SchemaDiagnostic {
                        line: 1,
                        column: 1,
                        message: format!(
                            "the root schema '{}' conflicts with an entity schema of the same name",
                            root.name
                        ),
                    });
                }
                if schemas.iter().any(|schema| schema.name == "State") {
                    parser.diagnostics.push(SchemaDiagnostic {
                        line: 1,
                        column: 1,
                        message: "an entity schema cannot be named 'State'; it is reserved for the root collection".into(),
                    });
                }
                for field in &root.scalars {
                    if is_map_member_name(&field.name) {
                        parser.diagnostics.push(SchemaDiagnostic {
                            line: field.line,
                            column: field.column,
                            message: format!(
                                "root state field '{}' shadows a Map member; rename it",
                                field.name
                            ),
                        });
                    }
                }
                for collection in &root.collections {
                    if is_map_member_name(&collection.name) {
                        parser.diagnostics.push(SchemaDiagnostic {
                            line: 1,
                            column: 1,
                            message: format!(
                                "root collection '{}' shadows a Map member; rename it",
                                collection.name
                            ),
                        });
                    }
                    if schemas
                        .iter()
                        .all(|schema| schema.name != collection.schema)
                    {
                        parser.diagnostics.push(SchemaDiagnostic {
                            line: 1,
                            column: 1,
                            message: format!(
                                "state collection '{}' references unknown schema '{}'",
                                collection.name, collection.schema
                            ),
                        });
                    }
                }
            }
        }
    }

    if !parser.diagnostics.is_empty() {
        return Err(parser.diagnostics);
    }
    Ok(TsSchema { schemas, root })
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Number(String),
    String(String),
    Symbol(char),
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    line: usize,
    column: usize,
}

fn lex(source: &str) -> Result<Vec<Token>, SchemaDiagnostic> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut column = 1usize;

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            i += 1;
            line += 1;
            column = 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            column += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                column += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start_line = line;
            let start_column = column;
            i += 2;
            column += 2;
            let mut closed = false;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    i += 2;
                    column += 2;
                    closed = true;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                i += 1;
            }
            if !closed {
                return Err(SchemaDiagnostic {
                    line: start_line,
                    column: start_column,
                    message: "unterminated block comment".into(),
                });
            }
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            let quote = c;
            let start_line = line;
            let start_column = column;
            i += 1;
            column += 1;
            let mut value = String::new();
            let mut closed = false;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\\' {
                    if let Some(escaped) = chars.get(i + 1).copied() {
                        value.push(match escaped {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            other => other,
                        });
                        i += 2;
                        column += 2;
                        continue;
                    }
                }
                if ch == quote {
                    i += 1;
                    column += 1;
                    closed = true;
                    break;
                }
                if ch == '\n' {
                    if quote != '`' {
                        break;
                    }
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                value.push(ch);
                i += 1;
            }
            if !closed {
                return Err(SchemaDiagnostic {
                    line: start_line,
                    column: start_column,
                    message: "unterminated string literal".into(),
                });
            }
            tokens.push(Token {
                kind: TokenKind::String(value),
                line: start_line,
                column: start_column,
            });
            continue;
        }
        if c == '_' || c == '$' || c.is_alphabetic() {
            let start = i;
            let start_column = column;
            while i < chars.len()
                && (chars[i] == '_' || chars[i] == '$' || chars[i].is_alphanumeric())
            {
                i += 1;
                column += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident(chars[start..i].iter().collect()),
                line,
                column: start_column,
            });
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            let start_column = column;
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '_'
                    || chars[i] == '.'
                    || chars[i] == 'e'
                    || chars[i] == 'E'
                    || ((chars[i] == '+' || chars[i] == '-')
                        && matches!(chars.get(i - 1), Some('e' | 'E'))))
            {
                i += 1;
                column += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number(chars[start..i].iter().collect()),
                line,
                column: start_column,
            });
            continue;
        }
        // Keep the scanner tolerant of ordinary TypeScript punctuation; only
        // the schema call matters, so unknown characters become symbols.
        tokens.push(Token {
            kind: TokenKind::Symbol(c),
            line,
            column,
        });
        i += 1;
        column += 1;
    }

    tokens.push(Token {
        kind: TokenKind::Eof,
        line,
        column,
    });
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Declaration {
    schema: SchemaDef,
    /// `(collection_name, target_schema_name)` for `t.map(Target)` fields.
    maps: Vec<(String, String)>,
    /// Direct base schemas; resolved recursively and flattened base-first.
    extends: Vec<String>,
}

/// Flatten inherited fields into each declaration. `schema.ts` is parsed, not
/// compiled, so inheritance is data-level: the generated class contains the
/// union of base and own fields and the wire hash covers that union.
fn resolve_declarations(
    declarations: &[Declaration],
    diagnostics: &mut Vec<SchemaDiagnostic>,
) -> Vec<Declaration> {
    struct Resolver<'a> {
        declarations: &'a [Declaration],
        index: std::collections::HashMap<&'a str, usize>,
        state: Vec<u8>,
        resolved: Vec<Option<Declaration>>,
        diagnostics: &'a mut Vec<SchemaDiagnostic>,
    }

    impl Resolver<'_> {
        fn resolve(&mut self, index: usize) -> Option<Declaration> {
            match self.state[index] {
                2 => return self.resolved[index].clone(),
                1 => {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' inherits from itself (cycle)",
                            declaration.schema.name
                        ),
                    });
                    return None;
                }
                _ => {}
            }
            self.state[index] = 1;

            let extends = self.declarations[index].extends.clone();
            let own_fields = self.declarations[index].schema.fields.clone();
            let maps = self.declarations[index].maps.clone();
            let mut fields: Vec<SchemaField> = Vec::new();

            for base_name in &extends {
                let Some(&base_index) = self.index.get(base_name.as_str()) else {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' extends unknown schema '{}'",
                            declaration.schema.name, base_name
                        ),
                    });
                    continue;
                };
                let Some(base) = self.resolve(base_index) else {
                    continue;
                };
                if !base.maps.is_empty() {
                    let declaration = &self.declarations[index];
                    self.diagnostics.push(SchemaDiagnostic {
                        line: declaration.schema.line,
                        column: declaration.schema.column,
                        message: format!(
                            "schema '{}' cannot extend root schema '{}'",
                            declaration.schema.name, base_name
                        ),
                    });
                    continue;
                }
                for field in base.schema.fields {
                    if let Some(existing) =
                        fields.iter().find(|candidate| candidate.name == field.name)
                    {
                        self.diagnostics.push(SchemaDiagnostic {
                            line: field.line,
                            column: field.column,
                            message: format!(
                                "inherited field '{}' conflicts with a field inherited from {}:{}",
                                field.name, existing.line, existing.column
                            ),
                        });
                    } else {
                        fields.push(field);
                    }
                }
            }

            for field in own_fields {
                if let Some(existing) = fields.iter().find(|candidate| candidate.name == field.name)
                {
                    self.diagnostics.push(SchemaDiagnostic {
                        line: field.line,
                        column: field.column,
                        message: format!(
                            "field '{}' is already inherited from {}:{}",
                            field.name, existing.line, existing.column
                        ),
                    });
                } else {
                    fields.push(field);
                }
            }

            self.state[index] = 2;
            let schema = SchemaDef {
                name: self.declarations[index].schema.name.clone(),
                version: self.declarations[index].schema.version,
                fields,
                line: self.declarations[index].schema.line,
                column: self.declarations[index].schema.column,
            };
            let resolved = Declaration {
                schema,
                maps,
                extends: Vec::new(),
            };
            self.resolved[index] = Some(resolved.clone());
            Some(resolved)
        }
    }

    let index = declarations
        .iter()
        .enumerate()
        .map(|(i, declaration)| (declaration.schema.name.as_str(), i))
        .collect();
    let mut resolver = Resolver {
        declarations,
        index,
        state: vec![0; declarations.len()],
        resolved: vec![None; declarations.len()],
        diagnostics,
    };
    let mut out = Vec::new();
    for i in 0..declarations.len() {
        if let Some(declaration) = resolver.resolve(i) {
            out.push(declaration);
        }
    }
    out
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<SchemaDiagnostic>,
    declarations: Vec<Declaration>,
}

impl Parser {
    fn parse_all(&mut self) {
        while !self.at_eof() {
            if self.is_ident("schema") && self.peek_symbol(1, '(') {
                self.parse_call();
            } else {
                self.advance();
            }
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn is_ident(&self, text: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(name) if name == text)
    }

    fn peek_symbol(&self, offset: usize, symbol: char) -> bool {
        matches!(
            self.tokens.get(self.pos + offset).map(|token| &token.kind),
            Some(TokenKind::Symbol(found)) if *found == symbol
        )
    }

    fn eat_symbol(&mut self, symbol: char) -> bool {
        if self.current().kind == TokenKind::Symbol(symbol) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn error(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.diagnostics.push(SchemaDiagnostic {
            line,
            column,
            message: message.into(),
        });
    }

    fn parse_call(&mut self) {
        let keyword = self.advance();
        let schema_pos = self.pos - 1;
        let inferred_name = self.infer_const_name(schema_pos);
        self.advance(); // '('

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

        let mut fields = Vec::new();
        let mut maps = Vec::new();
        loop {
            if self.at_eof() {
                self.error(keyword.line, keyword.column, "unterminated schema object");
                return;
            }
            if self.eat_symbol('}') {
                break;
            }

            let key_token = self.current().clone();
            let field_name = match &key_token.kind {
                TokenKind::Ident(name) | TokenKind::String(name) => name.clone(),
                _ => {
                    self.error(
                        key_token.line,
                        key_token.column,
                        "expected a schema field name",
                    );
                    self.skip_to_field_separator();
                    if self.eat_symbol(',') {
                        continue;
                    }
                    self.eat_symbol('}');
                    break;
                }
            };
            self.advance();

            if !self.eat_symbol(':') {
                let token = self.current().clone();
                self.error(
                    token.line,
                    token.column,
                    format!("expected ':' after schema field '{field_name}'"),
                );
                self.skip_to_field_separator();
                if self.eat_symbol(',') {
                    continue;
                }
                self.eat_symbol('}');
                break;
            }

            let parsed = self.parse_type(&field_name);
            let mut default = None;
            let mut flags = 0u8;
            self.parse_modifiers(&field_name, parsed.as_ref(), &mut default, &mut flags);

            if let Some(parsed) = parsed {
                let reserved = is_reserved_field_name(&field_name);
                let duplicate = fields
                    .iter()
                    .any(|field: &SchemaField| field.name == field_name)
                    || maps.iter().any(|(name, _)| *name == field_name);
                if reserved {
                    self.error(
                        key_token.line,
                        key_token.column,
                        format!(
                            "schema field name '{field_name}' is reserved by the generated code"
                        ),
                    );
                } else if duplicate {
                    self.error(
                        key_token.line,
                        key_token.column,
                        format!("schema declares field '{field_name}' more than once"),
                    );
                } else {
                    match parsed {
                        ParsedType::Map(target) => maps.push((field_name, target)),
                        ParsedType::Primitive(primitive) => fields.push(SchemaField {
                            name: field_name,
                            kind: primitive.kind,
                            element: primitive.element,
                            ref_target: primitive.ref_target,
                            flags,
                            source_type: primitive.source_type,
                            default,
                            line: key_token.line,
                            column: key_token.column,
                        }),
                    }
                }
            }

            if self.eat_symbol(',') {
                continue;
            }
            if self.eat_symbol('}') {
                break;
            }
            let token = self.current().clone();
            self.error(
                token.line,
                token.column,
                "expected ',' or '}' after schema field",
            );
            self.skip_to_field_separator();
            if self.eat_symbol(',') {
                continue;
            }
            self.eat_symbol('}');
            break;
        }

        let mut name = None;
        let mut extends = Vec::new();
        let mut version = 1u32;
        if self.eat_symbol(',') {
            if let TokenKind::String(value) = self.current().kind.clone() {
                name = Some(value);
                self.advance();
            } else if self.current().kind == TokenKind::Symbol(',') {
                // `schema({...},, ...)` is malformed; fall through to inference.
            } else {
                // A non-string second argument would be read as the schema name
                // by Colyseus; require the explicit string form.
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

        // Fluent declarative options: .extend(Base).version(2)
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
            self.advance(); // '.'
            self.advance(); // option name
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
                                version = value as u32
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

    fn infer_const_name(&self, schema_pos: usize) -> Option<String> {
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

    fn parse_type(&mut self, field_name: &str) -> Option<ParsedType> {
        let start = self.current().clone();
        if !self.is_ident("t") {
            self.error(
                start.line,
                start.column,
                format!("field '{field_name}' must use a `t.*` type, e.g. t.number()"),
            );
            self.skip_expression();
            return None;
        }
        self.advance();
        if !self.eat_symbol('.') {
            self.error(
                start.line,
                start.column,
                "expected '.' after `t`, e.g. t.number()",
            );
            return None;
        }
        let type_token = self.current().clone();
        let type_name = match &type_token.kind {
            TokenKind::Ident(name) => name.clone(),
            _ => {
                self.error(
                    type_token.line,
                    type_token.column,
                    "expected a type name after `t.`",
                );
                return None;
            }
        };
        self.advance();

        if type_name == "map" {
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
            return Some(ParsedType::Map(target));
        }

        if type_name == "ref" {
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
            return Some(ParsedType::Primitive(ParsedPrimitive {
                kind: WireKind::U32,
                element: None,
                ref_target: Some(target.clone()),
                source_type: format!("t.ref({target})"),
            }));
        }

        if type_name == "array" {
            return self.parse_array_type(&type_token);
        }

        let kind = primitive_kind(&type_name);
        if self.eat_symbol('(') {
            self.skip_balanced('(', ')');
        }
        if let Some(kind) = kind {
            return Some(ParsedType::Primitive(ParsedPrimitive {
                kind,
                element: None,
                ref_target: None,
                source_type: format!("t.{type_name}"),
            }));
        }

        let message = match type_name.as_str() {
            "set" | "collection" | "stream" => {
                "collections are not supported inside an OXS2 schema; use t.array(t.u32()) for flat values or t.map(Entity) for a root collection"
            }
            "uint64" | "biguint64" | "bigint64" => {
                "64-bit unsigned/BigInt fields are not supported by OXS2 state sync yet; use int64 or split the value"
            }
            "quantized" | "angle" => {
                "t.quantized()/t.angle() are not supported by OXS2 state sync yet"
            }
            _ => {
                "unknown schema type; use t.number, t.float32, t.int8/16/32/64, t.uint8/16/32, t.boolean, t.string, t.array(t.*) or t.ref"
            }
        };
        self.error(type_token.line, type_token.column, message);
        None
    }

    fn parse_array_type(&mut self, type_token: &Token) -> Option<ParsedType> {
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
                // Entity references travel as u32 ids, so arrays of refs are
                // arrays of ids.
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
        } else {
            self.error(
                inner.line,
                inner.column,
                "t.array(...) expects a t.* element type, e.g. t.array(t.uint32())",
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

    fn parse_modifiers(
        &mut self,
        field_name: &str,
        parsed: Option<&ParsedType>,
        default: &mut Option<String>,
        flags: &mut u8,
    ) {
        while self.eat_symbol('.') {
            let token = self.current().clone();
            let modifier = match &token.kind {
                TokenKind::Ident(name) => name.clone(),
                _ => {
                    self.error(token.line, token.column, "expected a modifier name");
                    return;
                }
            };
            self.advance();
            if !self.eat_symbol('(') {
                self.error(
                    token.line,
                    token.column,
                    format!("expected '(' after .{modifier}()"),
                );
                return;
            }

            match modifier.as_str() {
                "default" => {
                    let literal_token = self.current().clone();
                    let literal = self.parse_default_literal();
                    if !self.eat_symbol(')') {
                        self.skip_balanced('(', ')');
                    }
                    let primitive = match parsed {
                        Some(ParsedType::Primitive(primitive)) => Some(primitive),
                        _ => None,
                    };
                    match (primitive, literal) {
                        (Some(primitive), Some(_literal)) if primitive.kind == WireKind::Array => {
                            self.error(
                                literal_token.line,
                                literal_token.column,
                                format!(
                                    ".default() is not supported on array field '{field_name}'; assign the array in the constructor instead"
                                ),
                            );
                        }
                        (Some(primitive), Some(literal)) => {
                            if let Some(normalized) = validate_default(primitive.kind, &literal) {
                                *default = Some(normalized);
                            } else {
                                self.error(
                                    literal_token.line,
                                    literal_token.column,
                                    format!(
                                        "default value for field '{field_name}' is out of range or has the wrong type"
                                    ),
                                );
                            }
                        }
                        _ => {
                            self.error(
                                literal_token.line,
                                literal_token.column,
                                format!(
                                    ".default() is only supported on primitive and string fields (field '{field_name}')"
                                ),
                            );
                        }
                    }
                }
                "optional" | "static" | "transient" => {
                    if !self.eat_symbol(')') {
                        self.skip_balanced('(', ')');
                        self.error(
                            token.line,
                            token.column,
                            format!(".{modifier}() takes no arguments"),
                        );
                    }
                    *flags |= match modifier.as_str() {
                        "optional" => FIELD_OPTIONAL,
                        "static" => FIELD_STATIC,
                        _ => FIELD_TRANSIENT,
                    };
                }
                "version" => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        ".version() applies to the whole schema; use schema({...}, \"Name\").version(n)",
                    );
                }
                "view" | "validate" => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        format!(
                            ".{modifier}() is not supported by OXS2 state sync; encode the value directly or validate it in the room"
                        ),
                    );
                }
                _ => {
                    self.skip_balanced('(', ')');
                    self.error(
                        token.line,
                        token.column,
                        format!("unknown schema modifier .{modifier}()"),
                    );
                }
            }
        }
    }

    /// Accepts `.default(value)` and the generator form `.default(() => value)`.
    fn parse_default_literal(&mut self) -> Option<Literal> {
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

    fn parse_literal(&mut self) -> Option<Literal> {
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

    fn skip_to_field_separator(&mut self) {
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

    fn skip_expression(&mut self) {
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

    /// Consume through the matching `close` after the opening `open` has
    /// already been consumed.
    fn skip_balanced(&mut self, _open: char, _close: char) {
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

struct ParsedPrimitive {
    kind: WireKind,
    element: Option<WireKind>,
    ref_target: Option<String>,
    source_type: String,
}

enum ParsedType {
    Primitive(ParsedPrimitive),
    Map(String),
}

fn primitive_kind(name: &str) -> Option<WireKind> {
    match name {
        "number" | "float64" | "f64" => Some(WireKind::F64),
        "float32" | "f32" => Some(WireKind::F32),
        "int8" | "i8" => Some(WireKind::I8),
        "uint8" | "u8" => Some(WireKind::U8),
        "int16" | "i16" => Some(WireKind::I16),
        "uint16" | "u16" => Some(WireKind::U16),
        "int32" | "i32" => Some(WireKind::I32),
        "uint32" | "u32" => Some(WireKind::U32),
        "int64" | "i64" => Some(WireKind::I64),
        "boolean" | "bool" => Some(WireKind::Bool),
        "string" => Some(WireKind::String),
        _ => None,
    }
}

/// Root collections and scalars live on `room.state`, which is a `Map`.
/// Shadowing `Map` members would break every consumer, so reject them.
fn is_map_member_name(name: &str) -> bool {
    matches!(
        name,
        "size"
            | "get"
            | "set"
            | "has"
            | "delete"
            | "clear"
            | "keys"
            | "values"
            | "entries"
            | "forEach"
            | "toJSON"
            | "onAdd"
            | "onRemove"
            | "onChange"
    )
}

enum Literal {
    Number(String),
    Bool(bool),
    Str(String),
}

impl Literal {
    fn number(&self) -> Option<&str> {
        match self {
            Literal::Number(value) => Some(value),
            _ => None,
        }
    }
}

fn validate_default(kind: WireKind, literal: &Literal) -> Option<String> {
    match kind {
        WireKind::Bool => match literal {
            Literal::Bool(value) => Some(value.to_string()),
            _ => None,
        },
        WireKind::String => match literal {
            Literal::Str(value) => Some(value.clone()),
            _ => None,
        },
        WireKind::F32 | WireKind::F64 => literal
            .number()?
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|_| literal.number().unwrap().to_string()),
        WireKind::I64 => {
            let value = literal.number()?;
            value.parse::<i64>().ok().map(|_| value.to_string())
        }
        WireKind::U8 => range(literal, 0, u8::MAX as i64),
        WireKind::I8 => range(literal, i8::MIN as i64, i8::MAX as i64),
        WireKind::U16 => range(literal, 0, u16::MAX as i64),
        WireKind::I16 => range(literal, i16::MIN as i64, i16::MAX as i64),
        WireKind::U32 => range(literal, 0, u32::MAX as i64),
        WireKind::I32 => range(literal, i32::MIN as i64, i32::MAX as i64),
        WireKind::Array => None,
    }
}

fn range(literal: &Literal, min: i64, max: i64) -> Option<String> {
    let value = literal.number()?;
    value
        .parse::<i64>()
        .ok()
        .filter(|parsed| *parsed >= min && *parsed <= max)
        .map(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_colyseus_v5_schema_and_root_collection() {
        let source = r#"
            import { schema, t } from "./cyg-server";

            export const Player = schema({
                x: t.number(),
                y: t.number(),
                flags: t.uint32(),
            }, "Player");

            export const State = schema({
                players: t.map(Player),
            }, "State");
        "#;
        let parsed = parse(source).expect("schema.ts parses");
        assert_eq!(parsed.schemas.len(), 1);
        let player = &parsed.schemas[0];
        assert_eq!(player.name, "Player");
        assert_eq!(player.version, 1);
        assert_eq!(player.fields.len(), 3);
        assert_eq!(player.fields[0].kind, WireKind::F64);
        assert_eq!(player.fields[2].kind, WireKind::U32);
        let root = parsed.root.expect("root world");
        assert_eq!(root.name, "State");
        assert_eq!(root.collections.len(), 1);
        assert_eq!(root.collections[0].name, "players");
        assert_eq!(root.collections[0].schema, "Player");
    }

    #[test]
    fn infers_schema_name_from_const_binding() {
        let parsed = parse("const Tiny = schema({ hp: t.uint8() });").unwrap();
        assert_eq!(parsed.schemas[0].name, "Tiny");
    }

    #[test]
    fn accepts_aliases_refs_and_defaults() {
        let source = r#"
            const Player = schema({
                hp: t.uint8().default(100),
                shield: t.int16().default(-5),
                speed: t.float32(),
                target: t.ref(Player),
                alive: t.boolean().default(true),
            }, "Player");
        "#;
        let parsed = parse(source).unwrap();
        let fields = &parsed.schemas[0].fields;
        assert_eq!(fields[0].kind, WireKind::U8);
        assert_eq!(fields[0].default.as_deref(), Some("100"));
        assert_eq!(fields[1].kind, WireKind::I16);
        assert_eq!(fields[1].default.as_deref(), Some("-5"));
        assert_eq!(fields[2].kind, WireKind::F32);
        assert_eq!(fields[3].kind, WireKind::U32);
        assert_eq!(fields[4].default.as_deref(), Some("true"));
    }

    #[test]
    fn rejects_unsupported_containers_and_field_version() {
        let errors =
            parse(r#"const Bad = schema({ samples: t.array(t.int64()) }, "Bad");"#).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("loses precision")));

        let errors =
            parse(r#"const Bad = schema({ names: t.array(t.string()) }, "Bad");"#).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("numeric or boolean"))
        );

        let errors =
            parse(r#"const Bad = schema({ items: t.set(t.number()) }, "Bad");"#).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("collections are not supported"))
        );

        let errors =
            parse(r#"const Bad = schema({ hp: t.uint8().version(2) }, "Bad");"#).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("applies to the whole schema"))
        );
    }

    #[test]
    fn parses_strings_flags_arrays_and_function_defaults() {
        let source = r#"
            const Player = schema({
                name: t.string().default("rookie"),
                hp: t.uint8().default(() => 100),
                tag: t.string().optional(),
                score: t.uint32().static(),
                seen: t.array(t.float32()),
                target: t.ref(Player),
            }, "Player");
        "#;
        let parsed = parse(source).expect("phase 3 fields parse");
        let fields = &parsed.schemas[0].fields;
        assert_eq!(fields[0].kind, WireKind::String);
        assert_eq!(fields[0].default.as_deref(), Some("rookie"));
        assert_eq!(fields[1].default.as_deref(), Some("100"));
        assert_eq!(fields[2].kind, WireKind::String);
        assert_eq!(fields[2].flags & FIELD_OPTIONAL, FIELD_OPTIONAL);
        assert_eq!(fields[3].flags & FIELD_STATIC, FIELD_STATIC);
        assert_eq!(fields[4].kind, WireKind::Array);
        assert_eq!(fields[4].element, Some(WireKind::F32));
        assert_eq!(fields[5].kind, WireKind::U32);
        assert_eq!(fields[5].ref_target.as_deref(), Some("Player"));
    }

    #[test]
    fn supports_inheritance_third_arg_and_extend_chain() {
        let source = r#"
            const Base = schema({ x: t.number(), hp: t.uint8() }, "Base");
            const Child = schema({ name: t.string() }, "Child", Base);
            const Grand = schema({ score: t.uint32() }, "Grand").extend(Child).version(3);
        "#;
        let parsed = parse(source).expect("inheritance parses");
        let names: Vec<&str> = parsed.schemas[2]
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["x", "hp", "name", "score"],
            "base fields come first"
        );
        assert_eq!(parsed.schemas[2].version, 3);
        assert_eq!(parsed.schemas[1].version, 1);
    }

    #[test]
    fn rejects_bad_inheritance_and_ref_targets() {
        let errors = parse(
            r#"
            const Base = schema({ x: t.number() }, "Base");
            const Child = schema({ x: t.uint8() }, "Child").extend(Base);
            "#,
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("already inherited"))
        );

        let errors = parse(r#"const Child = schema({ x: t.number() }, "Child").extend(Missing);"#)
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("extends unknown schema"))
        );

        let errors =
            parse(r#"const Bad = schema({ target: t.ref(Missing) }, "Bad");"#).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("references unknown schema"))
        );

        let cycle = parse(
            r#"
            const A = schema({ a: t.number() }, "A").extend(B);
            const B = schema({ b: t.number() }, "B").extend(A);
            "#,
        )
        .unwrap_err();
        assert!(cycle.iter().any(|e| e.message.contains("cycle")));
    }

    #[test]
    fn rejects_out_of_range_defaults_and_duplicate_fields() {
        let errors =
            parse(r#"const Bad = schema({ hp: t.uint8().default(300) }, "Bad");"#).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("out of range")));

        let errors =
            parse(r#"const Bad = schema({ x: t.number(), x: t.number() }, "Bad");"#).unwrap_err();
        assert!(errors.iter().any(|e| e.message.contains("more than once")));
    }

    #[test]
    fn parses_multi_collection_root_with_scalars() {
        let source = r#"
            const Player = schema({ x: t.number() }, "Player");
            const Projectile = schema({ speed: t.float32() }, "Projectile");
            export const State = schema({
                tick: t.uint32(),
                paused: t.boolean(),
                players: t.map(Player),
                projectiles: t.map(Projectile),
            }, "State");
        "#;
        let parsed = parse(source).expect("multi root parses");
        assert_eq!(
            parsed.schemas.len(),
            2,
            "root scalars are not an entity schema"
        );
        let root = parsed.root.expect("root world");
        assert_eq!(root.name, "State");
        assert_eq!(root.scalars.len(), 2);
        assert_eq!(root.scalars[0].kind, WireKind::U32);
        assert_eq!(root.scalars[1].kind, WireKind::Bool);
        assert_eq!(root.collections.len(), 2);
        assert_eq!(root.collections[0].name, "players");
        assert_eq!(root.collections[0].schema, "Player");
        assert_eq!(root.collections[1].name, "projectiles");
    }

    #[test]
    fn rejects_collection_conflicts_and_unknown_targets() {
        let duplicate = parse(
            r#"
            const P = schema({ x: t.number() }, "P");
            const S = schema({ players: t.map(P), players: t.map(P) }, "S");
            "#,
        )
        .unwrap_err();
        assert!(
            duplicate
                .iter()
                .any(|e| e.message.contains("more than once"))
        );

        let unknown = parse(
            r#"
            const P = schema({ x: t.number() }, "P");
            const S = schema({ ghosts: t.map(Missing) }, "S");
            "#,
        )
        .unwrap_err();
        assert!(
            unknown
                .iter()
                .any(|e| e.message.contains("unknown schema 'Missing'"))
        );

        let shadow = parse(
            r#"
            const P = schema({ x: t.number() }, "P");
            const S = schema({ size: t.uint32(), p: t.map(P) }, "S");
            "#,
        )
        .unwrap_err();
        assert!(shadow.iter().any(|e| e.message.contains("Map member")));
    }

    #[test]
    fn ignores_comments_and_non_schema_code() {
        let source = r#"
            // const Old = schema({ nope: t.string() }, "Old");
            /* schema({ nope: t.string() }, "Old"); */
            const label = "schema({ nope: t.string() }, \"Old\")";
            export const Player = schema({ x: t.number() }, "Player");
        "#;
        let parsed = parse(source).unwrap();
        assert_eq!(parsed.schemas.len(), 1);
        assert_eq!(parsed.schemas[0].name, "Player");
    }
}
