//! Dependency-free schema pipeline benchmark.
//!
//! Run with `cargo run -p engine-schema --release --example schema_bench`.
//! Timings are averaged over repeated runs in one process; useful for
//! spotting regressions in descriptor size, parse cost and codegen output.

use engine_schema::codegen::{self, Language};
use engine_schema::{generate_with_root, parse_descriptor, ts};
use std::hint::black_box;
use std::time::Instant;

const SCHEMA_TS: &str = r#"
import { schema, t } from "@colyseus/schema";

export const Vector = schema({
    x: t.float32(),
    y: t.float32(),
}, "Vector");

export const Player = schema({
    x: t.number(),
    y: t.number(),
    vx: t.float32(),
    vy: t.float32(),
    hp: t.uint16().default(100),
    level: t.uint8().default(1),
    name: t.string().default("rookie"),
    score: t.uint32().static(),
    target: t.uint32().optional(),
    tags: t.array(t.uint32()),
}, "Player");

export const Projectile = schema({
    x: t.number(),
    y: t.number(),
    vx: t.float32(),
    vy: t.float32(),
    owner: t.uint32(),
    lifetime: t.float32(),
}, "Projectile");

export const State = schema({
    round: t.uint16().default(1),
    phase: t.string().default("lobby"),
    players: t.map(Player),
    projectiles: t.map(Projectile),
}, "State");
"#;

fn bench<T>(name: &str, iterations: u32, mut run: impl FnMut() -> T) -> T {
    // Warm up so the first allocation does not skew the average.
    black_box(run());
    let start = Instant::now();
    let mut last = None;
    for _ in 0..iterations {
        last = Some(black_box(run()));
    }
    let elapsed = start.elapsed();
    let per = elapsed.as_secs_f64() * 1e6 / f64::from(iterations);
    println!("{name:<28} {per:>10.2} us/op   {iterations} iterations");
    last.expect("at least one iteration")
}

fn main() {
    let parsed = ts::parse(SCHEMA_TS).expect("benchmark schema parses");

    bench("ts::parse", 500, || {
        ts::parse(SCHEMA_TS).expect("schema parses")
    });

    let generated = generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts");
    let descriptor = generated.descriptor.clone();
    println!(
        "descriptor                  {} bytes ({} schemas, {} collections)",
        descriptor.len(),
        parsed.schemas.len(),
        parsed
            .root
            .as_ref()
            .map_or(0, |root| root.collections.len()),
    );

    bench("generate_with_root", 2000, || {
        generate_with_root(&parsed.schemas, parsed.root.as_ref(), "schema.ts")
    });

    bench("parse_descriptor", 20_000, || {
        parse_descriptor(black_box(&descriptor)).expect("descriptor decodes")
    });

    for language in Language::ALL {
        let name = format!("codegen::{}", language.flag());
        let output = bench(&name, 1000, || {
            codegen::emit(language, &parsed.schemas, parsed.root.as_ref(), "schema.ts")
        });
        println!(
            "{:<28} {} bytes",
            format!("  {}", language.file_name()),
            output.len()
        );
    }
}
