//! The packed wire is the only wire. These tests pin the client half: a frame
//! the host produces must decode to exactly the state it encoded, including
//! the multi-collection world form (root scalars + every collection).

use cyg_core_rs::state::{
    ClientState, KIND_FULL, KIND_PATCH, KIND_WORLD_FULL, KIND_WORLD_PATCH, Value,
};
use engine_schema::{
    Descriptor, FieldEncoding, SchemaDef, SchemaField, WireKind, default_encoding, emit_descriptor,
    parse_descriptor,
};
use engine_webrtc_protocol::packed::{self, PackedOp, PackedSchema, WireValue};

fn field(
    name: &str,
    kind: WireKind,
    encoding: FieldEncoding,
    element: Option<WireKind>,
) -> SchemaField {
    SchemaField {
        name: name.into(),
        kind,
        element,
        ref_target: None,
        flags: 0,
        source_type: String::new(),
        default: None,
        encoding,
        nested_path: None,
        line: 0,
        column: 0,
    }
}

fn player_schema() -> SchemaDef {
    SchemaDef {
        nested_arrays: Vec::new(),
        name: "Player".into(),
        version: 1,
        fields: vec![
            field("hp", WireKind::U8, default_encoding(WireKind::U8), None),
            field(
                "yaw",
                WireKind::F32,
                FieldEncoding::range(8, 0.0, 360.0),
                None,
            ),
            field(
                "alive",
                WireKind::Bool,
                default_encoding(WireKind::Bool),
                None,
            ),
            field("name", WireKind::String, FieldEncoding::NONE, None),
            field(
                "flags",
                WireKind::U16,
                default_encoding(WireKind::U16),
                None,
            ),
            field(
                "path",
                WireKind::Array,
                FieldEncoding::NONE,
                Some(WireKind::F64),
            ),
        ],
        line: 0,
        column: 0,
    }
}

fn descriptor() -> Descriptor {
    let root = engine_schema::RootState {
        nested: Vec::new(),
        name: "Arena".into(),
        version: 1,
        scalars: vec![],
        collections: vec![engine_schema::RootCollection {
            name: "players".into(),
            schema: "Player".into(),
        }],
    };
    parse_descriptor(&emit_descriptor(&[player_schema()], Some(&root))).expect("descriptor")
}

/// The rows the packed frame carries.
fn rows() -> Vec<(u32, Vec<WireValue>)> {
    vec![
        (
            3,
            vec![
                WireValue::U8(100),
                WireValue::F32(90.0),
                WireValue::Bool(true),
                WireValue::Str("ana".into()),
                WireValue::U16(7),
                WireValue::Arr(vec![WireValue::F64(1.5), WireValue::F64(2.5)]),
            ],
        ),
        (
            11,
            vec![
                WireValue::U8(1),
                WireValue::F32(359.0),
                WireValue::Bool(false),
                WireValue::Str(String::new()),
                WireValue::U16(65_535),
                WireValue::Arr(vec![]),
            ],
        ),
    ]
}

fn apply(descriptor: &Descriptor, section: &[u8]) -> ClientState {
    let mut state = ClientState::new(descriptor);
    state
        .apply_section(section, descriptor)
        .expect("section applies");
    state
}

#[test]
fn a_packed_full_section_hydrates_the_collection() {
    let descriptor = descriptor();
    let schema = descriptor.schemas.first().expect("a schema").clone();
    let packed_schema = PackedSchema::from_def(&schema);
    let mut clamped = 0;
    let bytes = packed::encode_full(&packed_schema, &rows(), &mut clamped).expect("encode");
    assert_eq!(clamped, 0);

    let state = apply(&descriptor, &bytes);
    assert!(state.ready());
    let players = state.collection("players").expect("players");
    assert_eq!(
        players.entities.keys().copied().collect::<Vec<_>>(),
        vec![3, 11]
    );

    let ana = &players.entities[&3];
    assert_eq!(ana.values[0], Value::U8(100));
    assert_eq!(ana.values[2], Value::Bool(true));
    assert_eq!(ana.values[3], Value::Str("ana".into()));
    assert_eq!(ana.values[4], Value::U16(7));
    assert_eq!(
        ana.values[5],
        Value::Arr(vec![Value::F64(1.5), Value::F64(2.5)])
    );
    // yaw is quantized to 8 bits over a full turn — half a step is the contract.
    let Value::F32(yaw) = ana.values[1] else {
        panic!("yaw must stay a float");
    };
    assert!((yaw - 90.0).abs() <= 360.0 / 255.0 / 2.0 + 1e-3, "yaw {yaw}");
}

