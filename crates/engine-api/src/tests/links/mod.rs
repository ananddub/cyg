//! The `Link*` routes: which answer a request earns and in what order.
//! All ten share one pipeline, and `Unavailable` is the sound of a passing request.

mod fields;
mod helpers;
mod pipeline;
mod providers;
