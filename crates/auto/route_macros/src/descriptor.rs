use quote::quote;
use syn::LitStr;

use crate::model::{OpenApiParam, ParamSource, RequestBody, RequestContent, ResponseBody};

pub(crate) fn request_schema_descriptor_tokens(
    body: Option<&RequestBody>,
    method: &str,
) -> proc_macro2::TokenStream {
    match body {
        Some(RequestBody {
            ty,
            content: RequestContent::Json,
        }) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::json::<#ty>())
        },
        Some(RequestBody {
            ty,
            content: RequestContent::Form,
        }) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::form::<#ty>())
        },
        None if matches!(method, "GET" | "HEAD" | "DELETE" | "OPTIONS") => {
            quote!(::std::option::Option::None)
        }
        // A body method read through the engine's own `Body<T>` extractor is
        // still a JSON body. Publish the free-form JSON schema rather than
        // claiming the operation accepts nothing.
        None => quote! {
            ::std::option::Option::Some(
                ::auto_route::OpenApiSchemaDescriptor::json::<
                    ::auto_route::__private::serde_json::Value,
                >(),
            )
        },
    }
}

pub(crate) fn response_schema_descriptor_tokens(
    body: Option<&ResponseBody>,
) -> proc_macro2::TokenStream {
    match body {
        Some(ResponseBody::Json(ty)) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::json::<#ty>())
        },
        Some(ResponseBody::Sse(Some(ty))) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::sse_json::<#ty>())
        },
        Some(ResponseBody::Sse(None)) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::sse())
        },
        // A handler that builds its reply dynamically still replies with JSON
        // in this API. Publish that as the book's free-form JSON schema instead
        // of leaving the operation undocumented.
        None => quote! {
            ::std::option::Option::Some(
                ::auto_route::OpenApiSchemaDescriptor::json::<
                    ::auto_route::__private::serde_json::Value,
                >(),
            )
        },
    }
}

pub(crate) fn param_descriptor_tokens(params: &[OpenApiParam]) -> proc_macro2::TokenStream {
    let params = params.iter().map(|param| {
        let name = LitStr::new(&param.name, proc_macro2::Span::call_site());
        let ty = &param.ty;
        match param.source {
            ParamSource::Path => quote!(::auto_route::OpenApiParamDescriptor::path::<#ty>(#name)),
            ParamSource::Query => quote!(::auto_route::OpenApiParamDescriptor::query::<#ty>(#name)),
        }
    });

    quote!(&[#(#params),*])
}
