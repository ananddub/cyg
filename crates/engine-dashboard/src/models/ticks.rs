use serde::Serialize;

use crate::utils::histogram::TICK_HISTOGRAM_BUCKETS;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickSeries {
    pub interval_sec: u64,
    pub points: Vec<TickPoint>,
    pub histogram: [u32; TICK_HISTOGRAM_BUCKETS],
    pub bucket_edges_us: [u64; TICK_HISTOGRAM_BUCKETS],
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickPoint {
    pub at_ms: u64,
    pub p50_us: u64,
    pub p99_us: u64,
    pub hz: f64,
    pub players: u32,
    pub seq: u64,
    pub gap: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_series_still_ships_its_axis() {
        let json = serde_json::to_value(TickSeries::default()).unwrap();
        assert!(json["points"].as_array().unwrap().is_empty());
        assert_eq!(
            json["bucketEdgesUs"].as_array().unwrap().len(),
            TICK_HISTOGRAM_BUCKETS
        );
    }
}
