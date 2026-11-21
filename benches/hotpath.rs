//! Hot-path micro benchmarks — criterion + `black_box` per the books
//! methodology (Rust Atomics & Locks ch. 2: an un-boxed loop measures
//! nothing, the optimizer elides it; Idiomatic Rust ch. 10: benchmark
//! collection choices instead of guessing).
//!
//! Run: `cargo bench --bench hotpath`
//!
//! These complement the load bench (`just cyg-bench`, JS): that one measures
//! end-to-end p50/p95 under real traffic, these isolate the per-tick/per-packet
//! costs that dominate it.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, black_box as cb, criterion_group, criterion_main};
use engine_api::infra::rate_limit::IpRateLimiter;
use engine_config::AoiConfig;
use engine_server::netcode;
use engine_spatial::aoi::{AoiViewIndex, DeltaEntity, parse_delta};

/// A synthetic schema envelope: [OXS2][full_len u32][full][patch_len u32][patch]
/// with 2KB full / 64B patch sections — roughly a 16-entity room at 60Hz.
fn schema_envelope() -> Vec<u8> {
    let mut buf = Vec::with_capacity(4096);
    // Each section must itself start with the OXS2 magic (split_schema
    // validates both the envelope and both section headers).
    let mut full = vec![0u8; 2048];
    full[..4].copy_from_slice(b"OXS2");
    let mut patch = vec![0x7Fu8; 64];
    patch[..4].copy_from_slice(b"OXS2");
    buf.extend_from_slice(b"OXS2");
    buf.extend_from_slice(&(full.len() as u32).to_le_bytes());
    buf.extend_from_slice(&full);
    buf.extend_from_slice(&(patch.len() as u32).to_le_bytes());
    buf.extend_from_slice(&patch);
    buf
}

fn bench_split_schema(c: &mut Criterion) {
    let buf = schema_envelope();
    c.bench_function("netcode/split_schema_2kb", |b| {
        b.iter(|| netcode::split_schema(black_box(&buf)).is_some())
    });
}

fn bench_wrap_delta(c: &mut Criterion) {
    let buf = schema_envelope();
    let sections = netcode::split_schema(&buf).unwrap();
    c.bench_function("netcode/wrap_delta_2kb_full", |b| {
        let mut out = Vec::with_capacity(4096);
        b.iter(|| {
            netcode::wrap_delta_into(black_box(&mut out), 42, black_box(sections.full), 7, 1000, 0);
            black_box(&out);
        })
    });
    // The per-session cost after the parse-once/patch-frame refactor: 10
    // bytes in place vs the old full re-wrap.
    let mut frame = Vec::with_capacity(4096);
    netcode::wrap_delta_into(&mut frame, 0, sections.full, 0, 1000, 0);
    c.bench_function("netcode/patch_wrap_10bytes", |b| {
        b.iter(|| {
            netcode::patch_wrap_into(black_box(&mut frame), 1234, 9, 0);
            black_box(&frame);
        })
    });
}

fn bench_rate_limiter(c: &mut Criterion) {
    c.bench_function("rate_limit/allow_sharded_ip", |b| {
        let limiter = IpRateLimiter::with_rates(20.0, 40.0);
        let mut addr = std::net::IpAddr::from([203u8, 0, 113, 1]);
        b.iter(|| {
            // Vary the address so the map has real entries, not one hot bucket.
            let [a, b_, c, mut d] = match addr {
                std::net::IpAddr::V4(v4) => v4.octets(),
                _ => [203, 0, 113, 1],
            };
            d = d.wrapping_add(1);
            addr = std::net::IpAddr::from([a, b_, c, d]);
            cb(limiter.allow(black_box(addr)));
        })
    });
    c.bench_function("rate_limit/allow_player_sharded", |b| {
        let limiter = IpRateLimiter::with_rates(20.0, 40.0);
        let mut uid = uuid::Uuid::from_u128(1);
        b.iter(|| {
            uid = uuid::Uuid::from_u128(uid.as_u128() + 1);
            cb(limiter.allow_player(black_box(uid)));
        })
    });
}

/// 64 entities scattered around the origin, viewer at the center: the classic
/// "one AOI filter per viewer per tick" cost.
fn bench_aoi_filter(c: &mut Criterion) {
    let aoi = AoiConfig::default();
    // Build a delta-shaped buffer: [1 byte ver][N × 72B entities][bullets…].
    let entity_count = 64usize;
    let mut delta = vec![0u8; 5 + entity_count * 72 + 4];
    delta[0] = 1;
    let mut entities = Vec::with_capacity(entity_count);
    for i in 0..entity_count as i64 {
        // Positions on a ~600-unit ring around origin — near/far mix.
        let angle = (i as f64) * 0.1;
        let pos: [f64; 3] = [angle.cos() * 600.0, 0.0, angle.sin() * 600.0];
        let b = 5 + (i as usize) * 72;
        delta[b..b + 8].copy_from_slice(&i.to_le_bytes());
        delta[b + 8..b + 16].copy_from_slice(&pos[0].to_le_bytes());
        delta[b + 16..b + 24].copy_from_slice(&pos[1].to_le_bytes());
        delta[b + 24..b + 32].copy_from_slice(&pos[2].to_le_bytes());
        entities.push(DeltaEntity { id: i, pos });
    }
    let mut index = AoiViewIndex::new(aoi.far_radius);
    index.rebuild(&entities);
    let mut out = Vec::with_capacity(8 * 1024);
    let mut visible = Vec::with_capacity(entity_count);
    c.bench_function("aoi/filter_64_viewers", |b| {
        b.iter(|| {
            engine_spatial::aoi::filter_for_viewer_indexed(
                black_box(&entities),
                0,
                (5 + entity_count * 72, 5 + entity_count * 72 + 4),
                black_box(&delta),
                1,
                Some([0.0, 0.0, 0.0]),
                100,
                &aoi,
                &mut index,
                &mut out,
                &mut visible,
            );
            black_box(&out);
        })
    });
    c.bench_function("aoi/parse_delta_64", |b| {
        b.iter(|| {
            let parsed = parse_delta(black_box(&delta));
            black_box(parsed.is_some());
        })
    });
}

/// The per-tick fanout after the parse-once refactor: parse once, then patch
/// N session frames in place — the shape tick_delta now runs.
fn bench_tick_fanout_shape(c: &mut Criterion) {
    let buf = schema_envelope();
    let sections = netcode::split_schema(&buf).unwrap();
    let sessions = 64u64;
    c.bench_function("tick/parse_once_plus_64_session_patches", |b| {
        let mut base = Vec::with_capacity(4096);
        b.iter_batched(
            || (),
            |_| {
                netcode::wrap_delta_into(&mut base, 0, sections.full, 0, 1000, 0);
                for id in 0..sessions {
                    netcode::patch_wrap_into(&mut base, id as i64, (id % 256) as u16, 0);
                    black_box(&base);
                }
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_split_schema,
    bench_wrap_delta,
    bench_rate_limiter,
    bench_aoi_filter,
    bench_tick_fanout_shape
);
criterion_main!(benches);
