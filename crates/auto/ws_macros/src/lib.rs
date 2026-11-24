use proc_macro::TokenStream;
use syn::{Item, ItemFn, LitStr, parse_macro_input};

mod helpers;
mod socket_impl;

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
