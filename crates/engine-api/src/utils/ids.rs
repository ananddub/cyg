use uuid::Uuid;

use crate::error::V2Error;

pub(crate) fn parse_uuid(raw: &str, what: &str) -> Result<Uuid, V2Error> {
    Uuid::parse_str(raw).map_err(|_| V2Error::invalid_argument(format!("Invalid {what} id.")))
}

pub(crate) fn account(raw: &str) -> Result<Uuid, V2Error> {
    parse_uuid(raw, "account")
}
