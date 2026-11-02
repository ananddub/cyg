use syn::{FnArg, Ident, ImplItemFn, LitStr, Meta, PatType, Signature, Type};

use crate::docs::route_method;
use crate::live_options::LiveOptions;
use crate::model::LiveStrategy;

pub(super) struct Harvested {
    pub(super) route_attributes: Vec<(syn::Attribute, Ident)>,
    pub(super) is_live: bool,
    pub(super) live_event: Option<LitStr>,
    pub(super) live_client_name: Option<LitStr>,
    pub(super) live_table: Option<LitStr>,
    pub(super) live_tables: Vec<LitStr>,
    pub(super) live_strategy: Option<LiveStrategy>,
    pub(super) live_capacity: Option<syn::LitInt>,
    pub(super) live_replay: Option<syn::LitInt>,
}

pub(super) fn harvest_attributes(function: &mut ImplItemFn) -> syn::Result<Harvested> {
    let mut route_attributes = Vec::new();
    let mut retained_attributes = Vec::new();
    let mut is_live = false;
    let mut live_event = None;
    let mut live_client_name = None;
    let mut live_table = None;
    let mut live_tables = Vec::new();
    let mut live_strategy = None;
    let mut live_capacity = None;
    let mut live_replay = None;
    for attribute in std::mem::take(&mut function.attrs) {
        if let Some(method) = route_method(&attribute) {
            route_attributes.push((attribute, method));
        } else if attribute.path().is_ident("live") {
            match &attribute.meta {
                Meta::Path(_) => {}
                Meta::List(_) => {
                    let options = attribute
                        .parse_args::<LiveOptions>()
                        .map_err(|error| syn::Error::new_spanned(&attribute, error.to_string()))?;
                    live_client_name = options.client_name;
                    live_table = options.table;
                    live_tables = options.tables;
                    live_strategy = options.strategy;
                    live_capacity = options.capacity;
                    live_replay = options.replay;
                }
                Meta::NameValue(_) => {
                    return Err(syn::Error::new_spanned(
                        &attribute,
                        "expected #[live] or #[live(\"client_name\")]",
                    ));
                }
            }
            is_live = true;
        } else if attribute.path().is_ident("on") {
            live_event = Some(attribute.parse_args::<LitStr>().map_err(|_| {
                syn::Error::new_spanned(&attribute, "expected #[on(\"event:name\")]")
            })?);
        } else {
            retained_attributes.push(attribute);
        }
    }
    function.attrs = retained_attributes;

    Ok(Harvested {
        route_attributes,
        is_live,
        live_event,
        live_client_name,
        live_table,
        live_tables,
        live_strategy,
        live_capacity,
        live_replay,
    })
}

pub(super) fn receiver_arguments(signature: &Signature) -> syn::Result<Vec<Type>> {
    let mut inputs = signature.inputs.iter();
    match inputs.next() {
        Some(FnArg::Receiver(receiver))
            if receiver.reference.is_some() || receiver.colon_token.is_some() => {}
        _ => {
            return Err(syn::Error::new_spanned(
                signature,
                "controller route methods must take &self or self: Arc<Self> as their first argument",
            ));
        }
    }
    inputs
        .map(|argument| match argument {
            FnArg::Typed(PatType { ty, .. }) => Ok((**ty).clone()),
            FnArg::Receiver(receiver) => {
                Err(syn::Error::new_spanned(receiver, "unexpected receiver argument"))
            }
        })
        .collect()
}
