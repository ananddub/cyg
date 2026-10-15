use syn::{Ident, LitStr, Token, Type, parse::Parse, parse::ParseStream};

#[derive(Clone, Default)]
pub(crate) struct Docs {
    pub(crate) tag: Option<LitStr>,
    pub(crate) tag_description: Option<LitStr>,
    pub(crate) summary: Option<LitStr>,
    pub(crate) description: Option<LitStr>,
    pub(crate) request_description: Option<LitStr>,
    pub(crate) response_description: Option<LitStr>,
}

pub(crate) struct ControllerOptions {
    pub(crate) path: LitStr,
    pub(crate) docs: Docs,
}

pub(crate) struct RouteOptions {
    pub(crate) path: LitStr,
    pub(crate) sse_body: Option<Type>,
    pub(crate) docs: Docs,
}

impl ControllerOptions {
    fn empty() -> Self {
        Self {
            path: LitStr::new("", proc_macro2::Span::call_site()),
            docs: Docs::default(),
        }
    }
}

impl RouteOptions {
    pub(crate) fn empty() -> Self {
        Self {
            path: LitStr::new("", proc_macro2::Span::call_site()),
            sse_body: None,
            docs: Docs::default(),
        }
    }
}

impl Parse for ControllerOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut options = Self::empty();
        let mut needs_comma = false;
        if input.peek(LitStr) {
            options.path = input.parse()?;
            needs_comma = true;
        }
        while !input.is_empty() {
            if needs_comma {
                input.parse::<Token![,]>()?;
                if input.is_empty() {
                    break;
                }
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            parse_docs_option(&mut options.docs, key, input)?;
            needs_comma = true;
        }
        Ok(options)
    }
}

impl Parse for RouteOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut options = Self::empty();
        let mut needs_comma = false;
        if input.peek(LitStr) {
            options.path = input.parse()?;
            needs_comma = true;
        }
        while !input.is_empty() {
            if needs_comma {
                input.parse::<Token![,]>()?;
                if input.is_empty() {
                    break;
                }
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            if key == "sse" {
                if options.sse_body.is_some() {
                    return Err(syn::Error::new_spanned(key, "duplicate `sse` route option"));
                }
                options.sse_body = Some(input.parse()?);
            } else {
                parse_docs_option(&mut options.docs, key, input)?;
            }
            needs_comma = true;
        }
        Ok(options)
    }
}

fn parse_docs_option(docs: &mut Docs, key: Ident, input: ParseStream<'_>) -> syn::Result<()> {
    let value: LitStr = input.parse()?;
    match key.to_string().as_str() {
        "tag" => set_lit(&mut docs.tag, key, value),
        "tag_description" => set_lit(&mut docs.tag_description, key, value),
        "summary" => set_lit(&mut docs.summary, key, value),
        "description" | "docs" => set_lit(&mut docs.description, key, value),
        "request_description" | "request_docs" => {
            set_lit(&mut docs.request_description, key, value)
        }
        "response_description" | "response_docs" => {
            set_lit(&mut docs.response_description, key, value)
        }
        _ => Err(syn::Error::new_spanned(
            key,
            "unsupported route option; expected `sse = Type`, `tag`, `summary`, `description`, `request_description`, or `response_description`",
        )),
    }
}

fn set_lit(slot: &mut Option<LitStr>, key: Ident, value: LitStr) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(key, "duplicate route option"));
    }
    *slot = Some(value);
    Ok(())
}
