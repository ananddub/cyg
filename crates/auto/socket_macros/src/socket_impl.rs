use crate::{
    helpers::{is_socket_argument, live_return_type, type_ident, typed_arguments, validate_async},
    model::{Event, LiveEvent},
};
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ItemImpl, LitStr};

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

fn collect_events(
    item_impl: &mut ItemImpl,
    events: &mut Vec<Event>,
    live_events: &mut Vec<LiveEvent>,
) -> syn::Result<()> {
    for item in &mut item_impl.items {
        let ImplItem::Fn(function) = item else {
            continue;
        };
        let mut on = Vec::new();
        let mut live = false;
        function.attrs.retain(|attribute| {
            if attribute.path().is_ident("on") {
                on.push(attribute.clone());
                false
            } else if attribute.path().is_ident("live") {
                live = true;
                false
            } else {
                true
            }
        });
        if on.len() > 1 {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "a socket method can have only one #[on] attribute",
            ));
        }
        let on = on.pop();
        if !live && on.is_none() {
            continue;
        }
        validate_async(&function.sig)?;
        let mut inputs = function.sig.inputs.iter();
        match inputs.next() {
            Some(FnArg::Receiver(receiver))
                if receiver.reference.is_some() && receiver.mutability.is_none() => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    &function.sig,
                    "socket event methods must take &self as their first argument",
                ));
            }
        }
        let argument_types = typed_arguments(inputs)?;
        if live || !argument_types.iter().any(is_socket_argument) {
            let name = match on {
                Some(attribute) => attribute.parse_args::<LitStr>()?,
                None => LitStr::new(&function.sig.ident.to_string(), function.sig.ident.span()),
            };
            live_events.push(LiveEvent {
                name,
                handler: function.sig.ident.clone(),
                return_type: live_return_type(&function.sig.output)?,
                argument_types,
            });
        } else {
            events.push(Event {
                name: on.unwrap().parse_args::<LitStr>()?,
                handler: function.sig.ident.clone(),
                argument_types,
            });
        }
    }
    Ok(())
}

fn live_handle(
    event: &LiveEvent,
    namespace: &LitStr,
    type_ident: &syn::Ident,
) -> proc_macro2::TokenStream {
    let handler = &event.handler;
    let event_name = &event.name;
    let return_type = &event.return_type;
    let endpoint = format!("{type_ident}::{handler}");
    let arguments = event
        .argument_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let name = format_ident!("arg_{index}");
            quote!(#name: #ty)
        })
        .collect::<Vec<_>>();
    let names = (0..event.argument_types.len())
        .map(|index| format_ident!("arg_{index}"))
        .collect::<Vec<_>>();
    let args = match names.len() {
        0 => quote!(::std::vec::Vec::<()>::new()),
        1 => {
            let name = &names[0];
            quote!((#name,))
        }
        _ => quote!((#(#names),*)),
    };
    let event_handler = format_ident!("{}_event", handler);
    let subscription = format_ident!("{}_subscription", handler);
    quote! {
        pub fn #handler(#(#arguments),*) -> ::std::result::Result<::auto_socket::LivePublisher<#return_type>, ::auto_socket::PublishError> { ::auto_socket::LivePublisher::new(#namespace, #endpoint, #event_name).room(#args) }
        pub fn #event_handler() -> ::auto_socket::LivePublisher<#return_type> { ::auto_socket::LivePublisher::new(#namespace, #endpoint, #event_name) }
        pub fn #subscription(#(#arguments),*) -> ::std::result::Result<::auto_socket::LiveSubscription<#return_type>, ::auto_socket::PublishError> { ::auto_socket::LiveSubscription::new(#namespace, #endpoint, #event_name, stringify!(#handler), #args) }
    }
}

fn event_registration(event: &Event) -> proc_macro2::TokenStream {
    let name = &event.name;
    let handler = &event.handler;
    let arguments = event
        .argument_types
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let arg = format_ident!("__auto_socket_arg_{index}");
            quote!(#arg: #ty)
        })
        .collect::<Vec<_>>();
    let names = (0..event.argument_types.len())
        .map(|index| format_ident!("__auto_socket_arg_{index}"))
        .collect::<Vec<_>>();
    quote! { socket.on(#name, { let controller = ::std::sync::Arc::clone(&controller); move |#(#arguments),*| { let controller = ::std::sync::Arc::clone(&controller); async move { controller.#handler(#(#names),*).await } } }); }
}
