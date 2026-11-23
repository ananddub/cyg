use quote::quote;
use syn::{LitStr, Type};

use crate::model::{OpenApiParam, ParamSource, RequestBody, RequestContent, ResponseBody};

pub(crate) fn request_schema_descriptor_tokens(
    body: Option<&RequestBody>,
    _method: &str,
) -> proc_macro2::TokenStream {
    match body {
        Some(RequestBody {
            ty,
            content: RequestContent::Json,
        }) if is_json_value(ty) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::free_form_json())
        },
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
        Some(RequestBody {
            ty,
            content: RequestContent::Text,
        }) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::text::<#ty>())
        },
        Some(RequestBody {
            ty,
            content: RequestContent::Multipart,
        }) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::multipart::<#ty>())
        },
        // No extractor, no body: a custom method that only names the resource
        // in its path does not accept a JSON payload. Publishing a schema here
        // told clients to send one.
        None => quote!(::std::option::Option::None),
    }
}

pub(crate) fn response_schema_descriptor_tokens(
    body: Option<&ResponseBody>,
) -> proc_macro2::TokenStream {
    match body {
        Some(ResponseBody::Json(ty)) if is_json_value(ty) => quote! {
            ::std::option::Option::Some(::auto_route::OpenApiSchemaDescriptor::free_form_json())
        },
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
                ::auto_route::OpenApiSchemaDescriptor::free_form_json(),
            )
        },
    }
}

fn is_json_value(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Path(path)
            if path.qself.is_none()
                && path.path.segments.last().is_some_and(|segment| {
                    segment.ident == "Value"
                        && matches!(segment.arguments, syn::PathArguments::None)
                })
    )
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