#[test]
fn a_packed_patch_updates_and_removes() {
    let descriptor = descriptor();
    let schema = descriptor.schemas.first().expect("a schema").clone();
    let packed_schema = PackedSchema::from_def(&schema);
    let mut clamped = 0;
    let base = packed::encode_full(&packed_schema, &rows(), &mut clamped).expect("encode");
    let mut state = apply(&descriptor, &base);

    let ops = vec![
        PackedOp::Update {
            id: 3,
            mask: 0b11,
            values: vec![(0, WireValue::U8(55)), (1, WireValue::F32(180.0))],
        },
        PackedOp::Remove { id: 11 },
    ];
    let patch = packed::encode_patch(&packed_schema, &ops, &mut clamped).expect("encode");
    state
        .apply_section(&patch, &descriptor)
        .expect("packed patch");

    let players = state.collection("players").expect("players");
    assert_eq!(players.entities.len(), 1, "entity 11 was removed");
    let player = &players.entities[&3];
    assert_eq!(player.values[0], Value::U8(55));
    let Value::F32(yaw) = player.values[1] else {
        panic!("yaw must stay a float");
    };
    assert!((yaw - 180.0).abs() <= 360.0 / 255.0 / 2.0 + 1e-3, "yaw {yaw}");
    assert_eq!(player.values[2], Value::Bool(true), "unmasked fields stay");
}

#[test]
fn a_packed_world_applies_root_scalars_and_every_collection() {
    let marker = SchemaDef {
        nested_arrays: Vec::new(),
        name: "Marker".into(),
        version: 1,
        fields: vec![
            field(
                "x",
                WireKind::F32,
                FieldEncoding::range(16, -1000.0, 1000.0),
                None,
            ),
            field(
                "on",
                WireKind::Bool,
                default_encoding(WireKind::Bool),
                None,
            ),
        ],
        line: 0,
        column: 0,
    };
    let root = engine_schema::RootState {
        nested: Vec::new(),
        name: "World".into(),
        version: 1,
        scalars: vec![field(
            "tick",
            WireKind::U16,
            default_encoding(WireKind::U16),
            None,
        )],
        collections: vec![
            engine_schema::RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            },
            engine_schema::RootCollection {
                name: "markers".into(),
                schema: "Marker".into(),
            },
        ],
    };
    let descriptor =
        parse_descriptor(&emit_descriptor(&[player_schema(), marker], Some(&root))).expect("desc");
    let root_def = SchemaDef {
        nested_arrays: Vec::new(),
        name: "World".into(),
        version: 1,
        fields: root.scalars,
        line: 0,
        column: 0,
    };
    let root_packed = PackedSchema::from_def(&root_def);

    let player_packed = PackedSchema::from_def(&descriptor.schemas[0]);
    let marker_packed = PackedSchema::from_def(&descriptor.schemas[1]);
    let mut clamped = 0;
    let player_bytes =
        packed::encode_full(&player_packed, &rows(), &mut clamped).expect("player encode");
    let marker_bytes = packed::encode_full(
        &marker_packed,
        &[(5, vec![WireValue::F32(12.5), WireValue::Bool(true)])],
        &mut clamped,
    )
    .expect("marker encode");

    let bytes = packed::encode_world_full(
        &root_packed,
        &[WireValue::U16(42)],
        &[
            (0, player_packed.hash(), player_bytes),
            (1, marker_packed.hash(), marker_bytes),
        ],
        &mut clamped,
    )
    .expect("world encode");

    let state = apply(&descriptor, &bytes);
    assert!(state.ready());
    assert_eq!(state.root_value(0), Some(&Value::U16(42)));
    assert_eq!(state.collection("players").expect("players").entities.len(), 2);
    let markers = state.collection("markers").expect("markers");
    assert_eq!(markers.entities.len(), 1);
    let Value::F32(x) = markers.entities[&5].values[0] else {
        panic!("x must stay a float");
    };
    assert!((x - 12.5).abs() <= 2000.0 / 65_535.0 / 2.0 + 1e-3, "x {x}");
    assert_eq!(markers.entities[&5].values[1], Value::Bool(true));
}

