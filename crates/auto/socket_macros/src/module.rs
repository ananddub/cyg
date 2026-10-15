use crate::helpers::validate_async;
use quote::quote;
use syn::{Item, ItemMod, LitStr};

pub(crate) fn expand(
    namespace: LitStr,
    mut item_mod: ItemMod,
) -> syn::Result<proc_macro2::TokenStream> {
    let module_ident = item_mod.ident.clone();
    let Some((_, items)) = &mut item_mod.content else {
        return Err(syn::Error::new_spanned(
            &item_mod,
            "#[auto_socket] requires an inline module: `mod name { ... }`",
        ));
    };
    let mut events = Vec::new();
    for item in items.iter_mut() {
        let Item::Fn(function) = item else {
            continue;
        };
        let mut found = Vec::new();
        function.attrs.retain(|attribute| {
            if attribute.path().is_ident("on") {
                found.push(attribute.clone());
                false
            } else {
                true
            }
        });
        if found.len() > 1 {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "a module socket function can have only one #[on] attribute",
            ));
        }
        let Some(attribute) = found.pop() else {
            continue;
        };
        validate_async(&function.sig)?;
        events.push((
            attribute.parse_args::<LitStr>()?,
            function.sig.ident.clone(),
        ));
    }
    if events.is_empty() {
        return Err(syn::Error::new_spanned(
            &module_ident,
            "socket module contains no #[on] functions",
        ));
    }
    let registrations = events
        .iter()
        .map(|(event, handler)| quote! { socket.on(#event, #handler); });
    let generated: Item = syn::parse2(quote! {
        #[doc(hidden)] fn __auto_socket_factory_module<'a>(_: &'a ::auto_socket::__private::auto_di::Container) -> ::auto_socket::__private::auto_di::BoxFuture<'a, ::std::result::Result<::auto_socket::SocketRegistrar, ::auto_socket::__private::auto_di::DiError>> {
            ::std::boxed::Box::pin(async move { let registrar: ::auto_socket::SocketRegistrar = ::std::sync::Arc::new(move |socket| { #(#registrations)* }); Ok(registrar) })
        }
    })?;
    let submission: Item = syn::parse2(
        quote! { ::auto_socket::__private::inventory::submit! { ::auto_socket::SocketDescriptor::new(#namespace, __auto_socket_factory_module) } },
    )?;
    items.push(generated);
    items.push(submission);
    Ok(quote!(#item_mod))
}
