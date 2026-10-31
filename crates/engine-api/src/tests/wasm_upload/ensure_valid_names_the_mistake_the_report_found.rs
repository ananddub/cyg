use super::*;

#[test]
pub(crate) fn ensure_valid_names_the_mistake_the_report_found() {
    let not_wasm = b"this is a zip, probably".to_vec();
    let err = WasmService::ensure_valid(&WasmService::validate_binary(&not_wasm))
        .expect_err("a blob that is not wasm cannot be stored");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");
    assert!(
        err.message.contains("magic"),
        "the refusal says the bytes are not a module at all: {}",
        err.message
    );

    let wrong_abi = empty_wasm();
    let err = WasmService::ensure_valid(&WasmService::validate_binary(&wrong_abi))
        .expect_err("a module with no handlers cannot be stored");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");
    for name in REQUIRED_EXPORTS {
        assert!(
            err.message.contains(name),
            "an ABI refusal lists the export that is missing; `{name}` absent from {}",
            err.message
        );
    }

    let (good, _) = complete_wasm();
    WasmService::ensure_valid(&WasmService::validate_binary(&good))
        .expect("a module with the whole ABI is not refused");
}
pub(crate) fn sha256_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
pub(crate) fn params(
    filename: &str,
    bytes: &[u8],
    source: Option<&str>,
) -> crate::services::wasm::upload::InsertModuleParams {
    crate::services::wasm::upload::InsertModuleParams {
        filename: filename.to_owned(),
        binary_bytes: bytes.to_vec(),
        source_code: source.map(str::to_owned),
        sha256: sha256_of(bytes),
    }
}
pub(crate) fn skipped_without_database() {
    tracing::info!("skipping: the embedded `mem://` store did not open");
}
#[tokio::test]
pub(crate) async fn insert_module_versions_monotonically_and_deactivates_the_older() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-insert").await;
    let (bytes, names) = complete_wasm();

    let first = service
        .insert_module(&state, game.id, params("room-v1.wasm", &bytes, None))
        .await
        .expect("the first upload is version 1");
    assert_eq!(first.version, 1);
    assert!(first.is_active, "an upload is what the next room boots");
    assert_eq!(
        first.size_bytes as usize,
        bytes.len(),
        "the size is the row's payload"
    );
    assert!(
        !first.has_source,
        "an upload that sent no source stores none"
    );

    let second = service
        .insert_module(
            &state,
            game.id,
            params("room-v2.wasm", &bytes, Some("fn on_tick() {}")),
        )
        .await
        .expect("the second upload is the next number");
    assert_eq!(second.version, 2, "versions only ever go up, per game");

    let list = service
        .list_modules(&state, game.id)
        .await
        .expect("the listing reads");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].version, 2, "newest first");
    assert_eq!(list[1].version, 1);
    assert!(list[0].is_active, "the newer version holds the flag");
    assert!(!list[1].is_active, "and the older one lost it");
    assert_eq!(
        list.iter().filter(|module| module.is_active).count(),
        1,
        "exactly one version is active"
    );
    assert!(
        list[0].has_source,
        "the source an uploader sent is recorded"
    );
    assert!(
        list.iter().all(|module| module.sha256 == sha256_of(&bytes)),
        "the hash is the bytes' own"
    );

    assert_eq!(
        crate::services::games::GamesService::active_module_version(&state, game.id)
            .await
            .expect("the active version reads"),
        Some(2)
    );
    assert_eq!(
        crate::services::games::GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        2
    );

    let other = db::create_wasm(&state, "wasm-insert-other").await;
    let foreign = service
        .insert_module(&state, other.id, params("room.wasm", &bytes, None))
        .await
        .expect("another game's first upload");
    assert_eq!(
        foreign.version, 1,
        "a new tenant starts at version 1: {names:?} exports"
    );

    db::forget(&pool, game.id).await;
    db::forget(&pool, other.id).await;
}
