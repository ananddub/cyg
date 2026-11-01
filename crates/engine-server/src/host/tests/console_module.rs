    use super::*;
use engine_physics::PhysicsWorld3D;
    use sha2::{Digest, Sha256};

    const MODULE: &[u8] = include_bytes!("../../../../engine-net/examples/e2e/room_logic.wasm");

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn an_uploaded_module_is_prepared_and_hot_swapped_onto_the_host() {
        let runtime = WasmRuntime::new().expect("a wasmtime runtime");
        let pre = prepare_console_module(
            &runtime,
            &digest(MODULE),
            MODULE,
            engine_wasm::AbiPermissions::default(),
        )
        .expect("the console's bytes link against the host imports");

        let mut server = GameServer::new(
            None,
            8,
            PhysicsWorld3D::default(),
            engine_wasm::AbiPermissions::default(),
        );
        server.hot_reload(pre, 100.0, 100.0);
        let room = server.get_or_create("uploaded").expect("a room spawns");
        assert!(
            room.is_wasm(),
            "rooms spawned after the swap run the uploaded module"
        );
    }

    #[test]
    fn bytes_that_do_not_match_the_recorded_digest_are_refused() {
        let runtime = WasmRuntime::new().expect("a wasmtime runtime");
        let err = match prepare_console_module(
            &runtime,
            &digest(b"some other module"),
            MODULE,
            engine_wasm::AbiPermissions::default(),
        ) {
            Ok(_) => panic!("the payload must hash to the version the row recorded"),
            Err(err) => err,
        };
        assert!(err.contains("sha256 mismatch"), "{err}");
    }
