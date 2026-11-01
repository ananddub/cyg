use super::*;
use engine_schema::{RootCollection, RootState, SchemaDef, SchemaField, emit_descriptor};

fn field(name: &str, kind: WireKind) -> SchemaField {
    SchemaField {
        name: name.to_string(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: kind.typescript_type().to_string(),
        default: None,
        line: 0,
        column: 0,
    }
}

fn descriptor() -> Vec<u8> {
    let player = SchemaDef {
        name: "Player".into(),
        version: 1,
        fields: vec![field("x", WireKind::F64), field("y", WireKind::F64)],
        line: 0,
        column: 0,
    };
    let root = RootState {
        name: "State".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    emit_descriptor(&[player], Some(&root))
}

fn full_section(rows: &[(u32, f64, f64)]) -> Vec<u8> {
    let player = SchemaDef {
        name: "Player".into(),
        version: 1,
        fields: vec![field("x", WireKind::F64), field("y", WireKind::F64)],
        line: 0,
        column: 0,
    };
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.push(1);
    out.extend_from_slice(&wire_hash(&player).to_le_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for (id, x, y) in rows {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
    }
    out
}

fn aoi() -> AoiConfig {
    AoiConfig {
        enabled: true,
        near_radius: 100.0,
        far_radius: 400.0,
        skip_ticks: 4,
        bullet_radius: 100.0,
    }
}

#[test]
fn descriptor_round_trips_through_the_parser() {
    let bytes = descriptor();
    let parsed = parse_descriptor(&bytes).expect("descriptor parses");
    assert_eq!(parsed.version, 3);
    assert_eq!(parsed.schemas.len(), 1);
    assert_eq!(parsed.schemas[0].fields.len(), 2);
    assert_eq!(parsed.collections, vec![("players".to_string(), 0)]);
}

#[test]
fn far_entity_is_culled_on_a_non_boundary_tick() {
    let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).expect("positional schema");
    let full = full_section(&[(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 300.0, 0.0)]);
    let view = aoi_layout.view(&full).expect("full parses");
    let mut out = Vec::new();
    assert!(aoi_layout.filter_culled(&view, 1, 3, &aoi(), &mut out));
    let filtered = aoi_layout.view(&out).expect("filtered full parses");
    let ids: Vec<u32> = filtered.collections[0]
        .rows
        .iter()
        .map(|row| row.id)
        .collect();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn far_entity_rides_a_skip_boundary() {
    let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
    let full = full_section(&[(1, 0.0, 0.0), (2, 50.0, 0.0), (3, 300.0, 0.0)]);
    let view = aoi_layout.view(&full).unwrap();
    let mut out = Vec::new();
    assert!(!aoi_layout.filter_culled(&view, 1, 4, &aoi(), &mut out));
}

#[test]
fn unknown_viewer_leaves_the_full_untouched() {
    let aoi_layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
    let full = full_section(&[(1, 0.0, 0.0), (2, 900.0, 0.0)]);
    let view = aoi_layout.view(&full).unwrap();
    let mut out = Vec::new();
    assert!(!aoi_layout.filter_culled(&view, 42, 3, &aoi(), &mut out));
}

#[test]
fn non_positional_schema_disables_host_views() {
    let plain = SchemaDef {
        name: "Score".into(),
        version: 1,
        fields: vec![field("points", WireKind::U32)],
        line: 0,
        column: 0,
    };
    let world = RootState {
        name: "State".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![RootCollection {
            name: "scores".into(),
            schema: "Score".into(),
        }],
    };
    assert!(SchemaAoi::from_descriptor(&emit_descriptor(&[plain], Some(&world))).is_none());
}
