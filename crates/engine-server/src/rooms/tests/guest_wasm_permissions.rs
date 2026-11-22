use super::*;

const MESSAGE_GUEST_WAT: &str = r#"
        (module

          (import "env" "host_send"

            (func $host_send (param i64 i32 i32 i32 i32) (result i32)))

          (memory (export "memory") 1)

          (data (i32.const 0) "chat")

          (data (i32.const 8) "\01\02\03")

          (func (export "on_create") (param i32)

            (drop (call $host_send

              (i64.const 0) (i32.const 0) (i32.const 4) (i32.const 8) (i32.const 3))))

          (func (export "on_join") (param i64 i32 i32 i32 i32))

          (func (export "on_leave") (param i64 i32))

          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))

          (func (export "on_input") (param i64 i32 i32))

          (func (export "on_tick") (param f64) (result i64) (i64.const 0))

          (func (export "on_save") (result i64) (i64.const 0))

          (func (export "on_load") (param i32 i32))

        )

    "#;

fn wat_message_room(permissions: engine_wasm::AbiPermissions) -> GameRoom {
    let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
    let module = runtime
        .compile(MESSAGE_GUEST_WAT.as_bytes())
        .expect("compile message wat");
    let linker = build_host_linker(&runtime, permissions);
    let pre = runtime.prepare(&linker, &module).expect("prepare guest");
    GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate message room")
}

#[test]
fn guest_messages_are_queued_for_the_host_and_gated_by_permission() {
    let mut room = wat_message_room(engine_wasm::AbiPermissions {
        messaging: true,
        ..Default::default()
    });
    assert_eq!(
        room.drain_outbox(),
        vec![OutboundMessage {
            target: 0,
            event: "chat".into(),
            payload: vec![1, 2, 3].into(),
        }]
    );
    assert!(
        room.drain_outbox().is_empty(),
        "drain must consume the queue"
    );

    let mut denied = wat_message_room(engine_wasm::AbiPermissions {
        messaging: false,
        ..Default::default()
    });
    assert!(
        denied.drain_outbox().is_empty(),
        "a manifest without messaging must not let the guest queue traffic"
    );
}

#[test]
fn guest_on_is_full_overrides_the_host_ceiling() {
    let mut room = wat_room();

    room.join("a", 0.0, 0.0, b"");
    assert!(!room.is_full(16), "guest says not full at one player");

    room.join("b", 0.0, 0.0, b"");
    assert!(
        room.is_full(16),
        "the guest's on_is_full(count >= 2) must drive the decision, not the host's 16"
    );

    let mut relay = GameRoom::new_relay();
    relay.join("a", 0.0, 0.0, b"");
    relay.join("b", 0.0, 0.0, b"");
    assert!(!relay.is_full(16));
    assert!(relay.is_at_capacity(2));
}

#[test]
fn storage_permission_gates_guest_save_and_load() {
    let mut room = wat_room();
    let snap = serde_json::to_vec(&RoomSnapshotData::Wasm(vec![1, 2, 3])).expect("snapshot json");

    assert!(room.export_snapshot().is_some(), "permitted room persists");
    assert!(
        room.import_snapshot(&snap).is_ok(),
        "permitted room restores"
    );

    room.storage_enabled = false;
    assert!(
        room.export_snapshot().is_none(),
        "denied room must never call on_save"
    );
    assert!(
        room.import_snapshot(&snap).is_err(),
        "denied room must never call on_load"
    );
}

#[test]
fn storage_permission_gates_the_snapshot_store() {
    let mut allowed = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions {
            storage: true,
            ..Default::default()
        },
    );
    allowed.save_snapshot("zone", vec![1, 2, 3]);
    assert!(allowed.has_snapshot("zone"));

    let mut denied = GameServer::new(
        None,
        16,
        PhysicsWorld3D::default(),
        engine_wasm::AbiPermissions {
            storage: false,
            ..Default::default()
        },
    );
    denied.save_snapshot("zone", vec![1, 2, 3]);
    assert!(
        !denied.has_snapshot("zone"),
        "a manifest without storage must not reach the snapshot store"
    );

    let room = denied.get_or_create("zone", b"").expect("relay room");
    assert!(!room.storage_enabled);
}

