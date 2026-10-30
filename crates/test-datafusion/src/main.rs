use std::fs::File;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{ArrayRef, BooleanArray, Float32Array, Int32Array, Int64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use datafusion::prelude::*;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

fn get_rss_memory_mb() -> f64 {
    // Read resident set size (RSS) from /proc/self/statm on Linux
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = statm.split_whitespace().collect();
        if parts.len() > 1 {
            if let Ok(pages) = parts[1].parse::<u64>() {
                // Page size is typically 4096 bytes (4KB)
                return (pages * 4096) as f64 / (1024.0 * 1024.0);
            }
        }
    }
    0.0
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("============================================================");
    println!("🔥 DATAFUSION GAME ANALYTICS BENCHMARK (1,000,000 EVENTS)");
    println!("============================================================");

    let base_ram = get_rss_memory_mb();
    println!("Baseline Process RAM: {:.2} MB", base_ram);

    let n_events = 1_000_000;
    println!(
        "\n[1/4] Generating {} dummy match events in memory...",
        n_events
    );
    let gen_start = Instant::now();

    let mut tick_ids = Vec::with_capacity(n_events);
    let mut player_ids = Vec::with_capacity(n_events);
    let mut event_types = Vec::with_capacity(n_events);
    let mut pos_x = Vec::with_capacity(n_events);
    let mut pos_y = Vec::with_capacity(n_events);
    let mut pos_z = Vec::with_capacity(n_events);
    let mut is_headshot = Vec::with_capacity(n_events);
    let mut damage_dealt = Vec::with_capacity(n_events);

    let event_names = ["move", "fire", "hit", "kill", "jump"];
    for i in 0..n_events {
        let p_id = (i % 16) as i32; // 16 players in match
        tick_ids.push(i as i64);
        player_ids.push(p_id);
        event_types.push(event_names[i % 5]);
        pos_x.push(10.0 + (i as f32 % 100.0));
        pos_y.push(0.0 + (i as f32 % 10.0));
        pos_z.push(20.0 + (i as f32 % 80.0));
        is_headshot.push(i % 7 == 0);
        damage_dealt.push(if i % 5 == 2 {
            45
        } else if i % 5 == 3 {
            100
        } else {
            0
        });
    }

    let schema = Arc::new(Schema::new(vec![
        Field::new("tick", DataType::Int64, false),
        Field::new("player_id", DataType::Int32, false),
        Field::new("event", DataType::Utf8, false),
        Field::new("x", DataType::Float32, false),
        Field::new("y", DataType::Float32, false),
        Field::new("z", DataType::Float32, false),
        Field::new("is_headshot", DataType::Boolean, false),
        Field::new("damage", DataType::Int32, false),
    ]));

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(tick_ids)) as ArrayRef,
            Arc::new(Int32Array::from(player_ids)) as ArrayRef,
            Arc::new(StringArray::from(event_types)) as ArrayRef,
            Arc::new(Float32Array::from(pos_x)) as ArrayRef,
            Arc::new(Float32Array::from(pos_y)) as ArrayRef,
            Arc::new(Float32Array::from(pos_z)) as ArrayRef,
            Arc::new(BooleanArray::from(is_headshot)) as ArrayRef,
            Arc::new(Int32Array::from(damage_dealt)) as ArrayRef,
        ],
    )?;

    println!("   Generation took: {:?}", gen_start.elapsed());
    let ram_after_gen = get_rss_memory_mb();
    println!(
        "   RAM after RecordBatch creation: {:.2} MB (+{:.2} MB)",
        ram_after_gen,
        ram_after_gen - base_ram
    );

    // Save as Parquet file
    let parquet_path = "/tmp/test_match_events.parquet";
    println!(
        "\n[2/4] Writing 1,000,000 events to compressed Parquet file: {}",
        parquet_path
    );
    let write_start = Instant::now();
    let file = File::create(parquet_path)?;
    let props = WriterProperties::builder()
        .set_compression(parquet::basic::Compression::SNAPPY)
        .build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    let file_meta = std::fs::metadata(parquet_path)?;
    let file_size_mb = file_meta.len() as f64 / (1024.0 * 1024.0);
    println!("   Writing took: {:?}", write_start.elapsed());
    println!(
        "   Parquet File Size on Disk: {:.2} MB (1 Million events!)",
        file_size_mb
    );

    // Query with Apache DataFusion
    println!("\n[3/4] Initializing Apache DataFusion Query Engine...");
    let init_start = Instant::now();
    let ctx = SessionContext::new();
    ctx.register_parquet("events", parquet_path, ParquetReadOptions::default())
        .await?;
    println!("   DataFusion Context Init: {:?}", init_start.elapsed());

    // Run SQL Query: Anti-cheat & Match Summary
    println!("\n[4/4] Executing Complex Analytical SQL Query over 1 Million Events:");
    println!("   SQL: Aggregate Kills, Total Damage, and Headshot Count grouped by Player...");

    let ram_before_query = get_rss_memory_mb();
    let query_start = Instant::now();

    let df = ctx
        .sql(
            "
        SELECT 
            player_id,
            COUNT(*) as total_events,
            SUM(damage) as total_damage,
            COUNT(CASE WHEN event = 'kill' THEN 1 END) as kills,
            COUNT(CASE WHEN is_headshot = true THEN 1 END) as headshots,
            AVG(x) as avg_x,
            AVG(z) as avg_z
        FROM events
        GROUP BY player_id
        ORDER BY kills DESC
        LIMIT 5
    ",
        )
        .await?;

    let results = df.collect().await?;
    let query_elapsed = query_start.elapsed();
    let peak_ram = get_rss_memory_mb();

    println!("\n--- RESULTS (Top 5 Players) ---");
    for b in results {
        println!(
            "   Query Execution Succeeded! Returned {} aggregated rows with {} computed metrics.",
            b.num_rows(),
            b.num_columns()
        );
    }

    println!("============================================================");
    println!("📊 BENCHMARK VERDICT (REAL EMPIRICAL NUMBERS):");
    println!("============================================================");
    println!("  • Events Scanned:          1,000,000 rows");
    println!("  • Parquet Disk Storage:    {:.2} MB", file_size_mb);
    println!(
        "  • SQL Execution Time:      {:.2} ms ({:?})",
        query_elapsed.as_secs_f64() * 1000.0,
        query_elapsed
    );
    println!("  • RAM Before Query:        {:.2} MB", ram_before_query);
    println!("  • Peak RAM During Query:   {:.2} MB", peak_ram);
    println!(
        "  • Delta RAM for Query:     {:.2} MB",
        peak_ram - ram_before_query
    );
    println!("============================================================");

    // Cleanup
    let _ = std::fs::remove_file(parquet_path);
    Ok(())
}
