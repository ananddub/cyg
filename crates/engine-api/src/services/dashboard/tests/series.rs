use engine_dashboard::{StatusSnapshot, TICK_HISTOGRAM_BUCKETS};

use crate::services::dashboard as services;
use crate::utils::dashboard::history::history;

#[test]
fn a_skipped_sample_reads_as_a_gap_not_a_flat_line() {
    let _guard = super::TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let h = history();
    let t0 = (engine_bus::clock::mono_ns() / 1_000_000).saturating_sub(20_000);
    let mut series = services::build_ticks();
    for _ in 0..3 {
        h.clear();
        h.push(
            StatusSnapshot {
                seq: 9101,
                tick_budget_p50_us: 100,
                ..Default::default()
            },
            t0,
        );
        h.push(
            StatusSnapshot {
                seq: 9102,
                tick_budget_p50_us: 200,
                ..Default::default()
            },
            t0 + 1_000,
        );
        h.push(
            StatusSnapshot {
                seq: 9103,
                tick_budget_p50_us: 300,
                tick_histogram: [1; TICK_HISTOGRAM_BUCKETS],
                ..Default::default()
            },
            t0 + 4_000,
        );
        series = services::build_ticks();
        if series.points.len() == h.len() && h.len() == 3 {
            break;
        }
    }
    assert_eq!(series.points.len(), 3, "one point per ring entry");
    assert_eq!(series.interval_sec, 1);
    assert_eq!(series.bucket_edges_us, engine_dashboard::bucket_edges_us());
    assert_eq!(
        series.histogram, [1; TICK_HISTOGRAM_BUCKETS],
        "newest window"
    );
    assert_eq!(
        series.points.iter().map(|p| p.seq).collect::<Vec<_>>(),
        vec![9101, 9102, 9103]
    );
    assert_eq!(
        series.points.iter().map(|p| p.gap).collect::<Vec<_>>(),
        vec![false, false, true]
    );
    assert_eq!(series.points[1].at_ms, t0 + 1_000);
    assert_eq!(series.points[1].p50_us, 200);
}
