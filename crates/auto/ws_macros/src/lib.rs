//! Procedural macros for `auto_ws`.
//!
//! `#[auto_ws("/path")]` turns an inherent impl into a socket declaration: the
//! impl keeps its `new()` (so `auto-di` can build it) and its methods are
//! inspected for three inner attributes:
//!
//! - `#[on("event")]` registers the method as the handler for one inbound
//!   event tag;
//! - `#[connect]` registers a hook that runs once per connection, before the
//!   read loop;
//! - `#[disconnect]` registers a hook that runs once per connection, after the
//!   read loop — in declaration order, so teardown can be written top-down.
//!
//! The inner attributes are consumed here (they never expand on their own),
//! exactly like `auto_route`'s `#[get]` inside `#[controller]`.

use proc_macro::TokenStream;
use syn::{Item, ItemFn, LitStr, parse_macro_input};

mod helpers;
mod socket_impl;

/// Declare a websocket at `attr` (the axum path), with `auto-di`-resolved
/// `Arc<Self>` and macro-registered event/lifecycle handlers.
#[proc_macro_attribute]
pub fn auto_ws(attr: TokenStream, item: TokenStream) -> TokenStream {
    let path = parse_macro_input!(attr as LitStr);
    let item = parse_macro_input!(item as Item);
    let result = match item {
        Item::Impl(item) => socket_impl::expand(path, item),
        other => Err(syn::Error::new_spanned(
            other,
            "#[auto_ws] must be placed on an inherent impl block",
        )),
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Only meaningful inside an `#[auto_ws]` impl: the outer macro consumes the
/// attribute. Written as an error so a stray use is caught with a sentence
/// instead of a resolution failure.
#[proc_macro_attribute]
pub fn on(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let _function = parse_macro_input!(item as ItemFn);
    syn::Error::new_spanned(
        _function.sig.ident,
        "#[on] is only valid inside an #[auto_ws] impl block",
    )
    .into_compile_error()
    .into()
}

/// Only meaningful inside an `#[auto_ws]` impl; see [`on`].
#[proc_macro_attribute]
pub fn connect(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let _function = parse_macro_input!(item as ItemFn);
    syn::Error::new_spanned(
        _function.sig.ident,
        "#[connect] is only valid inside an #[auto_ws] impl block",
    )
    .into_compile_error()
    .into()
}

/// Only meaningful inside an `#[auto_ws]` impl; see [`on`].
#[proc_macro_attribute]
pub fn disconnect(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let _function = parse_macro_input!(item as ItemFn);
    syn::Error::new_spanned(
        _function.sig.ident,
        "#[disconnect] is only valid inside an #[auto_ws] impl block",
    )
    .into_compile_error()
    .into()
}
