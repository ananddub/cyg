use quote::quote;
use syn::{Item, ItemMod};

use crate::docs::{doc_comment_summary, merged_docs, route_method};
use crate::options::ControllerOptions;

pub(crate) fn expand_controller_module(
    controller_options: ControllerOptions,
    mut item_mod: ItemMod,
) -> syn::Result<proc_macro2::TokenStream> {
    let module_ident = item_mod.ident.clone();
    let controller_docs = merged_docs(
        &controller_options.docs,
        doc_comment_summary(&item_mod.attrs),
    );
    let Some((_, items)) = &mut item_mod.content else {
        return Err(syn::Error::new_spanned(
            &item_mod,
            "#[controller] requires an inline module: `mod name { ... }`",
        ));
    };
    let mut routes = Vec::new();

    for item in items.iter_mut() {
        let Item::Fn(function) = item else {
            continue;
        };
        let mut route_attributes = Vec::new();
        let mut retained = Vec::new();
        for attribute in std::mem::take(&mut function.attrs) {
            if let Some(method) = route_method(&attribute) {
                route_attributes.push((attribute, method));
            } else {
                retained.push(attribute);
            }
        }
        function.attrs = retained;

        if route_attributes.len() > 1 {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "a module route function can have only one route attribute",
            ));
        }
        let Some((attribute, method)) = route_attributes.pop() else {
            continue;
        };
        crate::controller::scan::collect_module_route(
            function,
            attribute,
            method,
            &controller_options,
            &mut routes,
        )?;
    }

    if routes.is_empty() {
        return Err(syn::Error::new_spanned(
            &module_ident,
            "controller module contains no route functions",
        ));
    }

    let registrations = routes.iter().map(|route| {
        let method = &route.method;
        let handler = &route.handler;
        let path = &route.path;
        quote! {
            router = router.route(
                #path,
                ::auto_route::__private::axum::routing::#method(#handler),
            );
        }
    });
    let generated: Item = syn::parse2(quote! {
        #[doc(hidden)]
        fn __auto_route_factory_module<'a>(
            _container: &'a ::auto_route::__private::auto_di::Container,
        ) -> ::auto_route::__private::auto_di::BoxFuture<
            'a,
            ::std::result::Result<
                ::auto_route::__private::axum::Router<()>,
                ::auto_route::__private::auto_di::DiError,
            >,
        > {
            ::std::boxed::Box::pin(async move {
                let mut router = ::auto_route::__private::axum::Router::new();
                #(#registrations)*
                Ok(router)
            })
        }
    })?;
    let submission: Item = syn::parse2(quote! {
        ::auto_route::__private::inventory::submit! {
            ::auto_route::RouteDescriptor::new(__auto_route_factory_module)
        }
    })?;
    let openapi_submissions = crate::controller::openapi::module_openapi_submissions(
        &routes,
        &module_ident,
        &controller_docs,
    );
    items.push(generated);
    items.push(submission);
    for submission in openapi_submissions {
        items.push(syn::parse2(submission)?);
    }
    Ok(quote!(#item_mod))
}
