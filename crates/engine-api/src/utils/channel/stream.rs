use uuid::Uuid;

use crate::error::V2Error;

use super::parse::stream_of_channel;
use super::{MODE_DM, MODE_GROUP, MODE_ROOM};

pub fn channel_of_stream(mode: i16, subject: Uuid, descriptor: Uuid, label: &str) -> String {
    let field = |id: Uuid| {
        if id.is_nil() {
            String::new()
        } else {
            id.to_string()
        }
    };
    format!("{mode}.{}.{}.{}", field(subject), field(descriptor), label)
}

#[derive(Debug, Clone)]
pub struct Stream {
    pub mode: i16,
    pub subject: Uuid,
    pub descriptor: Uuid,
    pub label: String,
}

impl Stream {
    pub fn dm(a: Uuid, b: Uuid) -> Stream {
        let (subject, descriptor) = if a.to_string() < b.to_string() {
            (a, b)
        } else {
            (b, a)
        };
        Stream {
            mode: MODE_DM,
            subject,
            descriptor,
            label: String::new(),
        }
    }

    pub fn room(name: &str) -> Result<Stream, V2Error> {
        stream_of_channel(&channel_of_stream(
            MODE_ROOM,
            Uuid::nil(),
            Uuid::nil(),
            name,
        ))
    }

    pub fn group(id: Uuid) -> Stream {
        Stream {
            mode: MODE_GROUP,
            subject: id,
            descriptor: Uuid::nil(),
            label: String::new(),
        }
    }

    pub(crate) fn id(&self) -> String {
        channel_of_stream(self.mode, self.subject, self.descriptor, &self.label)
    }
}
