use crate::channel::MAX_FRAGMENTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    BodyTooLarge {
        body_len: usize,
        fragments_needed: usize,
        max_body_len: usize,
    },
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BodyTooLarge {
                body_len,
                fragments_needed,
                max_body_len,
            } => write!(
                f,
                "delta body of {body_len} B needs {fragments_needed} fragments, \
                 over {MAX_FRAGMENTS}; the channel carries at most {max_body_len} B"
            ),
        }
    }
}

impl std::error::Error for PackError {}
