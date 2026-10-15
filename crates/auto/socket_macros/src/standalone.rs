use crate::helpers::validate_async;
use quote::{format_ident, quote};
use syn::{
    Ident, ItemFn, LitStr, Token,
    parse::{Parse, ParseStream},
};

pub(crate) struct Args {
    event: LitStr,
    namespace: LitStr,
}
impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let event = input.parse()?;
        if input.is_empty() {
            return Ok(Self {
                event,
                namespace: LitStr::new("/", proc_macro2::Span::call_site()),
            });
        }
        input.parse::<Token![,]>()?;
        let key: Ident = input.parse()?;
        if key != "namespace" {
            return Err(syn::Error::new_spanned(key, "expected `namespace`"));
        }
        input.parse::<Token![=]>()?;
        Ok(Self {
            event,
            namespace: input.parse()?,
        })
    }
}
pub(crate) fn expand(args: Args, function: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    validate_async(&function.sig)?;
    let handler = &function.sig.ident;
    let factory = format_ident!("__auto_socket_factory_{}", handler);
    let event = args.event;
    let namespace = args.namespace;
    Ok(quote! {
        #function
        #[doc(hidden)] fn #factory<'a>(_: &'a ::auto_socket::__private::auto_di::Container) -> ::auto_socket::__private::auto_di::BoxFuture<'a, ::std::result::Result<::auto_socket::SocketRegistrar, ::auto_socket::__private::auto_di::DiError>> {
            ::std::boxed::Box::pin(async move { let registrar: ::auto_socket::SocketRegistrar = ::std::sync::Arc::new(move |socket| socket.on(#event, #handler)); Ok(registrar) })
        }
        ::auto_socket::__private::inventory::submit! { ::auto_socket::SocketDescriptor::new(#namespace, #factory) }
    })
}
