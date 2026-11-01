use std::path::Path;

use crate::host::*;
pub(crate) fn boot_schema(
    _boot: &BootMaps,
    project_dir: &Path,
    cfg: &GameConfig,
    raw_transport: &mut WebRtcTransport,
) -> (Vec<u8>, Option<SchemaAoi>) {
    let candidates = [
        project_dir.join("out").join("cyg.schema.bin"),
        project_dir.join("cyg.schema.bin"),
    ];
    let found = candidates.iter().find(|p| p.exists());
    let schema_descriptor = found
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default();
    if !schema_descriptor.is_empty() {
        tracing::info!(
            "[server] 📋 schema descriptor loaded ({} bytes) from {}",
            schema_descriptor.len(),
            found
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "unknown".into())
        );
        raw_transport.set_schema_descriptor(schema_descriptor.clone());
    }
    let schema_aoi = if cfg.aoi.enabled && !schema_descriptor.is_empty() {
        let parsed = crate::schema_view::SchemaAoi::from_descriptor(&schema_descriptor);
        if parsed.is_none() {
            tracing::warn!(
                "[server] ⚠️ [aoi] enabled but the schema has no x/y fields — schema state stays global"
            );
        }
        parsed
    } else {
        None
    };
    (schema_descriptor, schema_aoi)
}
