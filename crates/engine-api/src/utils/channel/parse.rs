//! Parsing the dotted channel id into the [`Stream`] it names.

use uuid::Uuid;

use crate::error::V2Error;

use super::Stream;
use super::{MODE_DM, MODE_GROUP, MODE_ROOM};

const BAD_ID: &str = "Invalid channel ID.";

pub fn stream_of_channel(channel_id: &str) -> Result<Stream, V2Error> {
    let parts: Vec<&str> = channel_id.splitn(4, '.').collect();
    if parts.len() != 4 {
        return Err(V2Error::invalid_argument(BAD_ID));
    }
    let mode: i16 = parts[0]
        .parse()
        .map_err(|_| V2Error::invalid_argument(BAD_ID))?;
    let nil_or = |raw: &str| -> Result<Uuid, V2Error> {
        if raw.is_empty() {
            Ok(Uuid::nil())
        } else {
            Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument(BAD_ID))
        }
    };
    let subject = nil_or(parts[1])?;
    let descriptor = nil_or(parts[2])?;
    let label = parts[3];

    match mode {
        // A room carries its name in the label and nothing else; BaaS bounds
        // it at 1-64 chars and rejects control characters.
        MODE_ROOM => {
            if !parts[1].is_empty() || !parts[2].is_empty() {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
            if label.is_empty() || label.len() > 64 || label.chars().any(|c| c.is_control()) {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
        }
        MODE_GROUP => {
            if !parts[2].is_empty() || !label.is_empty() {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
            if subject.is_nil() {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
        }
        MODE_DM => {
            if !label.is_empty() {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
            if subject.is_nil() || descriptor.is_nil() || subject == descriptor {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
            // BaaS stores the pair in a canonical order so one DM has one id.
            if subject.to_string() > descriptor.to_string() {
                return Err(V2Error::invalid_argument(BAD_ID));
            }
        }
        _ => return Err(V2Error::invalid_argument(BAD_ID)),
    }

    Ok(Stream {
        mode,
        subject,
        descriptor,
        label: label.to_string(),
    })
}
