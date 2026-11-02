use crate::helpers::{is_socket_argument, live_return_type, typed_arguments, validate_async};
use crate::model::{Event, LiveEvent};
use syn::{FnArg, ImplItem, ItemImpl, LitStr};

pub(super) fn collect_events(
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
