use quote::{format_ident, quote};
use syn::{ItemImpl, LitStr};

use crate::helpers::{type_ident, validate_impl};

mod collect;

use collect::collect;

pub(crate) fn expand(
    path: LitStr,
    mut item_impl: ItemImpl,
) -> syn::Result<proc_macro2::TokenStream> {
    validate_impl(&item_impl)?;
    let self_ty = item_impl.self_ty.as_ref().clone();
    let type_name = type_ident(&self_ty)?;
    let factory = format_ident!("__auto_ws_factory_{}", type_name);

    let has_singleton = item_impl.attrs.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "singleton")
    });
    let (mut events, mut connects, mut disconnects) = (Vec::new(), Vec::new(), Vec::new());
    collect(&mut item_impl, &mut events, &mut connects, &mut disconnects)?;
    if events.is_empty() && connects.is_empty() && disconnects.is_empty() {
        return Err(syn::Error::new_spanned(
            &item_impl.self_ty,
            "socket declaration contains no #[on], #[connect] or #[disconnect] methods",
        ));
    }

    let managed_impl = if has_singleton {
        quote!(#item_impl)
    } else {
        quote! {
            #[::auto_ws::__private::auto_di::singleton]
            #item_impl
        }
    };

    let event_inserts = events.iter().map(|event| {
        let name = &event.name;
        let method = &event.handler;
        quote! {
            {
                let controller = ::std::clone::Clone::clone(&controller);
                registrar.events.insert(
                    #name,
                    ::std::sync::Arc::new(
                        move |conn: ::auto_ws::Conn, msg: ::auto_ws::Msg| {
                            let controller = ::std::clone::Clone::clone(&controller);
                            let handled = async move {
                                let out = controller.#method(&conn, &msg).await;
                                ::auto_ws::reply(out)
                            };
                            ::std::boxed::Box::pin(handled) as ::auto_ws::BoxFuture<::auto_ws::Reply>
                        },
                    ),
                );
            }
        }
    });
    let connect_inserts = connects.iter().map(|hook| {
        let method = &hook.handler;
        quote! {
            {
                let controller = ::std::clone::Clone::clone(&controller);
                registrar.connects.push(::std::sync::Arc::new(
                    move |conn: ::auto_ws::Conn| {
                        let controller = ::std::clone::Clone::clone(&controller);
                        let opened = async move { controller.#method(&conn).await };
                        ::std::boxed::Box::pin(opened) as ::auto_ws::BoxFuture<()>
                    },
                ));
            }
        }
    });
    let disconnect_inserts = disconnects.iter().map(|hook| {
        let method = &hook.handler;
        quote! {
            {
                let controller = ::std::clone::Clone::clone(&controller);
                registrar.disconnects.push(::std::sync::Arc::new(
                    move |conn: ::auto_ws::Conn| {
                        let controller = ::std::clone::Clone::clone(&controller);
                        let closed = async move { controller.#method(&conn).await };
                        ::std::boxed::Box::pin(closed) as ::auto_ws::BoxFuture<()>
                    },
                ));
            }
        }
    });

    Ok(quote! {
        #managed_impl

        #[doc(hidden)]
        #[allow(non_snake_case)]
        fn #factory<'a>(
            container: &'a ::auto_ws::__private::auto_di::Container,
        ) -> ::auto_ws::__private::auto_di::BoxFuture<
            'a,
            ::std::result::Result<::auto_ws::SocketRegistrar, ::auto_ws::__private::auto_di::DiError>,
        > {
            ::std::boxed::Box::pin(async move {
                let controller = container.resolve::<#self_ty>().await?;
                let mut registrar = ::auto_ws::SocketRegistrar::default();
                #(#event_inserts)*
                #(#connect_inserts)*
                #(#disconnect_inserts)*
                ::std::result::Result::Ok(registrar)
            })
        }

        ::auto_ws::__private::inventory::submit! {
            ::auto_ws::SocketDescriptor::new(#path, #factory)
        }
    })
}
