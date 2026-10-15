use syn::{FnArg, GenericArgument, Ident, PatType, PathArguments, ReturnType, Type};

pub(crate) fn validate_async(signature: &syn::Signature) -> syn::Result<()> {
    if signature.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            signature,
            "socket handlers must be async",
        ));
    }
    if !signature.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &signature.generics,
            "generic socket handlers are not supported",
        ));
    }
    Ok(())
}
pub(crate) fn is_socket_argument(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.path.segments.last().is_some_and(|segment| matches!(segment.ident.to_string().as_str(), "SocketRef" | "Data")))
}
pub(crate) fn live_return_type(output: &ReturnType) -> syn::Result<Type> {
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(
            output,
            "#[live] methods must return a value",
        ));
    };
    Ok(wrapper_inner_type(ty, &["Json", "Result", "Option"]).unwrap_or_else(|| (**ty).clone()))
}
fn wrapper_inner_type(ty: &Type, wrappers: &[&str]) -> Option<Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if !wrappers.iter().any(|wrapper| segment.ident == *wrapper) {
        return None;
    }
    arguments.args.iter().find_map(|argument| match argument {
        GenericArgument::Type(inner) => Some(inner.clone()),
        _ => None,
    })
}
pub(crate) fn typed_arguments<'a>(
    inputs: impl Iterator<Item = &'a FnArg>,
) -> syn::Result<Vec<Type>> {
    inputs
        .map(|argument| match argument {
            FnArg::Typed(PatType { ty, .. }) => Ok((**ty).clone()),
            FnArg::Receiver(receiver) => Err(syn::Error::new_spanned(
                receiver,
                "unexpected receiver argument",
            )),
        })
        .collect()
}
pub(crate) fn type_ident(ty: &Type) -> syn::Result<&Ident> {
    let Type::Path(path) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "socket handler type must be a named type",
        ));
    };
    path.path
        .segments
        .last()
        .map(|segment| &segment.ident)
        .ok_or_else(|| syn::Error::new_spanned(ty, "socket handler type must be a named type"))
}
