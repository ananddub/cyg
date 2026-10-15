use quote::quote;
use syn::{GenericArgument, Ident, PathArguments, Type};

use crate::types::wrapper_inner_type;

pub(crate) fn live_argument_type(ty: &Type) -> Type {
    wrapper_inner_type(ty, &["Path", "Query", "Json", "Form"]).unwrap_or_else(|| ty.clone())
}

pub(crate) fn is_claims_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "Claims"))
}

fn is_permission_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "RequirePermission"))
}

pub(crate) fn permission_types(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != "RequirePermission" {
        return None;
    }
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    Some((types.next()?, types.next()?))
}

pub(crate) fn is_live_auth_type(ty: &Type) -> bool {
    is_claims_type(ty) || is_permission_type(ty)
}

pub(crate) fn is_live_server_arg(ty: &Type) -> bool {
    is_live_auth_type(ty)
        || matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| {
            matches!(segment.ident.to_string().as_str(), "Extension" | "State" | "ConnectInfo" | "Request")
        }))
}

pub(crate) fn live_auth_user_id(name: &Ident, ty: &Type) -> proc_macro2::TokenStream {
    if is_permission_type(ty) {
        quote!(#name.0.user.user_id)
    } else {
        quote!(#name.user.user_id)
    }
}
