use proc_macro::TokenStream;
use syn::{Item, ItemFn, LitStr, parse_macro_input};

mod helpers;
mod model;
mod module;
mod socket_impl;
mod standalone;

#[proc_macro_attribute]
pub fn auto_socket(attr: TokenStream, item: TokenStream) -> TokenStream {
    let namespace = parse_macro_input!(attr as LitStr);
    let item = parse_macro_input!(item as Item);
    let result = match item {
        Item::Impl(item) => socket_impl::expand(namespace, item),
        Item::Mod(item) => module::expand(namespace, item),
        other => Err(syn::Error::new_spanned(
            other,
            "#[auto_socket] must be placed on an inherent impl or inline module",
        )),
    };
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

#[proc_macro_attribute]
pub fn on(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as standalone::Args);
    let function = parse_macro_input!(item as ItemFn);
    standalone::expand(args, function)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
