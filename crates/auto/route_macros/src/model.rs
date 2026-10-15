use syn::{Ident, LitStr, Type};

use crate::options::Docs;

pub(crate) struct Route {
    pub(crate) method: Ident,
    pub(crate) handler: Ident,
    pub(crate) path: LitStr,
    pub(crate) docs: Docs,
    pub(crate) argument_types: Vec<Type>,
    pub(crate) request_body: Option<RequestBody>,
    pub(crate) response_body: Option<ResponseBody>,
    pub(crate) params: Vec<OpenApiParam>,
    pub(crate) live_event: Option<LitStr>,
    pub(crate) live_client_name: Option<LitStr>,
    pub(crate) live_table: Option<LitStr>,
    pub(crate) live_tables: Vec<LitStr>,
    pub(crate) live_strategy: LiveStrategy,
    pub(crate) live_capacity: Option<syn::LitInt>,
    pub(crate) live_replay: Option<syn::LitInt>,
    pub(crate) live_return_type: Option<Type>,
    pub(crate) live_returns_result: bool,
}

pub(crate) struct ModuleRoute {
    pub(crate) method: Ident,
    pub(crate) handler: Ident,
    pub(crate) path: LitStr,
    pub(crate) docs: Docs,
    pub(crate) request_body: Option<RequestBody>,
    pub(crate) response_body: Option<ResponseBody>,
    pub(crate) params: Vec<OpenApiParam>,
}

#[derive(Clone)]
pub(crate) struct RequestBody {
    pub(crate) ty: Type,
    pub(crate) content: RequestContent,
}

#[derive(Clone, Copy)]
pub(crate) enum RequestContent {
    Json,
    Form,
}

#[derive(Clone)]
pub(crate) enum ResponseBody {
    Json(Type),
    Sse(Option<Type>),
}

pub(crate) struct OpenApiParam {
    pub(crate) name: String,
    pub(crate) ty: Type,
    pub(crate) source: ParamSource,
}

#[derive(Clone, Copy)]
pub(crate) enum ParamSource {
    Path,
    Query,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveStrategy {
    Sqlite,
    Publish,
    Latest,
    Stream,
}
