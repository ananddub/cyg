use quote::quote;
use syn::{Attribute, Ident, LitStr, Type};

use crate::options::{Docs, RouteOptions};

const METHODS: &[&str] = &["get", "post", "put", "delete", "patch", "options", "head"];

pub(crate) fn route_method(attribute: &Attribute) -> Option<Ident> {
    let ident = attribute.path().get_ident()?;
    METHODS
        .contains(&ident.to_string().as_str())
        .then(|| ident.clone())
}

pub(crate) fn marker_options(attribute: &Attribute) -> syn::Result<RouteOptions> {
    attribute.parse_args::<RouteOptions>().or_else(|error| {
        if matches!(&attribute.meta, syn::Meta::Path(_)) {
            Ok(RouteOptions::empty())
        } else {
            Err(error)
        }
    })
}

pub(crate) fn merged_docs(explicit: &Docs, fallback_summary: Option<LitStr>) -> Docs {
    let mut docs = explicit.clone();
    if docs.summary.is_none() {
        docs.summary = fallback_summary;
    }
    docs
}

pub(crate) fn option_lit_tokens(value: Option<&LitStr>) -> proc_macro2::TokenStream {
    match value {
        Some(value) => quote!(::std::option::Option::Some(#value)),
        None => quote!(::std::option::Option::None),
    }
}

pub(crate) fn doc_comment_summary(attributes: &[Attribute]) -> Option<LitStr> {
    let mut lines = attributes
        .iter()
        .filter_map(|attribute| {
            if !attribute.path().is_ident("doc") {
                return None;
            }
            let syn::Meta::NameValue(name_value) = &attribute.meta else {
                return None;
            };
            let syn::Expr::Lit(expr_lit) = &name_value.value else {
                return None;
            };
            let syn::Lit::Str(value) = &expr_lit.lit else {
                return None;
            };
            Some(value.value().trim().to_owned())
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return None;
    }

    let summary = lines.remove(0);
    Some(LitStr::new(&summary, proc_macro2::Span::call_site()))
}

pub(crate) fn type_ident(ty: &Type) -> syn::Result<&Ident> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "controller type must be a named type",
        ));
    };
    path.path
        .segments
        .last()
        .map(|segment| &segment.ident)
        .ok_or_else(|| syn::Error::new_spanned(ty, "controller type must be a named type"))
}

/// The API version a path was authored under: `/v1/leaderboard` -> `Some("v1")`.
///
/// Only the leading segment counts, and only when it is exactly `v1`: a path
/// such as `/v2fa` or `/status` is left alone, and a controller that still says
/// `/v2/...` is *not* silently rewritten — it registers at `/v2`, where
/// `tests/auto_routes/only_v1_is_versioned.rs` fails on it.
pub(crate) fn api_version(path: &str) -> Option<&str> {
    match path.trim_start_matches('/').split('/').next() {
        Some("v1") => Some("v1"),
        _ => None,
    }
}

/// The path without its leading API version segment, ready for an axum router:
/// `/v1/leaderboard/{id}` -> `/leaderboard/{id}`. A path with no version is
/// returned unchanged.
pub(crate) fn without_version_prefix(path: &str) -> String {
    match api_version(path) {
        Some(version) => {
            let stripped = path.trim_start_matches('/');
            let rest = stripped[version.len()..].trim_start_matches('/');
            format!("/{rest}")
        }
        None => path.to_owned(),
    }
}

pub(crate) fn join_paths(base: &str, route: &str) -> String {
    let base = base.trim_matches('/');
    let route = route.trim_matches('/');
    let joined = match (base.is_empty(), route.is_empty()) {
        (true, true) => "/".to_owned(),
        (false, true) => format!("/{base}"),
        (true, false) => format!("/{route}"),
        (false, false) => format!("/{base}/{route}"),
    };

    joined
        .split('/')
        .map(|segment| {
            if let Some(parameter) = segment.strip_prefix(':') {
                format!("{{{parameter}}}")
            } else if let Some(parameter) = segment.strip_prefix('*') {
                format!("{{*{parameter}}}")
            } else {
                segment.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn openapi_tag(path: &str) -> String {
    path.trim_start_matches('/')
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("default")
        .to_owned()
}

pub(crate) fn validate_route_signature(signature: &syn::Signature, label: &str) -> syn::Result<()> {
    if signature.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            signature,
            format!("{label} must be async"),
        ));
    }
    if !signature.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &signature.generics,
            format!("generic {label} are not supported"),
        ));
    }
    Ok(())
}
