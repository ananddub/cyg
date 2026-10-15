use syn::{Ident, LitStr, Type};

pub(crate) struct Event {
    pub(crate) name: LitStr,
    pub(crate) handler: Ident,
    pub(crate) argument_types: Vec<Type>,
}

pub(crate) struct LiveEvent {
    pub(crate) name: LitStr,
    pub(crate) handler: Ident,
    pub(crate) return_type: Type,
    pub(crate) argument_types: Vec<Type>,
}
