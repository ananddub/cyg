use syn::parse::Parse;
use syn::parse::ParseStream;
use syn::{Ident, LitStr, Token};

use crate::model::LiveStrategy;

pub(crate) struct LiveOptions {
    pub(crate) client_name: Option<LitStr>,
    pub(crate) table: Option<LitStr>,
    pub(crate) tables: Vec<LitStr>,
    pub(crate) strategy: Option<LiveStrategy>,
    pub(crate) capacity: Option<syn::LitInt>,
    pub(crate) replay: Option<syn::LitInt>,
}

impl Parse for LiveOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut client_name = None;
        let mut table = None;
        let mut tables = Vec::new();
        let mut strategy = None;
        let mut capacity = None;
        let mut replay = None;
        while !input.is_empty() {
            if input.peek(LitStr) {
                client_name = Some(input.parse()?);
            } else {
                let key: Ident = input.parse()?;
                input.parse::<Token![=]>()?;
                if key == "table" {
                    let value: LitStr = input.parse()?;
                    table = Some(value);
                } else if key == "tables" {
                    let content;
                    syn::bracketed!(content in input);
                    while !content.is_empty() {
                        tables.push(content.parse::<LitStr>()?);
                        if content.is_empty() {
                            break;
                        }
                        content.parse::<Token![,]>()?;
                    }
                } else if key == "strategy" {
                    let value: Ident = input.parse()?;
                    strategy = Some(match value.to_string().as_str() {
                        "sqlite" => LiveStrategy::Sqlite,
                        "publish" => LiveStrategy::Publish,
                        "latest" => LiveStrategy::Latest,
                        "stream" => LiveStrategy::Stream,
                        _ => {
                            return Err(syn::Error::new_spanned(
                                value,
                                "strategy must be sqlite, publish, latest, or stream",
                            ));
                        }
                    });
                } else if key == "capacity" {
                    capacity = Some(input.parse()?);
                } else if key == "replay" {
                    replay = Some(input.parse()?);
                } else {
                    return Err(syn::Error::new_spanned(key, "unknown #[live] option"));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        Ok(Self {
            client_name,
            table,
            tables,
            strategy,
            capacity,
            replay,
        })
    }
}
