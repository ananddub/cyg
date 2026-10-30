use syn::{FnArg, Ident, Type, TypePath};

pub(crate) fn validate_impl(item: &syn::ItemImpl) -> syn::Result<()> {
    if item.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            item,
            "#[auto_ws] requires an inherent impl block",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "generic socket impl blocks are not supported",
        ));
    }
    Ok(())
}

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

/// `&self` — the handler runs on the `Arc<Self>` the container resolved, so it
/// must not take the receiver by value.
pub(crate) fn validate_receiver(signature: &syn::Signature) -> syn::Result<()> {
    match signature.inputs.first() {
        Some(FnArg::Receiver(receiver))
            if receiver.reference.is_some()
                && receiver.mutability.is_none()
                && receiver.colon_token.is_none() =>
        {
            Ok(())
        }
        _ => Err(syn::Error::new_spanned(
            signature,
            "socket handlers must take &self as their first argument",
        )),
    }
}

/// The argument count after `&self`, so a mistyped handler is a sentence and
/// not a wall of inference errors.
pub(crate) fn validate_arity(
    signature: &syn::Signature,
    expected: usize,
    what: &str,
) -> syn::Result<()> {
    let count = signature.inputs.iter().skip(1).count();
    if count != expected {
        return Err(syn::Error::new_spanned(
            signature,
            format!("{what} must take exactly {expected} argument(s) after &self"),
        ));
    }
    Ok(())
}

pub(crate) fn type_ident(ty: &Type) -> syn::Result<&Ident> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return Err(syn::Error::new_spanned(
            ty,
            "socket handlers must be declared on a named type",
        ));
    };
    path.segments
        .last()
        .map(|segment| &segment.ident)
        .ok_or_else(|| {
            syn::Error::new_spanned(ty, "socket handlers must be declared on a named type")
        })
}