fn wat_stat_room(permissions: engine_wasm::AbiPermissions, name: &str) -> GameRoom {
    let wat = format!(
        r#"
        (module
          (import "env" "host_stat_add"
            (func $host_stat_add (param i64 i32 i32 i64) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "{name}")
          (func (export "on_create") (param i32)
            (drop (call $host_stat_add
              (i64.const 7) (i32.const 0) (i32.const {len}) (i64.const 3))))
          (func (export "on_join") (param i64 i32 i32 i32 i32))
          (func (export "on_leave") (param i64 i32))
          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))
          (func (export "on_input") (param i64 i32 i32))
          (func (export "on_tick") (param f64) (result i64) (i64.const 0))
          (func (export "on_save") (result i64) (i64.const 0))
          (func (export "on_load") (param i32 i32))
        )
        "#,
        len = name.len()
    );
    let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
    let module = runtime.compile(wat.as_bytes()).expect("compile stat wat");
    let linker = build_host_linker(&runtime, permissions);
    let pre = runtime.prepare(&linker, &module).expect("prepare guest");
    GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate stats room")
}

#[test]
fn guest_stats_are_gated_and_validated() {
    let mut allowed = wat_stat_room(
        engine_wasm::AbiPermissions {
            stats: true,
            ..Default::default()
        },
        "kills",
    );
    let intents = allowed.drain_stat_intents();
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].seat, 7);
    assert_eq!(intents[0].stat.as_ref(), "kills");
    assert_eq!(intents[0].delta, 3);

    let mut denied = wat_stat_room(
        engine_wasm::AbiPermissions {
            stats: false,
            ..Default::default()
        },
        "kills",
    );
    assert!(
        denied.drain_stat_intents().is_empty(),
        "without allow_stats the guest must not queue a counter"
    );

    let mut bad = wat_stat_room(
        engine_wasm::AbiPermissions {
            stats: true,
            ..Default::default()
        },
        "Kills",
    );
    assert!(
        bad.drain_stat_intents().is_empty(),
        "a name outside the lowercase identifier set is refused"
    );
}

fn wat_view_room(permissions: engine_wasm::AbiPermissions, field: &str) -> GameRoom {
    let wat = format!(
        r#"
        (module
          (import "env" "host_view_hide"
            (func $hide (param i64 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (data (i32.const 0) "players")
          (data (i32.const 16) "{field}")
          (func (export "on_create") (param i32)
            (drop (call $hide
              (i64.const 7) (i32.const 0) (i32.const 7)
              (i32.const 16) (i32.const {len}))))
          (func (export "on_join") (param i64 i32 i32 i32 i32))
          (func (export "on_leave") (param i64 i32))
          (func (export "on_alloc") (param i32) (result i32) (i32.const 0))
          (func (export "on_input") (param i64 i32 i32))
          (func (export "on_tick") (param f64) (result i64) (i64.const 0))
          (func (export "on_save") (result i64) (i64.const 0))
          (func (export "on_load") (param i32 i32))
        )
        "#,
        len = field.len()
    );
    let runtime = engine_wasm::WasmRuntime::new().expect("wasm runtime");
    let module = runtime.compile(wat.as_bytes()).expect("compile view wat");
    let linker = build_host_linker(&runtime, permissions);
    let pre = runtime.prepare(&linker, &module).expect("prepare guest");
    GameRoom::new(&pre, 16, PhysicsWorld3D::default()).expect("instantiate view room")
}

#[test]
fn guest_view_intents_are_gated_and_name_checked() {
    let mut allowed = wat_view_room(
        engine_wasm::AbiPermissions {
            views: true,
            ..Default::default()
        },
        "inventory",
    );
    let intents = allowed.drain_view_intents();
    assert_eq!(intents.len(), 1);
    match &intents[0] {
        crate::netcode::ViewIntent::HideField {
            seat,
            collection,
            field,
        } => {
            assert_eq!(*seat, 7);
            assert_eq!(collection.as_ref(), "players");
            assert_eq!(field.as_ref(), "inventory");
        }
        other => panic!("expected a hide intent, got {other:?}"),
    }

    let mut denied = wat_view_room(
        engine_wasm::AbiPermissions {
            views: false,
            ..Default::default()
        },
        "inventory",
    );
    assert!(denied.drain_view_intents().is_empty());

    let mut bad = wat_view_room(
        engine_wasm::AbiPermissions {
            views: true,
            ..Default::default()
        },
        "inv-entory",
    );
    assert!(
        bad.drain_view_intents().is_empty(),
        "a hyphen is outside the identifier subset"
    );
}
