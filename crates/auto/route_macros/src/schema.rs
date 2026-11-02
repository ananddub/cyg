use syn::{FnArg, GenericArgument, ItemFn, PatType, PathArguments, ReturnType, Type};

use crate::model::{RequestBody, RequestContent, ResponseBody};
use crate::types::wrapper_inner_type;

pub(crate) fn validate_route_function(function: &ItemFn, label: &str) -> syn::Result<()> {
    crate::docs::validate_route_signature(&function.sig, label)
}

pub(crate) fn route_argument_types<'a>(inputs: impl Iterator<Item = &'a FnArg>) -> syn::Result<Vec<Type>> {
    inputs
        .map(|argument| match argument {
            FnArg::Typed(PatType { ty, .. }) => Ok((**ty).clone()),
            FnArg::Receiver(receiver) => Err(syn::Error::new_spanned(
                receiver,
                "module route functions cannot take self",
            )),
        })
        .collect()
}

pub(crate) fn infer_request_body(arguments: &[Type]) -> Option<RequestBody> {
    arguments.iter().find_map(|argument| {
        wrapper_inner_type(argument, &["Json", "ValidatedJson"])
            .map(|ty| RequestBody {
                ty,
                content: RequestContent::Json,
            })
            .or_else(|| {
                wrapper_inner_type(argument, &["Form"]).map(|ty| RequestBody {
                    ty,
                    content: RequestContent::Form,
                })
            })
    })
}

pub(crate) fn infer_response_body(output: &ReturnType) -> Option<ResponseBody> {
    match output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => response_body_type(ty),
    }
}

pub(crate) fn response_body_type(ty: &Type) -> Option<ResponseBody> {
    if is_sse_response_type(ty) {
        return Some(ResponseBody::Sse(None));
    }

    if let Some(inner) = wrapper_inner_type(ty, &["Json"]) {
        return Some(ResponseBody::Json(inner));
    }

    match ty {
        Type::Paren(paren) => response_body_type(&paren.elem),
        Type::Reference(reference) => response_body_type(&reference.elem),
        Type::Tuple(tuple) => tuple.elems.iter().find_map(response_body_type),
        Type::Path(type_path) => {
            let segment = type_path.path.segments.last()?;
            if segment.ident != "Result" && segment.ident != "Option" {
                return None;
            }
            let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                return None;
            };
            arguments.args.iter().find_map(|argument| match argument {
                GenericArgument::Type(ty) => response_body_type(ty),
                _ => None,
            })
        }
        _ => None,
    }
}

pub(crate) fn is_result_type(ty: &Type) -> bool {
    match ty {
        Type::Paren(paren) => is_result_type(&paren.elem),
        Type::Reference(reference) => is_result_type(&reference.elem),
        Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "Result"),
        _ => false,
    }
}

fn is_sse_response_type(ty: &Type) -> bool {
    match ty {
        Type::Paren(paren) => is_sse_response_type(&paren.elem),
        Type::Reference(reference) => is_sse_response_type(&reference.elem),
        Type::Path(type_path) => type_path.path.segments.last().is_some_and(|segment| {
            let ident = segment.ident.to_string();
            ident == "Sse"
                || ident.ends_with("Sse")
                || ident.ends_with("EventStream")
                || ident.ends_with("EventStreamResponse")
                || ident.ends_with("EventsStream")
                || ident.ends_with("EventsStreamResponse")
        }),
        _ => false,
    }
}
