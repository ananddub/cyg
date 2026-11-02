use proc_macro::TokenStream;
use syn::{parse_macro_input, Item};

mod controller;
mod descriptor;
mod docs;
mod live;
mod live_options;
mod model;
mod module;
mod options;
mod params;
mod schema;
mod standalone;
mod types;

#[cfg(test)]
mod path_tests;

use controller::expand_controller;
use module::expand_controller_module;
use options::ControllerOptions;
use standalone::standalone_route;

#[proc_macro_attribute]
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    let controller_options = parse_macro_input!(attr as ControllerOptions);
    let item = parse_macro_input!(item as Item);

    let expanded = match item {
        Item::Impl(item_impl) => expand_controller(controller_options, item_impl),
        Item::Mod(item_mod) => expand_controller_module(controller_options, item_mod),
        other => Err(syn::Error::new_spanned(
            other,
            "#[controller] must be placed on an inherent impl or inline module",
        )),
    };

    match expanded {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

macro_rules! route_attribute {
    ($name:ident) => {
        #[proc_macro_attribute]
        pub fn $name(attr: TokenStream, item: TokenStream) -> TokenStream {
            standalone_route(stringify!($name), attr, item)
        }
    };
}

route_attribute!(get);
route_attribute!(post);
route_attribute!(put);
route_attribute!(delete);
route_attribute!(patch);
route_attribute!(options);
route_attribute!(head);
