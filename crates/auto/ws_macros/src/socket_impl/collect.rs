use syn::{AttrStyle, ImplItem, ItemImpl, LitStr};

use crate::helpers::{validate_arity, validate_async, validate_receiver};

pub(super) struct Event {
    pub(super) name: LitStr,
    pub(super) handler: syn::Ident,
}

pub(super) struct Hook {
    pub(super) handler: syn::Ident,
}

pub(super) fn collect(
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
                connects.push(Hook {
                    handler: function.sig.ident.clone(),
                });
            }
            None => {
                validate_arity(&function.sig, 1, "a #[disconnect] hook")?;
                disconnects.push(Hook {
                    handler: function.sig.ident.clone(),
                });
            }
        }
    }
    Ok(())
}
