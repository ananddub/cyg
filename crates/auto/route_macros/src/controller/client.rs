use quote::quote;
use syn::{Ident, LitStr};

use crate::live::{is_claims_type, is_live_server_arg, permission_types};
use crate::model::Route;
use crate::params::path_parameter_names;
use crate::types::wrapper_inner_type;

pub(crate) fn live_access_registrations(
    routes: &[Route],
    type_ident: &Ident,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .filter_map(|route| {
            let handler = &route.handler;
            let endpoint = format!("{type_ident}::{handler}");
            if let Some((resource, operation)) = route.argument_types.iter().find_map(permission_types)
            {
                let resource = quote!(stringify!(#resource));
                let operation = quote!(stringify!(#operation));
                return Some(quote! {
                    ::auto_route::__private::inventory::submit! {
                        ::auto_route::__private::auto_socket::LiveAccessDescriptor::permission(
                            #endpoint,
                            #resource,
                            #operation,
                        )
                    }
                });
            }
            if !route.argument_types.iter().any(is_claims_type) {
                return None;
            }
            Some(quote! {
                ::auto_route::__private::inventory::submit! {
                    ::auto_route::__private::auto_socket::LiveAccessDescriptor::authenticated(#endpoint)
                }
            })
        })
        .collect()
}

pub(crate) fn live_client_registrations(
    routes: &[Route],
    type_ident: &Ident,
) -> Vec<proc_macro2::TokenStream> {
    routes
        .iter()
        .filter_map(|route| {
            let event = route.live_event.as_ref()?;
            let client_name = route.live_client_name.as_ref()?;
            let handler = &route.handler;
            let endpoint = format!("{type_ident}::{handler}");
            let path = &route.path;
            let result_type = route.live_return_type.as_ref()?;
            let path_names = path_parameter_names(&path.value())
                .into_iter()
                .map(|name| LitStr::new(&name, path.span()))
                .collect::<Vec<_>>();
            let mut public_index = 0usize;
            let arguments = route
                .argument_types
                .iter()
                .filter_map(|ty| {
                    if is_live_server_arg(ty) {
                        return None;
                    }
                    let index = public_index;
                    public_index += 1;
                    if wrapper_inner_type(ty, &["Path"]).is_some() {
                        let inner = wrapper_inner_type(ty, &["Path"]).expect("path inner type");
                        return Some(quote!(::auto_route::LiveClientArgumentDescriptor::Path {
                            index: #index,
                            names: &[#(#path_names),*],
                            type_name: ::auto_route::ts_name::<#inner>,
                        }));
                    }
                    if wrapper_inner_type(ty, &["Query"]).is_some() {
                        let inner = wrapper_inner_type(ty, &["Query"]).expect("query inner type");
                        return Some(quote!(::auto_route::LiveClientArgumentDescriptor::Query {
                            index: #index,
                            type_name: ::auto_route::ts_name::<#inner>,
                        }));
                    }
                    None
                })
                .collect::<Vec<_>>();
            Some(quote! {
                ::auto_route::__private::inventory::submit! {
                    ::auto_route::LiveClientRouteDescriptor::new(
                        #client_name,
                        "/_cyg/live",
                        #endpoint,
                        #event,
                        #path,
                        &[#(#arguments),*],
                        ::auto_route::ts_name::<#result_type>,
                        ::auto_route::ts_decl::<#result_type>,
                    )
                }
            })
        })
        .collect()
}
