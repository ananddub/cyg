use engine_core::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct Player {
    input: PlayerInputBuffer,
    next_seq: u64,
}

fn run_pipeline(players: usize, ticks: u64) -> Duration {
    let mut runner = TickRunner::new();
    let mut inputs: HashMap<u32, Player> = (0..players as u32)
        .map(|i| {
            (
                i,
                Player {
                    input: PlayerInputBuffer::new(64),
                    next_seq: 0,
                },
            )
        })
        .collect();

    for i in 0..players as u32 {
        runner.world.spawn((
            EntityId(i),
            Position2D {
                x: i as f32,
                y: 0.0,
            },
            Velocity {
                vx: 1.0,
                vy: 0.5,
                vz: 0.0,
            },
            PlayerControlled {
                client_id: i as u64,
            },
            Health::new(100.0),
        ));
    }

    let mut rewind = RewindManager::new(500, 60);
    let mut rng = RoomRng::from_seed_str("capacity-probe");

    runner.add_system(
        |mut q: bevy_ecs::prelude::Query<(&mut Position2D, &Velocity, &mut Health)>| {
            for (mut pos, vel, mut hp) in &mut q {
                pos.x += vel.vx * 0.016_6;
                pos.y += vel.vy * 0.016_6;

                let _ = hp.take_damage(0.001);
            }
        },
    );

    let mut clock = FixedClock::new(60);
    let start = Instant::now();
    for _ in 0..ticks {
        let ctx = clock.advance();

        for player in inputs.values_mut() {
            for _ in 0..2 {
                player.input.push(PlayerInput::new(
                    player.next_seq,
                    ctx.dt_ms * player.next_seq as f64,
                    vec![0u8; 32],
                ));
                player.next_seq += 1;
            }
            while player.input.consume().is_some() {}
        }

        runner.tick();

        for entity in 0..players as u64 {
            rewind.record(
                entity,
                ctx.tick,
                ctx.dt_ms * ctx.tick as f64,
                TransformSnapshot::new(entity as f32, 0.0, 0.0, 1.0),
            );
        }

        for _ in 0..players {
            let _ = rng.next_f32();
        }
    }
    start.elapsed()
}

#[test]
#[ignore = "capacity probe: run with --ignored --nocapture"]
fn one_thread_capacity() {
    println!("\n=== 1-thread sim capacity (60 Hz, full per-player pipeline) ===");
    println!(
        "{:>8} {:>10} {:>12} {:>12}",
        "players", "ticks", "per-tick", "per-player"
    );

    let mut per_player_ns = 0.0f64;
    let mut samples = 0usize;
    for players in [100usize, 500, 1000, 2000] {
        let ticks = if players >= 1000 { 60 } else { 120 };
        let elapsed = run_pipeline(players, ticks);
        let per_tick_us = elapsed.as_secs_f64() * 1e6 / ticks as f64;
        let per_player = elapsed.as_secs_f64() * 1e9 / (ticks as f64 * players as f64);

        if players >= 500 {
            per_player_ns += per_player;
            samples += 1;
        }
        println!("{players:>8} {ticks:>10} {per_tick_us:>9.1}us {per_player:>9.0}ns");
    }

    let avg = per_player_ns / samples as f64;
    let budget_ns = 16.67e6;
    let ccu = (budget_ns / avg) as usize;
    println!("\nmeasured per-player cost: {avg:.0} ns/tick (engine-core sim floor)");
    println!("=> loop-layer ceiling: ~{ccu} players on ONE thread at 16.67ms");
    println!("   NOTE: this is the core floor only — physics, WASM guests, state");
    println!("   patching and net I/O stack on top; treat 1/3 as the safe mark.");
}
