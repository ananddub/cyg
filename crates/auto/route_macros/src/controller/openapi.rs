use quote::quote;
use syn::{Ident, LitStr};

use crate::descriptor::{
    param_descriptor_tokens, request_schema_descriptor_tokens, response_schema_descriptor_tokens,
};
use crate::docs::{openapi_tag, option_lit_tokens};
use crate::model::{ModuleRoute, Route};
use crate::options::Docs;

pub(crate) fn controller_openapi_submissions(
    routes: &[Route],
    type_ident: &Ident,
    controller_docs: &Docs,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .map(|route| {
            let method = route.method.to_string().to_ascii_uppercase();
            let path = &route.path;
            let handler = &route.handler;
            let operation_id = format!("{type_ident}::{handler}");
            let tag = route
                .docs
                .tag
                .as_ref()
                .or(controller_docs.tag.as_ref())
                .cloned()
                .unwrap_or_else(|| LitStr::new(&openapi_tag(&path.value()), path.span()));
            let tag_description = option_lit_tokens(
                route
                    .docs
                    .tag_description
                    .as_ref()
                    .or(controller_docs.tag_description.as_ref())
                    .or(controller_docs.description.as_ref()),
            );
            let summary = option_lit_tokens(route.docs.summary.as_ref());
            let description = option_lit_tokens(route.docs.description.as_ref());
            let request_description = option_lit_tokens(route.docs.request_description.as_ref());
            let response_description = option_lit_tokens(route.docs.response_description.as_ref());
            let params = param_descriptor_tokens(&route.params);
            let request_schema = request_schema_descriptor_tokens(route.request_body.as_ref());
            let response_schema = response_schema_descriptor_tokens(route.response_body.as_ref());
            quote! {
                ::auto_route::__private::inventory::submit! {
                    ::auto_route::OpenApiRouteDescriptor::new(
                        #method,
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
            }
        })
        .collect()
}

pub(crate) fn module_openapi_submissions(
    routes: &[ModuleRoute],
    module_ident: &Ident,
    controller_docs: &Docs,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .map(|route| {
            let method = route.method.to_string().to_ascii_uppercase();
            let path = &route.path;
            let operation_id = format!("{}::{}", module_ident, route.handler);
            let tag = route
                .docs
                .tag
                .as_ref()
                .or(controller_docs.tag.as_ref())
                .cloned()
                .unwrap_or_else(|| LitStr::new(&openapi_tag(&path.value()), path.span()));
            let tag_description = option_lit_tokens(
                route
                    .docs
                    .tag_description
                    .as_ref()
                    .or(controller_docs.tag_description.as_ref())
                    .or(controller_docs.description.as_ref()),
            );
            let summary = option_lit_tokens(route.docs.summary.as_ref());
            let description = option_lit_tokens(route.docs.description.as_ref());
            let request_description = option_lit_tokens(route.docs.request_description.as_ref());
            let response_description = option_lit_tokens(route.docs.response_description.as_ref());
            let params = param_descriptor_tokens(&route.params);
            let request_schema = request_schema_descriptor_tokens(route.request_body.as_ref());
            let response_schema = response_schema_descriptor_tokens(route.response_body.as_ref());
            quote! {
                ::auto_route::__private::inventory::submit! {
                    ::auto_route::OpenApiRouteDescriptor::new(
                        #method,
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
            }
        })
        .collect()
}
