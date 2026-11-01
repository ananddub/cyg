use super::*;
use std::hint::black_box;
use std::time::Instant;

fn rss_mb() -> f64 {
    let s = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let kb: f64 = s
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(0.0);
    kb * 4.0 / 1024.0
}

fn pctl(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[(v.len() as f64 * p).min((v.len() - 1) as f64) as usize]
}

#[test]
fn bench_ecs_single_thread_capacity() {
    let mut room = EcsRoomState::new("bench-1k", None);
    let t0 = Instant::now();
    for i in 0..1000 {
        room.add_player(
            &format!("s{i}"),
            i as i64 + 1,
            (i % 40) as f64 * 15.0,
            (i / 40) as f64 * 15.0,
        );
    }
    let spawn_s = t0.elapsed().as_secs_f64();
    for i in 0..1000 {
        room.apply_input(&format!("s{i}"), &[input_flags::D]);
    }
    let mut ticks = Vec::new();
    let mut delta_bytes = 0usize;
    for _ in 0..120 {
        let t = Instant::now();
        let d = room.tick(1.0 / 60.0);
        delta_bytes = d.len();
        ticks.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    black_box(&ticks);
    tracing::info!(
        "[A] 1 room / 1000 players: spawn {:.0}ms, tick mean {:.3}ms p50 {:.3} p95 {:.3} p99 {:.3}, delta/tick {}B, RSS+{:.1}MB",
        spawn_s * 1000.0,
        ticks.iter().sum::<f64>() / ticks.len() as f64,
        pctl(ticks.clone(), 0.5),
        pctl(ticks.clone(), 0.95),
        pctl(ticks.clone(), 0.99),
        delta_bytes,
        rss_mb()
    );

    let mut rooms: Vec<EcsRoomState> = (0..100)
        .map(|r| {
            let mut room = EcsRoomState::new(&format!("bench-{r}"), None);
            for i in 0..100 {
                room.add_player(
                    &format!("s{i}"),
                    i as i64 + 1,
                    (i % 10) as f64 * 15.0,
                    (i / 10) as f64 * 15.0,
                );
                room.apply_input(&format!("s{i}"), &[input_flags::D | input_flags::S]);
            }
            room
        })
        .collect();
    let mut frame = Vec::new();
    let mut bytes = 0usize;
    for _ in 0..120 {
        let t = Instant::now();
        for room in &mut rooms {
            bytes += room.tick(1.0 / 60.0).len();
        }
        frame.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    black_box(&frame);
    tracing::info!(
        "[B] 100 rooms / 10000 players total: frame mean {:.3}ms p50 {:.3} p95 {:.3} p99 {:.3}, deltas/frame {:.0}KB, RSS+{:.1}MB",
        frame.iter().sum::<f64>() / frame.len() as f64,
        pctl(frame.clone(), 0.5),
        pctl(frame.clone(), 0.95),
        pctl(frame.clone(), 0.99),
        bytes as f64 / 120.0 / 1024.0,
        rss_mb()
    );
}
