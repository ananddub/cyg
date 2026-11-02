use crate::model::{Event, LiveEvent};
use quote::{format_ident, quote};
use syn::LitStr;

pub(super) fn live_handle(
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

pub(super) fn event_registration(event: &Event) -> proc_macro2::TokenStream {
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
