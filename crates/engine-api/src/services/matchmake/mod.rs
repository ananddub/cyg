use auto_di::singleton;

pub mod chain;
pub mod handoff;
pub mod reserve;

#[cfg(test)]
mod tests;

pub struct MatchmakeService;

#[singleton]
impl MatchmakeService {
    fn new() -> Self {
        Self
    }
}
