use poem_openapi::Object;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Object)]
pub struct Empty {}

/// A binary multipart field, documented as OpenAPI `string`/`binary`.
///
/// The upload handlers stream the multipart body themselves, so this type is
/// only the schema carrier for the generated spec. Its JSON conversion is a
/// lossy string fallback; it is never a request DTO on the wire.
#[derive(Debug, Clone, Default)]
pub struct BinaryField(pub Vec<u8>);

impl poem_openapi::types::Type for BinaryField {
    const IS_REQUIRED: bool = true;
    type RawValueType = Self;
    type RawElementValueType = Self;

    fn name() -> std::borrow::Cow<'static, str> {
        "binary".into()
    }

    fn schema_ref() -> poem_openapi::registry::MetaSchemaRef {
        poem_openapi::registry::MetaSchemaRef::Inline(Box::new(
            poem_openapi::registry::MetaSchema::new_with_format("string", "binary"),
        ))
    }

    fn as_raw_value(&self) -> Option<&Self::RawValueType> {
        Some(self)
    }

    fn raw_element_iter<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = &'a Self::RawElementValueType> + 'a> {
        Box::new(self.as_raw_value().into_iter())
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl poem_openapi::types::ParseFromJSON for BinaryField {
    fn parse_from_json(value: Option<serde_json::Value>) -> poem_openapi::types::ParseResult<Self> {
        match value {
            Some(serde_json::Value::String(text)) => Ok(Self(text.into_bytes())),
            Some(other) => Err(poem_openapi::types::ParseError::expected_type(other)),
            None => Ok(Self::default()),
        }
    }
}

impl poem_openapi::types::ToJSON for BinaryField {
    fn to_json(&self) -> Option<serde_json::Value> {
        Some(serde_json::Value::String(
            String::from_utf8_lossy(&self.0).into_owned(),
        ))
    }
}
