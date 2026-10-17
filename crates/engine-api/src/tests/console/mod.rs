//! The console surface and its own login realm: the bootstrap admin can sign in,
//! a refusal is uniform, and a console token is not a player session — nor the
//! reverse. The write routes are gated by the role inside the token, which is what
//! lets the gates be exercised with no database behind the plane.

mod boards;
mod gates;
mod helpers;
mod matches;
mod realm;
mod routes;
