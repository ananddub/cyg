use quote::{format_ident, quote};
use syn::{AttrStyle, ImplItem, ItemImpl, LitStr};

use crate::helpers::{
    type_ident, validate_arity, validate_async, validate_impl, validate_receiver,
};

struct Event {
    name: LitStr,
    handler: syn::Ident,
}

struct Hook {
    handler: syn::Ident,
}

pub(crate) fn expand(path: LitStr, mut item_impl: ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    validate_impl(&item_impl)?;
    let self_ty = item_impl.self_ty.as_ref().clone();
    let type_name = type_ident(&self_ty)?;
    let factory = format_ident!("__auto_ws_factory_{}", type_name);
    // Like `#[controller]`: a socket is built by `auto-di`, and the attribute
    // registers the provider itself so the declaration stays one attribute.
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

fn collect(
    item_impl: &mut ItemImpl,
    events: &mut Vec<Event>,
    connects: &mut Vec<Hook>,
    disconnects: &mut Vec<Hook>,
) -> syn::Result<()> {
    for item in &mut item_impl.items {
        let ImplItem::Fn(function) = item else {
            continue;
        };
        let mut on = None;
        let mut connect = false;
        let mut disconnect = false;
        function.attrs.retain(|attribute| {
            if !matches!(attribute.style, AttrStyle::Outer) {
                return true;
            }
            if attribute.path().is_ident("on") {
                on = Some(attribute.clone());
                false
            } else if attribute.path().is_ident("connect") {
                connect = true;
                false
            } else if attribute.path().is_ident("disconnect") {
                disconnect = true;
                false
            } else {
                true
            }
        });
        let markers = usize::from(connect) + usize::from(disconnect) + usize::from(on.is_some());
        if markers == 0 {
            continue;
        }
        if markers > 1 {
            return Err(syn::Error::new_spanned(
                &function.sig,
                "a socket method can carry only one of #[on], #[connect], #[disconnect]",
            ));
        }
        validate_async(&function.sig)?;
        validate_receiver(&function.sig)?;
        match on {
            Some(attribute) => {
                validate_arity(&function.sig, 2, "an #[on] handler")?;
                events.push(Event {
                    name: attribute.parse_args::<LitStr>()?,
                    handler: function.sig.ident.clone(),
                });
            }
            None if connect => {
                validate_arity(&function.sig, 1, "a #[connect] hook")?;
                connects.push(Hook { handler: function.sig.ident.clone() });
            }
            None => {
                validate_arity(&function.sig, 1, "a #[disconnect] hook")?;
                disconnects.push(Hook { handler: function.sig.ident.clone() });
            }
        }
    }
    Ok(())
}
