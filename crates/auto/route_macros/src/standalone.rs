use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, FnArg, ItemFn, LitStr, PatType};

use crate::descriptor::{
    param_descriptor_tokens, request_schema_descriptor_tokens, response_schema_descriptor_tokens,
};
use crate::docs::{
    doc_comment_summary, join_paths, merged_docs, openapi_tag, option_lit_tokens,
};
use crate::model::ResponseBody;
use crate::options::RouteOptions;
use crate::params::infer_params;
use crate::schema::{infer_request_body, infer_response_body};

pub(crate) fn standalone_route(method: &str, attr: TokenStream, item: TokenStream) -> TokenStream {
    let route_options = parse_macro_input!(attr as RouteOptions);
    let function = parse_macro_input!(item as ItemFn);
    match expand_standalone_route(method, route_options, function) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_standalone_route(
    method: &str,
    route_options: RouteOptions,
    function: ItemFn,
) -> syn::Result<proc_macro2::TokenStream> {
    if function.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &function.sig,
            "standalone route handlers must be async",
        ));
    }
    if !function.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &function.sig.generics,
            "generic standalone route handlers are not supported",
        ));
    }

    let handler = &function.sig.ident;
    let operation_id = format!("{handler}");
    let method = format_ident!("{method}");
    let method_name = method.to_string().to_ascii_uppercase();
    let factory = format_ident!("__auto_route_factory_{}", handler);
    let path = LitStr::new(
        &join_paths("", &route_options.path.value()),
        route_options.path.span(),
    );
    let argument_types = function
        .sig
        .inputs
        .iter()
        .map(|argument| match argument {
            FnArg::Typed(PatType { ty, .. }) => Ok((**ty).clone()),
            FnArg::Receiver(receiver) => Err(syn::Error::new_spanned(
                receiver,
                "standalone route handlers cannot take self",
            )),
        })
        .collect::<syn::Result<Vec<_>>>()?;
    let params = infer_params(&path.value(), &argument_types);
    let params = param_descriptor_tokens(&params);
    let request_schema =
        request_schema_descriptor_tokens(infer_request_body(&argument_types).as_ref());
    let docs = merged_docs(&route_options.docs, doc_comment_summary(&function.attrs));
    let tag = docs
        .tag
        .unwrap_or_else(|| LitStr::new(&openapi_tag(&path.value()), path.span()));
    let tag_description = option_lit_tokens(docs.tag_description.as_ref());
    let summary = option_lit_tokens(docs.summary.as_ref());
    let description = option_lit_tokens(docs.description.as_ref());
    let request_description = option_lit_tokens(docs.request_description.as_ref());
    let response_description = option_lit_tokens(docs.response_description.as_ref());
    let response_body = route_options
        .sse_body
        .map(|ty| ResponseBody::Sse(Some(ty)))
        .or_else(|| infer_response_body(&function.sig.output));
    let response_schema = response_schema_descriptor_tokens(response_body.as_ref());

    Ok(quote! {
        #function

        #[doc(hidden)]
        fn #factory<'a>(
            _container: &'a ::auto_route::__private::auto_di::Container,
        ) -> ::auto_route::__private::auto_di::BoxFuture<
            'a,
            ::std::result::Result<
                ::auto_route::__private::axum::Router<()>,
                ::auto_route::__private::auto_di::DiError,
            >,
        > {
            ::std::boxed::Box::pin(async move {
                Ok(::auto_route::__private::axum::Router::new().route(
                    #path,
                    ::auto_route::__private::axum::routing::#method(#handler),
                ))
            })
        }

        ::auto_route::__private::inventory::submit! {
            ::auto_route::RouteDescriptor::new(#factory)
        }

        ::auto_route::__private::inventory::submit! {
            ::auto_route::OpenApiRouteDescriptor::new(
                #method_name,
                #path,
                #operation_id,
                #tag,
                #tag_description,
                #summary,
                #description,
                #params,
                #request_description,
                #request_schema,
                #response_description,
                #response_schema,
            )
        }
    })
}
