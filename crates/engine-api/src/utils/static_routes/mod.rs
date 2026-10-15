//! Static surface helpers: the containment rule, the MIME table, the response
//! shapes and the Unity path mapping.

pub(crate) mod guard;
pub(crate) mod mime;
pub(crate) mod responses;
pub(crate) mod unity;

pub use mime::content_type_for;