#[test]
fn a_packed_world_patch_touches_only_the_changed_collection() {
    let marker = SchemaDef {
        nested_arrays: Vec::new(),
        name: "Marker".into(),
        version: 1,
        fields: vec![field(
            "x",
            WireKind::F32,
            default_encoding(WireKind::F32),
            None,
        )],
        line: 0,
        column: 0,
    };
    let root = engine_schema::RootState {
        nested: Vec::new(),
        name: "World".into(),
        version: 1,
        scalars: vec![field(
            "tick",
            WireKind::U16,
            default_encoding(WireKind::U16),
            None,
        )],
        collections: vec![
            engine_schema::RootCollection {
                name: "players".into(),
                schema: "Player".into(),
            },
            engine_schema::RootCollection {
                name: "markers".into(),
                schema: "Marker".into(),
            },
        ],
    };
    let descriptor =
        parse_descriptor(&emit_descriptor(&[player_schema(), marker], Some(&root))).expect("desc");
    let root_def = SchemaDef {
        nested_arrays: Vec::new(),
        name: "World".into(),
        version: 1,
        fields: root.scalars,
        line: 0,
        column: 0,
    };
    let root_packed = PackedSchema::from_def(&root_def);
    let player_packed = PackedSchema::from_def(&descriptor.schemas[0]);
    let marker_packed = PackedSchema::from_def(&descriptor.schemas[1]);
    let mut clamped = 0;

    let player_bytes =
        packed::encode_full(&player_packed, &rows(), &mut clamped).expect("player encode");
    let marker_bytes = packed::encode_full(
        &marker_packed,
        &[(5, vec![WireValue::F32(1.0)])],
        &mut clamped,
    )
    .expect("marker encode");
    let full = packed::encode_world_full(
        &root_packed,
        &[WireValue::U16(1)],
        &[
            (0, player_packed.hash(), player_bytes),
            (1, marker_packed.hash(), marker_bytes),
        ],
        &mut clamped,
    )
    .expect("world encode");
    let mut state = apply(&descriptor, &full);

    let marker_patch = packed::encode_patch(
        &marker_packed,
        &[PackedOp::Update {
            id: 5,
            mask: 0b1,
            values: vec![(0, WireValue::F32(9.0))],
        }],
        &mut clamped,
    )
    .expect("marker patch");
    let patch = packed::encode_world_patch(
        &root_packed,
        0b1,
        &[(0, WireValue::U16(2))],
        &[(1, marker_packed.hash(), marker_patch)],
        &mut clamped,
    )
    .expect("world patch");
    state
        .apply_section(&patch, &descriptor)
        .expect("world patch applies");

    assert_eq!(state.root_value(0), Some(&Value::U16(2)));
    // The untouched collection keeps both rows.
    assert_eq!(state.collection("players").expect("players").entities.len(), 2);
    assert_eq!(state.collection("markers").expect("markers").entities[&5].values[0], Value::F32(9.0));
}

#[test]
fn the_packed_kind_constants_match_the_protocol_crate() {
    assert_eq!(KIND_FULL, packed::KIND_FULL);
    assert_eq!(KIND_PATCH, packed::KIND_PATCH);
    assert_eq!(KIND_WORLD_FULL, packed::KIND_WORLD_FULL);
    assert_eq!(KIND_WORLD_PATCH, packed::KIND_WORLD_PATCH);
    assert!(packed::is_packed_section(b"OXS2\x05"));
    assert!(packed::is_packed_section(b"OXS2\x07"));
    assert!(!packed::is_packed_section(b"OXS2\x01"));
}

#[test]
fn a_legacy_section_is_rejected_instead_of_silently_ignored() {
    let descriptor = descriptor();
    let mut legacy = Vec::new();
    legacy.extend_from_slice(b"OXS2");
    legacy.push(1); // the old fixed-width full
    legacy.extend_from_slice(&[0u8; 8]);
    let mut state = ClientState::new(&descriptor);
    let error = state
        .apply_section(&legacy, &descriptor)
        .expect_err("the fixed-width wire is gone");
    assert!(error.contains("unsupported schema frame kind 1"), "{error}");
}

#[test]
fn a_hash_mismatch_is_a_client_error_not_a_silent_empty_collection() {
    let descriptor = descriptor();
    let schema = descriptor.schemas.first().expect("a schema").clone();
    // A schema that differs only in version hashes differently — the frame must be rejected
    // instead of quietly leaving the client with an empty collection.
    let other = PackedSchema::from_def(&SchemaDef {
        version: schema.version + 1,
        ..schema
    });
    let mut clamped = 0;
    let bytes = packed::encode_full(&other, &rows(), &mut clamped).expect("encode");

    let mut state = ClientState::new(&descriptor);
    let error = state
        .apply_section(&bytes, &descriptor)
        .expect_err("a different schema must be rejected");
    assert!(error.contains("hash mismatch"), "unexpected error: {error}");
}
