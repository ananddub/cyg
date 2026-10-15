
use serde_json::Value;

use engine_dashboard::{
    BusHealth, Capacity, ClientRow, Overview, RateLimitHealth, RoomRow, SeatHealth, ServerInfo,
    TickHealth, TickPoint, TickSeries, TransportMix, WorkerRow,
};

fn assert_camel_case(value: &Value, path: &str) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                assert!(!key.contains('_'), "snake_case key `{key}` at {path} breaks the camelCase contract");
                assert_camel_case(child, &format!("{path}/{key}"));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_camel_case(item, &format!("{path}[{i}]"));
            }
        }
        _ => {}
    }
}

fn camel<T: serde::Serialize>(name: &str, value: &T) -> Value {
    let json = serde_json::to_value(value).unwrap_or_else(|e| panic!("{name} failed to serialise: {e}"));
    assert_camel_case(&json, name);
    json
}

#[test]
fn every_contract_struct_serialises_camel_case() {
    camel("Overview", &Overview::default());
    camel("ServerInfo", &ServerInfo::default());
    camel("Capacity", &Capacity::default());
    camel("TickHealth", &TickHealth::default());
    camel("TransportMix", &TransportMix::default());
    camel("BusHealth", &BusHealth::default());
    camel("SeatHealth", &SeatHealth::default());
    camel("RateLimitHealth", &RateLimitHealth::default());
    camel("RoomRow", &RoomRow::default());
    camel("WorkerRow", &WorkerRow::default());
    camel("ClientRow", &ClientRow::default());
    camel("TickSeries", &TickSeries::default());
    camel("TickPoint", &TickPoint::default());
    // StatusSnapshot deliberately stays snake_case; it's `/status`, not dashboard wire
}
