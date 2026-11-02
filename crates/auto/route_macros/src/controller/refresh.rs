use quote::{format_ident, quote};
use syn::{Ident, Type};

use super::publishers::strategy_tokens;
use crate::live::{is_live_server_arg, live_argument_type};
use crate::model::Route;
use crate::types::wrapper_inner_type;

pub(crate) fn live_refresh_registrations(
    routes: &[Route],
    type_ident: &Ident,
    self_ty: &Type,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .flat_map(|route| {
            let mut tables = route.live_tables.clone();
            if let Some(table) = route.live_table.clone() {
                tables.push(table);
            }
            if tables.is_empty() {
                return Vec::new();
            }
            let Some(event) = route.live_event.as_ref() else { return Vec::new(); };
            let Some(return_type) = route.live_return_type.as_ref() else { return Vec::new(); };
            let handler = &route.handler;
            if route.argument_types.iter().any(is_live_server_arg)
                || route.argument_types.iter().any(|ty| {
                    wrapper_inner_type(ty, &["Path", "Query"]).is_none()
                })
            {
                return Vec::new();
            }
            let endpoint = format!("{type_ident}::{handler}");
            let strategy = strategy_tokens(route);
            let decoded_names = route.argument_types.iter().enumerate().map(|(index, _)| format_ident!("live_arg_{index}")).collect::<Vec<_>>();
            let decoded_types = route.argument_types.iter().map(live_argument_type).collect::<Vec<_>>();
            let decode_args = match decoded_types.len() {
                0 => quote! {},
                1 => {
                    let name = &decoded_names[0];
                    let ty = &decoded_types[0];
                    quote! {
                        let ::std::result::Result::Ok((#name,)) = ::auto_route::__private::serde_json::from_value::<(#ty,)>(args.clone()) else { return; };
                    }
                }
                _ => quote! {
                    let ::std::result::Result::Ok((#(#decoded_names),*)) = ::auto_route::__private::serde_json::from_value::<(#(#decoded_types),*)>(args.clone()) else { return; };
                },
            };
            let handler_args = route.argument_types.iter().zip(decoded_names.iter()).map(|(ty, name)| {
                if wrapper_inner_type(ty, &["Path"]).is_some() {
                    quote!(::auto_route::__private::axum::extract::Path(#name))
                } else {
                    quote!(::auto_route::__private::axum::extract::Query(#name))
                }
            }).collect::<Vec<_>>();
            let await_handler = if route.live_returns_result {
                quote! {
                    let ::std::result::Result::Ok(::auto_route::__private::axum::Json(data)) = controller.#handler(#(#handler_args),*).await else { return; };
                }
            } else {
                quote! {
                    let ::auto_route::__private::axum::Json(data): ::auto_route::__private::axum::Json<#return_type> = controller.#handler(#(#handler_args),*).await;
                }
            };
            let refresh_factory = format_ident!("__auto_route_live_refresh_{}_{}", type_ident, handler);
            vec![quote! {
                #[doc(hidden)]
                #[allow(non_snake_case)]
                fn #refresh_factory<'a>(
                    container: &'a ::auto_route::__private::auto_di::Container,
                ) -> ::auto_route::__private::auto_di::BoxFuture<'a, ::std::result::Result<::std::sync::Arc<dyn Fn(::auto_route::__private::serde_json::Value, ::std::option::Option<::auto_route::__private::auto_socket::LiveIdentity>) -> ::auto_route::__private::auto_di::BoxFuture<'static, ()> + Send + Sync + 'static>, ::auto_route::__private::auto_di::DiError>> {
                    ::std::boxed::Box::pin(async move {
                        let controller = container.resolve::<#self_ty>().await?;
                        let refresh: ::std::sync::Arc<dyn Fn(::auto_route::__private::serde_json::Value, ::std::option::Option<::auto_route::__private::auto_socket::LiveIdentity>) -> ::auto_route::__private::auto_di::BoxFuture<'static, ()> + Send + Sync + 'static> = ::std::sync::Arc::new(move |args, identity| {
                            let controller = ::std::sync::Arc::clone(&controller);
                            let future: ::auto_route::__private::auto_di::BoxFuture<'static, ()> = ::std::boxed::Box::pin(async move {
                                if let ::std::option::Option::Some(data) = ::auto_route::__private::auto_socket::cached_live_value(
                                    #endpoint,
                                    &args,
                                    identity.as_ref(),
                                ).await {
                                    ::auto_route::__private::tracing::info!(endpoint = #endpoint, args = ?args, "live refresh cache hit");
                                    let publisher = ::auto_route::__private::auto_socket::LivePublisher::new("/_cyg/live", #endpoint, #event).strategy(#strategy);
                                    let publisher = if let ::std::option::Option::Some(identity) = identity { publisher.user(identity.user_id) } else { publisher };
                                    if let Ok(publisher) = publisher.room(args) {
                                        if let Err(error) = publisher.publish(data).await {
                                            ::auto_route::__private::tracing::warn!(endpoint = #endpoint, error = %error, "live cached publish failed");
                                        }
                                    }
                                    return;
                                }
                                #decode_args
                                ::auto_route::__private::tracing::info!(endpoint = #endpoint, args = ?args, "live refresh handler started");
                                #await_handler
                                if let ::std::result::Result::Ok(value) = ::auto_route::__private::serde_json::to_value(&data) {
                                    ::auto_route::__private::auto_socket::cache_live_value(
                                        #endpoint,
                                        &args,
                                        identity.as_ref(),
                                        value,
                                    ).await;
                                }
                                let publisher = ::auto_route::__private::auto_socket::LivePublisher::new("/_cyg/live", #endpoint, #event).strategy(#strategy);
                                let publisher = if let ::std::option::Option::Some(identity) = identity { publisher.user(identity.user_id) } else { publisher };
                                if let Ok(publisher) = publisher.room(args) {
                                    if let Err(error) = publisher.publish(data).await {
                                        ::auto_route::__private::tracing::warn!(endpoint = #endpoint, error = %error, "live fresh publish failed");
                                    } else {
                                        ::auto_route::__private::tracing::info!(endpoint = #endpoint, "live fresh publish completed");
                                    }
                                }
                            });
                            future
                        });
                        Ok(refresh)
                    })
                }
                    ::auto_route::__private::inventory::submit! {
                    ::auto_route::__private::auto_socket::LiveRefreshDescriptor::new(
                        #endpoint,
                        #refresh_factory,
                    )
                }
            }]
        })
        .collect()
}
