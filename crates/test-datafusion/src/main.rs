use std::fs::File;
use std::time::Instant;

use datafusion::prelude::*;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

mod events;
mod mem;

use mem::get_rss_memory_mb;

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

    let (schema, batch) = events::build(n_events)?;

    println!("   Generation took: {:?}", gen_start.elapsed());
    let ram_after_gen = get_rss_memory_mb();
    println!(
        "   RAM after RecordBatch creation: {:.2} MB (+{:.2} MB)",
        ram_after_gen,
        ram_after_gen - base_ram
    );

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

    println!("\n[3/4] Initializing Apache DataFusion Query Engine...");
    let init_start = Instant::now();
    let ctx = SessionContext::new();
    ctx.register_parquet("events", parquet_path, ParquetReadOptions::default())
        .await?;
    println!("   DataFusion Context Init: {:?}", init_start.elapsed());

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

    let _ = std::fs::remove_file(parquet_path);
    Ok(())
}
