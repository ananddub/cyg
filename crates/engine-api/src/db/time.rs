use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serializer};

pub const EPOCH: &str = "1970-01-01T00:00:00.000000Z";

pub fn encode(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(SecondsFormat::Micros, true)
}

pub fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("unix epoch is a valid timestamp")
}

pub fn always() -> DateTime<Utc> {
    chrono::NaiveDate::from_ymd_opt(9999, 12, 31)
        .and_then(|date| date.and_hms_micro_opt(23, 59, 59, 999_999))
        .expect("9999-12-31 is a valid date")
        .and_utc()
}

pub fn decode(text: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

pub mod iso {
    use super::*;

    pub fn serialize<S: Serializer>(time: &DateTime<Utc>, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&encode(*time))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<DateTime<Utc>, D::Error> {
        let text = String::deserialize(input)?;
        decode(&text).map_err(serde::de::Error::custom)
    }
}

pub mod iso_opt {
    use super::*;

    pub fn serialize<S: Serializer>(
        time: &Option<DateTime<Utc>>,
        out: S,
    ) -> Result<S::Ok, S::Error> {
        match time {
            Some(time) => out.serialize_str(&encode(*time)),
            None => out.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        input: D,
    ) -> Result<Option<DateTime<Utc>>, D::Error> {
        let text = Option::<String>::deserialize(input)?;
        text.map(|text| decode(&text).map_err(serde::de::Error::custom))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn fixed_width_sorts_chronologically() {
        let earlier = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let later = earlier + chrono::Duration::milliseconds(500);
        assert!(encode(earlier) < encode(later));
        let default = |time: DateTime<Utc>| time.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        assert!(default(later) < default(earlier));
    }

    #[test]
    fn round_trips_through_serde() {
        #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        struct Row {
            #[serde(with = "iso")]
            at: DateTime<Utc>,
            #[serde(with = "iso_opt")]
            maybe: Option<DateTime<Utc>>,
        }
        let row = Row {
            at: Utc.with_ymd_and_hms(2026, 10, 29, 12, 34, 56).unwrap(),
            maybe: None,
        };
        let json = serde_json::to_value(&row).expect("serialize");
        assert_eq!(json["at"], "2026-10-29T12:34:56.000000Z");
        let back: Row = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, row);
    }
}
