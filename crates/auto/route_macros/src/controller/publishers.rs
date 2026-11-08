use quote::{format_ident, quote};
use syn::Ident;

use crate::live::{is_live_auth_type, is_live_server_arg, live_argument_type, live_auth_user_id};
use crate::model::{LiveStrategy, Route};

pub(crate) fn strategy_tokens(route: &Route) -> proc_macro2::TokenStream {
    match route.live_strategy {
        LiveStrategy::Sqlite => quote!(::auto_route::__private::auto_socket::LiveStrategy::Sqlite),
        LiveStrategy::Publish => {
            quote!(::auto_route::__private::auto_socket::LiveStrategy::Publish)
        }
        LiveStrategy::Latest => quote!(::auto_route::__private::auto_socket::LiveStrategy::Latest),
        LiveStrategy::Stream => {
            let capacity = route
                .live_capacity
                .as_ref()
                .map(|value| quote!(#value))
                .unwrap_or_else(|| quote!(256usize));
            let replay = route
                .live_replay
                .as_ref()
                .map(|value| quote!(#value))
                .unwrap_or_else(|| quote!(0usize));
            quote!(::auto_route::__private::auto_socket::LiveStrategy::Stream { capacity: #capacity, replay: #replay })
        }
    }
}

pub(crate) fn controller_publishers(
    routes: &[Route],
    type_ident: &Ident,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .filter_map(|route| {
            let event = route.live_event.as_ref()?;
            let client_name = route.live_client_name.as_ref()?;
            let return_type = route.live_return_type.as_ref()?;
            let handler = &route.handler;
            if client_name.value() == handler.to_string() {
                return None;
            }
            let endpoint = format!("{type_ident}::{handler}");
            let arguments = route.argument_types.iter().enumerate().map(|(index, ty)| {
                let name = format_ident!("arg_{index}");
                if is_live_server_arg(ty) { quote!(#name: &#ty) } else { let public_ty = live_argument_type(ty); quote!(#name: #public_ty) }
            }).collect::<Vec<_>>();
            let names = route.argument_types.iter().enumerate().filter(|(_, ty)| !is_live_server_arg(ty)).map(|(index, _)| format_ident!("arg_{index}")).collect::<Vec<_>>();
            let claims = route.argument_types.iter().enumerate().find(|(_, ty)| is_live_auth_type(ty)).map(|(index, ty)| (format_ident!("arg_{index}"), ty));
            let args = match names.len() { 0 => quote!(::std::vec::Vec::<()>::new()), 1 => { let n = &names[0]; quote!((#n,)) }, _ => quote!((#(#names),*)) };
            let scope = claims.map(|(claims, ty)| { let user_id = live_auth_user_id(&claims, ty); quote!(.user(#user_id)) }).unwrap_or_default();
            let client_arguments = route.argument_types.iter().enumerate().filter(|(_, ty)| !is_live_server_arg(ty)).map(|(index, ty)| { let name = format_ident!("arg_{index}"); let public_ty = live_argument_type(ty); quote!(#name: #public_ty) }).collect::<Vec<_>>();
            let publisher = format_ident!("{}", client_name.value());
            let subscription = format_ident!("{}_subscription", client_name.value());
            let strategy = strategy_tokens(route);
            Some(quote! {
                pub fn #publisher(#(#arguments),*) -> ::std::result::Result<::auto_route::__private::auto_socket::LivePublisher<#return_type>, ::auto_route::__private::auto_socket::PublishError> {
                    ::auto_route::__private::auto_socket::LivePublisher::new("/_cyg/live", #endpoint, #event).strategy(#strategy)#scope.room(#args)
                }

                pub fn #subscription(#(#client_arguments),*) -> ::std::result::Result<::auto_route::__private::auto_socket::LiveSubscription<#return_type>, ::auto_route::__private::auto_socket::PublishError> {
                    ::auto_route::__private::auto_socket::LiveSubscription::new("/_cyg/live", #endpoint, #event, #client_name, #args)
                }
            })
        })
        .collect()
}

pub(crate) fn live_handles(routes: &[Route], type_ident: &Ident) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .filter_map(|route| {
            let event = route.live_event.as_ref()?;
            let client_name = route.live_client_name.as_ref()?;
            let return_type = route.live_return_type.as_ref()?;
            let handler = &route.handler;
            let endpoint = format!("{type_ident}::{handler}");
            let arguments = route
                .argument_types
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    let name = format_ident!("arg_{index}");
                    if is_live_server_arg(ty) { quote!(#name: &#ty) } else { let public_ty = live_argument_type(ty); quote!(#name: #public_ty) }
                })
                .collect::<Vec<_>>();
            let names = route.argument_types.iter().enumerate().filter(|(_, ty)| !is_live_server_arg(ty)).map(|(index, _)| format_ident!("arg_{index}")).collect::<Vec<_>>();
            let claims = route.argument_types.iter().enumerate().find(|(_, ty)| is_live_auth_type(ty)).map(|(index, ty)| (format_ident!("arg_{index}"), ty));
            let scope = claims.map(|(claims, ty)| { let user_id = live_auth_user_id(&claims, ty); quote!(.user(#user_id)) }).unwrap_or_default();
            let client_arguments = route.argument_types.iter().enumerate().filter(|(_, ty)| !is_live_server_arg(ty)).map(|(index, ty)| { let name = format_ident!("arg_{index}"); let public_ty = live_argument_type(ty); quote!(#name: #public_ty) }).collect::<Vec<_>>();
            let args = match names.len() {
                0 => quote!(::std::vec::Vec::<()>::new()),
                1 => { let n = &names[0]; quote!((#n,)) },
                _ => quote!((#(#names),*)),
            };
            let event_handler = format_ident!("{}_event", handler);
            let subscription = format_ident!("{}_subscription", handler);
            let strategy = strategy_tokens(route);
            Some(quote! {
                pub fn #handler(#(#arguments),*)
                    -> ::std::result::Result<
                        ::auto_route::__private::auto_socket::LivePublisher<#return_type>,
                        ::auto_route::__private::auto_socket::PublishError,
                    >
                {
                    ::auto_route::__private::auto_socket::LivePublisher::new(
                        "/_cyg/live",
                        #endpoint,
                        #event,
                    ).strategy(#strategy)#scope.room(#args)
                }
                pub fn #event_handler() -> ::auto_route::__private::auto_socket::LivePublisher<#return_type> {
                    ::auto_route::__private::auto_socket::LivePublisher::new(
                        "/_cyg/live", #endpoint, #event,
                    ).strategy(#strategy)
                }
                pub fn #subscription(#(#client_arguments),*) -> ::std::result::Result<::auto_route::__private::auto_socket::LiveSubscription<#return_type>, ::auto_route::__private::auto_socket::PublishError> {
                    ::auto_route::__private::auto_socket::LiveSubscription::new("/_cyg/live", #endpoint, #event, #client_name, #args)
                }
            })
        })
        .collect()
}
