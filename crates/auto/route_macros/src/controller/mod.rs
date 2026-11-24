use quote::{format_ident, quote};
use syn::ItemImpl;

use crate::docs::{doc_comment_summary, type_ident};
use crate::options::ControllerOptions;

mod client;
pub(crate) mod openapi;
mod publishers;
mod refresh;
mod registrations;
pub(crate) mod scan;

pub(crate) fn expand_controller(
    controller_options: ControllerOptions,
    mut item_impl: ItemImpl,
) -> syn::Result<proc_macro2::TokenStream> {
    if item_impl.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            &item_impl,
            "#[controller] requires an inherent impl block",
        ));
    }
    if !item_impl.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl.generics,
            "generic controller impl blocks are not supported",
        ));
    }

    let self_ty = item_impl.self_ty.as_ref().clone();
    let controller_docs = merged_controller_docs(&controller_options, &item_impl);
    let has_singleton = item_impl.attrs.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "singleton")
    });
    let type_ident = type_ident(&self_ty)?;
    let factory_ident = format_ident!("__auto_route_factory_{}", type_ident);

    let controller_path = controller_options.path.value();
    // A controller whose path opens with an API version is part of the versioned
    // tree: its routes register without that segment and the plane mounts them
    // under the canonical base (and any deprecated alias). Everything else —
    // `/status`, `/ws`, `/dashboard/*` — stays where it is, outside the version.
    let versioned = crate::docs::api_version(&controller_path).is_some();
    let routes = scan::scan_impl_routes(&mut item_impl.items, &controller_path)?;

    if routes.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "controller contains no route methods",
        ));
    }

    let path_const_names: Vec<_> = routes
        .iter()
        .map(|r| format_ident!("__PATH_{}", r.handler))
        .collect();
    let path_const_values: Vec<_> = routes.iter().map(|r| &r.path).collect();

    let controller_name = type_ident.to_string();
    let module_name = controller_name
        .strip_suffix("Controller")
        .unwrap_or(&controller_name)
        .to_ascii_lowercase();
    let live_module_ident = format_ident!("{}_live", module_name);

    let controller_publishers = publishers::controller_publishers(&routes, type_ident);
    let live_handles = publishers::live_handles(&routes, type_ident);
    let registrations = registrations::route_registrations(&routes);
    let openapi_submissions =
        openapi::controller_openapi_submissions(&routes, type_ident, &controller_docs);
    let live_socket_registration = registrations::live_socket_registration(&routes, type_ident);
    let live_refresh_registrations =
        refresh::live_refresh_registrations(&routes, type_ident, &self_ty);
    let live_table_registrations = registrations::live_table_registrations(&routes, type_ident);
    let live_access_registrations = client::live_access_registrations(&routes, type_ident);
    let live_client_registrations = client::live_client_registrations(&routes, type_ident);

    let managed_impl = if has_singleton {
        quote!(#item_impl)
    } else {
        quote! {
            #[auto_route::__private::auto_di::singleton]
            #item_impl
        }
    };

    Ok(quote! {
        #managed_impl

        impl #self_ty {
            #(#controller_publishers)*
        }

        pub mod #live_module_ident {
            use super::*;
            #(#live_handles)*
        }

        #live_socket_registration
        #(#live_refresh_registrations)*
        #(#live_table_registrations)*
        #(#live_access_registrations)*
        #(#live_client_registrations)*

        #[allow(non_upper_case_globals)]
        impl #self_ty {
            #(
                #[doc(hidden)]
                pub const #path_const_names: &'static str = #path_const_values;
            )*
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #factory_ident<'a>(
            container: &'a ::auto_route::__private::auto_di::Container,
        ) -> ::auto_route::__private::auto_di::BoxFuture<
            'a,
            ::std::result::Result<
                ::auto_route::__private::axum::Router<()>,
                ::auto_route::__private::auto_di::DiError,
            >,
        > {
            ::std::boxed::Box::pin(async move {
                let controller = container.resolve::<#self_ty>().await?;
                let mut router = ::auto_route::__private::axum::Router::new();
                #(#registrations)*
                Ok(router)
            })
        }

        ::auto_route::__private::inventory::submit! {
            ::auto_route::RouteDescriptor::new(#factory_ident, #versioned)
        }

        #(#openapi_submissions)*
    })
}

fn merged_controller_docs(
    controller_options: &ControllerOptions,
    item_impl: &ItemImpl,
) -> crate::options::Docs {
    crate::docs::merged_docs(
        &controller_options.docs,
        doc_comment_summary(&item_impl.attrs),
    )
}
