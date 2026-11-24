use engine_config::AoiConfig;
use engine_schema::{RootCollection, RootState, SchemaDef, SchemaField, WireKind, emit_descriptor};
use engine_webrtc_protocol::packed::{self, PackedSchema, WireValue};

use crate::schema_view::SchemaAoi;

fn field(name: &str, kind: WireKind) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element: None,
        ref_target: None,
        flags: 0,
        source_type: kind.typescript_type().to_string(),
        default: None,
        encoding: engine_schema::default_encoding(kind),
        nested_path: None,
        line: 0,
        column: 0,
    }
}
fn player() -> SchemaDef {
    SchemaDef {
        nested_arrays: Vec::new(),
        name: "Player".into(),
        version: 1,
        fields: vec![field("x", WireKind::F64), field("y", WireKind::F64)],
        line: 0,
        column: 0,
    }
}
fn root() -> RootState {
    RootState {
        nested: Vec::new(),
        name: "State".into(),
        version: 1,
        scalars: Vec::new(),
        collections: vec![RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    }
}
fn descriptor() -> Vec<u8> {
    emit_descriptor(&[player()], Some(&root()))
}
fn full_section(rows: &[(u32, f64, f64)]) -> Vec<u8> {
    let rows = rows
        .iter()
        .map(|(id, x, y)| (*id, vec![WireValue::F64(*x), WireValue::F64(*y)]))
        .collect::<Vec<_>>();
    packed::encode_full(&PackedSchema::from_def(&player()), &rows, &mut 0).unwrap()
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
    let parsed = engine_schema::parse_descriptor(&descriptor()).expect("descriptor parses");
    assert_eq!(parsed.version, engine_schema::DESCRIPTOR_VERSION);
    assert_eq!(parsed.schemas.len(), 1);
    assert_eq!(parsed.schemas[0].fields.len(), 2);
    assert_eq!(parsed.collections, vec![("players".to_string(), 0)]);
}
#[test]
fn far_entity_is_culled_on_a_non_boundary_tick() {
    let layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
    let view = layout
        .view(&full_section(&[
            (1, 0.0, 0.0),
            (2, 50.0, 0.0),
            (3, 300.0, 0.0),
        ]))
        .unwrap();
    let mut out = Vec::new();
    assert!(layout.filter_culled(&view, 1, 3, &aoi(), &mut out));
    let filtered = layout.view(&out).expect("filtered packed full parses");
    assert_eq!(
        filtered.collections[0]
            .rows
            .iter()
            .map(|row| row.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}
#[test]
fn far_entity_rides_a_skip_boundary() {
    let layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
    let view = layout
        .view(&full_section(&[
            (1, 0.0, 0.0),
            (2, 50.0, 0.0),
            (3, 300.0, 0.0),
        ]))
        .unwrap();
    let mut out = Vec::new();
    assert!(!layout.filter_culled(&view, 1, 4, &aoi(), &mut out));
}
#[test]
fn unknown_viewer_leaves_the_full_untouched() {
    let layout = SchemaAoi::from_descriptor(&descriptor()).unwrap();
    let view = layout
        .view(&full_section(&[(1, 0.0, 0.0), (2, 900.0, 0.0)]))
        .unwrap();
    let mut out = Vec::new();
    assert!(!layout.filter_culled(&view, 42, 3, &aoi(), &mut out));
}
#[test]
fn non_positional_schema_disables_host_views() {
    let plain = SchemaDef {
        nested_arrays: Vec::new(),
        name: "Score".into(),
        version: 1,
        fields: vec![field("points", WireKind::U32)],
        line: 0,
        column: 0,
    };
    let world = RootState {
        nested: Vec::new(),
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
