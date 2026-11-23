use quote::{format_ident, quote};
use syn::Ident;

use crate::model::Route;

pub(crate) fn route_registrations(routes: &[Route]) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .map(|route| {
            let method = &route.method;
            let handler = &route.handler;
            let path = &route.route_path;
            let arguments = route
                .argument_types
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    let name = format_ident!("__auto_route_arg_{index}");
                    quote!(#name: #ty)
                })
                .collect::<Vec<_>>();
            let argument_names = (0..route.argument_types.len())
                .map(|index| format_ident!("__auto_route_arg_{index}"))
                .collect::<Vec<_>>();

            quote! {
                router = router.route(
                    #path,
                    ::auto_route::__private::axum::routing::#method({
                        let controller = ::std::sync::Arc::clone(&controller);
                        move |#(#arguments),*| {
                            let controller = ::std::sync::Arc::clone(&controller);
                            async move {
                                let response = controller.#handler(#(#argument_names),*).await;
                                ::auto_route::__private::axum::response::IntoResponse::into_response(response)
                            }
                        }
                    }),
                );
            }
        })
        .collect()
}

pub(crate) fn live_socket_registration(
    routes: &[Route],
    type_ident: &Ident,
) -> proc_macro2::TokenStream {
    if routes.iter().any(|route| route.live_event.is_some()) {
        let live_factory = format_ident!("__auto_route_live_factory_{}", type_ident);
        quote! {
            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #live_factory<'a>(
                _container: &'a ::auto_route::__private::auto_di::Container,
            ) -> ::auto_route::__private::auto_di::BoxFuture<'a, ::std::result::Result<::auto_route::__private::auto_socket::SocketRegistrar, ::auto_route::__private::auto_di::DiError>> {
                ::std::boxed::Box::pin(async move {
                    let registrar: ::auto_route::__private::auto_socket::SocketRegistrar =
                        ::std::sync::Arc::new(|_socket| {});
                    Ok(registrar)
                })
            }
            ::auto_route::__private::inventory::submit! {
                ::auto_route::__private::auto_socket::SocketDescriptor::new("/_cyg/live", #live_factory)
            }
        }
    } else {
        quote! {}
    }
}

pub(crate) fn live_table_registrations(
    routes: &[Route],
    type_ident: &Ident,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .filter_map(|route| {
            let mut tables = route.live_tables.clone();
            if let Some(table) = route.live_table.clone() {
                tables.push(table);
            }
            if tables.is_empty() {
                return None;
            }
            let handler = &route.handler;
            let endpoint = format!("{type_ident}::{handler}");
            Some(quote! {
                ::auto_route::__private::inventory::submit! {
                    ::auto_route::__private::auto_socket::LiveTableDescriptor::new(
                        #endpoint,
                        &[#(#tables),*],
                    )
                }
            })
        })
        .collect()
}
