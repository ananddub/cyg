use base64::Engine;
use serde::{Deserialize, Deserializer, Serializer};

pub fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub(crate) fn decode(value: serde_json::Value) -> Result<Vec<u8>, String> {
    match value {
        serde_json::Value::String(text) => base64::engine::general_purpose::STANDARD
            .decode(&text)
            .map_err(|err| format!("invalid base64 column: {err}")),
        serde_json::Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_u64()
                    .filter(|byte| *byte <= u64::from(u8::MAX))
                    .map(|byte| byte as u8)
                    .ok_or_else(|| format!("not a byte: {item}"))
            })
            .collect(),
        other => Err(format!("expected a base64 column, found {other}")),
    }
}

pub mod b64 {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &[u8], out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<Vec<u8>, D::Error> {
        let value = serde_json::Value::deserialize(input)?;
        decode(value).map_err(serde::de::Error::custom)
    }
}

pub mod b64_opt {
    use super::*;

    pub fn serialize<S: Serializer>(bytes: &Option<Vec<u8>>, out: S) -> Result<S::Ok, S::Error> {
        match bytes {
            Some(bytes) => out.serialize_str(&encode(bytes)),
            None => out.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        let value = serde_json::Value::deserialize(input)?;
        match value {
            serde_json::Value::Null => Ok(None),
            other => decode(other).map(Some).map_err(serde::de::Error::custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Row {
        #[serde(with = "b64")]
        raw: Vec<u8>,
        #[serde(with = "b64_opt")]
        optional: Option<Vec<u8>>,
    }

    #[test]
    fn round_trips_and_accepts_byte_arrays() {
        let row = Row {
            raw: vec![1, 2, 3],
            optional: None,
        };
        let json = serde_json::to_value(&row).expect("serialize");
        assert_eq!(json["raw"], "AQID");
        assert_eq!(serde_json::from_value::<Row>(json.clone()).unwrap(), row);

        let array = serde_json::json!({ "raw": [1, 2, 3], "optional": null });
        assert_eq!(serde_json::from_value::<Row>(array).unwrap(), row);
    }
}
