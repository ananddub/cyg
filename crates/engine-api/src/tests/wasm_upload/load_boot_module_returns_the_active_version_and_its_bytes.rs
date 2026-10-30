use sha2::{Digest, Sha256};

use super::*;

#[tokio::test]
pub(crate) async fn load_boot_module_returns_the_active_version_and_its_bytes() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-boot").await;
    let mut state = state;
    std::sync::Arc::make_mut(&mut state.game).game_id = Some(game.id.to_string());
    let (bytes, _) = complete_wasm();

    assert!(
        WasmService::load_boot_module(&state).await.is_none(),
        "a game without uploaded modules boots with no console module"
    );

    for version in [1, 2] {
        service
            .insert_module(
                &state,
                game.id,
                params(&format!("room-v{version}.wasm"), &bytes, None),
            )
            .await
            .expect("the version is stored");
    }
    let boot = WasmService::load_boot_module(&state)
        .await
        .expect("the newest upload is the active version the host boots with");
    assert_eq!(boot.version, 2);
    assert_eq!(boot.bytes, bytes, "the host gets exactly the stored bytes");
    assert_eq!(
        boot.sha256,
        hex::encode(Sha256::digest(&bytes)),
        "the digest the host verifies travels with the payload"
    );

    service
        .activate_module(&state, game.id, 1, engine_bus::ReloadKind::Rollback)
        .await
        .expect("an older version can be activated");
    let rolled_back = WasmService::load_boot_module(&state)
        .await
        .expect("the boot read follows the active flag, not the newest version");
    assert_eq!(rolled_back.version, 1);

    db::forget(&pool, game.id).await;
}
