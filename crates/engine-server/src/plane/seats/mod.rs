mod registry;
mod token;

pub use registry::SeatRegistry;
pub use token::SeatError;
pub use token::SeatGrant;
pub use token::join_token;

#[cfg(test)]
mod tests;
