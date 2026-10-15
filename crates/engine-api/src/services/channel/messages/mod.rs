//! The channel message endpoints: read a page of a channel's messages, and
//! write, update or tombstone one. Every path checks membership first.

mod list;
mod remove;
mod send;
mod update;
