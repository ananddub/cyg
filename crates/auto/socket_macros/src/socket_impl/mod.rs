use quote::{format_ident, quote};
use syn::{ItemImpl, LitStr};

use crate::helpers::type_ident;

mod collect;
mod emit;

use collect::collect_events;
use emit::{event_registration, live_handle};

pub(crate) fn expand(
    namespace: LitStr,
    mut item_impl: ItemImpl,
) -> syn::Result<proc_macro2::TokenStream> {
    validate_impl(&item_impl)?;
    let self_ty = item_impl.self_ty.as_ref().clone();
    let type_ident = type_ident(&self_ty)?;
    let factory = format_ident!("__auto_socket_factory_{}", type_ident);
    let (mut events, mut live_events) = (Vec::new(), Vec::new());
    collect_events(&mut item_impl, &mut events, &mut live_events)?;
    if events.is_empty() && live_events.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "socket handler contains no #[on] or #[live] methods",
        ));
    }
    let registrations = events.iter().map(event_registration);
    let controller_name = type_ident.to_string();
    let live_module = format_ident!(
        "{}_live",
        controller_name
            .strip_suffix("Socket")
            .unwrap_or(&controller_name)
            .to_ascii_lowercase()
    );
    let live_handles = live_events
        .iter()
        .map(|event| live_handle(event, &namespace, type_ident));
    Ok(quote! {
        #item_impl
        pub mod #live_module { use super::*; #(#live_handles)* }
        #[doc(hidden)] #[allow(non_snake_case)]
        fn #factory<'a>(container: &'a ::auto_socket::__private::auto_di::Container) -> ::auto_socket::__private::auto_di::BoxFuture<'a, ::std::result::Result<::auto_socket::SocketRegistrar, ::auto_socket::__private::auto_di::DiError>> {
            ::std::boxed::Box::pin(async move {
                let controller = container.resolve::<#self_ty>().await?;
                let registrar: ::auto_socket::SocketRegistrar = ::std::sync::Arc::new(move |socket| { #(#registrations)* });
                Ok(registrar)
            })
        }
        ::auto_socket::__private::inventory::submit! { ::auto_socket::SocketDescriptor::new(#namespace, #factory) }
    })
}

fn validate_impl(item: &ItemImpl) -> syn::Result<()> {
    if item.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            item,
            "#[auto_socket] requires an inherent impl block",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "generic socket handler impl blocks are not supported",
        ));
    }
    Ok(())
}
