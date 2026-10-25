//! The dynamic-WASM-upload gate: what an uploaded room module has to be before the
//! plane stores it, which games the surface applies to at all, and how the module
//! routes present themselves.
//!
//! Everything here is pure up to the mode guard. `WasmService::validate_binary` is a
//! decision about bytes — it is asserted with hand-assembled modules, so the test needs
//! no database, no temp directory and no build of a real guest, and a change to the ABI
//! is a change to the byte-builder rather than to a fixture nobody can read. The upload
//! *lifecycle* (insert, activate, delete) and the engine-mode gate both read `games`
//! first — a `relay` game has no server-side module to boot — so they are `games.rs`'s
//! database-gated group, where the rows live.
//!
//! The required export names are not hardcoded from a specification: they are parsed
//! out of the host's own declaration of what it calls on a guest, so a rename on either
//! side fails a test here instead of booting a room that panics on the first tick.

use crate::dto::console::wasm::ActivateRequest;
use crate::error::{INVALID_ARGUMENT, UNAUTHENTICATED};
use crate::infra::console::{self, ConsoleRole};
use crate::services::wasm::REQUIRED_EXPORTS;
use crate::services::wasm::upload::WasmService;

use super::games::db;
use super::server::V2Server;
use super::v2_state;

/// The refusal a relay game earns from this surface, spelled exactly as the plan spells
/// it: an operator reading the console's error has to be told which plane their game is
/// on, and what to change if they meant the other one.
const RELAY_REFUSAL: &str = "Game is configured for Unity C# Relay. WASM upload is only \
     applicable for Web/Custom Authoritative games.";

/// The host's declaration of the game-host ABI: the `wasm_module!` block whose entries
/// become the functions `GameRoom` calls. Read at compile time from the file that owns
/// the calls, so this test cannot fall behind the runtime.
const HOST_SOURCE: &str = include_str!("../../../../crates/engine-server/src/rooms.rs");

/// Every export name the host declares it invokes on a guest.
///
/// Only the non-test part of `rooms.rs` is parsed: the `#[cfg(test)]` module
/// at the bottom holds helpers whose names merely look like exports (e.g. a
/// test named `on_auth_sees_the_client_options_and_can_reject`), and counting
/// one of those would fail every drift assertion below.
pub(super) fn abi_exports() -> Vec<String> {
    let runtime_source = HOST_SOURCE
        .split("#[cfg(test)]")
        .next()
        .unwrap_or(HOST_SOURCE);
    runtime_source
        .lines()
        .filter_map(|line| {
            let (_, after) = line.trim().split_once("fn on_")?;
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then(|| format!("on_{name}"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The hand-assembled module. A WASM binary is a magic number, a version, and a
// sequence of (id, length, payload) sections, which is little enough to write out.
// ---------------------------------------------------------------------------

/// `pub(super)` because `games.rs` stores these bytes in `room_modules.binary_bytes`.
pub(super) fn empty_wasm() -> Vec<u8> {
    vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00]
}

/// LEB128, the unsigned variable-length integer the section lengths and counts use.
fn leb128(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return out;
        }
    }
}

/// A section: its id, its payload length, and its payload.
fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb128(payload.len()));
    out.extend(payload);
    out
}

/// The type section of every module built here: one function type, `() -> ()`. It is
/// enough for every ABI handler to be *declared*, which is all an upload gate reads.
fn functype_entry() -> Vec<u8> {
    let mut out = leb128(1);
    out.extend([0x60u8, 0x00, 0x00]);
    out
}

/// A module exporting `names` — each as a function of type `() -> ()`, which is all
/// the export *table* has to agree on for a name to be present. The bodies are empty
/// because the validator under test reads the table, not the code.
pub(super) fn module_with_exports(names: &[String]) -> Vec<u8> {
    let mut out = empty_wasm();

    // Type section (1): one function type, no parameters, no results.
    out.extend(section(1, &functype_entry()));

    // Function section (3): one function per export, all of type 0.
    let functions: Vec<u8> = leb128(names.len())
        .into_iter()
        .chain(std::iter::repeat_n(0u8, names.len()))
        .collect();
    out.extend(section(3, &functions));

    // Export section (7): kind 0x00 is a function, and the index is its slot.
    let mut exports = leb128(names.len());
    for (index, name) in names.iter().enumerate() {
        exports.extend(leb128(name.len()));
        exports.extend(name.as_bytes());
        exports.push(0x00);
        exports.extend(leb128(index));
    }
    out.extend(section(7, &exports));

    // Code section (10): one empty body per function — no locals, then `end`.
    let mut bodies = leb128(names.len());
    for _ in names {
        bodies.extend(leb128(2));
        bodies.extend([0x00, 0x0b]);
    }
    out.extend(section(10, &bodies));

    out
}

/// The complete module: every export the host says it calls.
fn complete_wasm() -> (Vec<u8>, Vec<String>) {
    let names = abi_exports();
    assert!(
        !names.is_empty(),
        "the host declares at least one ABI export"
    );
    (module_with_exports(&names), names)
}

// ---------------------------------------------------------------------------
// The ABI list, and the drift guard that keeps this file honest about it.
// ---------------------------------------------------------------------------

/// The ABI the upload gate enforces *is* the ABI the host declares: the parsed call
/// list and `WasmService::REQUIRED_EXPORTS` must be the same eight names. This is the
/// test that makes the gate honest — an upload validator that drifted from the host
/// would either store a module no room can boot or refuse one that runs fine.
#[test]
fn the_upload_gate_requires_exactly_what_the_host_invokes() {
    let mut declared = abi_exports();
    declared.sort();
    declared.dedup();
    let mut gate: Vec<String> = REQUIRED_EXPORTS
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    gate.sort();
    assert_eq!(
        declared, gate,
        "the exports the host declares and the exports the upload gate requires differ"
    );
}

/// The seven lifecycle entries the contract names are among the required set — the
/// drift test above says the two lists agree, this one says the list is not empty or
/// renamed into nothing.
#[test]
fn the_required_exports_are_the_ones_the_host_declares() {
    let declared = abi_exports();
    for required in [
        "on_create",
        "on_join",
        "on_leave",
        "on_alloc",
        "on_tick",
        "on_save",
        "on_load",
    ] {
        assert!(
            declared.iter().any(|name| name == required),
            "`{required}` is invoked by the room host and so must be a required export; \
             the host declares {declared:?}"
        );
    }
    assert_eq!(
        declared
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        declared.len(),
        "the host's declaration must not name an export twice: {declared:?}"
    );
}

/// A module assembled from that list is parseable by the validator, which is the only
/// reason the list is worth reading out of the source: an export section the validator
/// cannot walk would accept nothing and reject everything.
#[test]
fn a_module_built_from_the_declared_abi_is_a_real_module() {
    let (bytes, names) = complete_wasm();
    assert_eq!(&bytes[0..4], b"\0asm", "the magic is the magic");
    assert_eq!(
        &bytes[4..8],
        &[1, 0, 0, 0],
        "and the version is the one WASM 1 knows"
    );

    let report = WasmService::validate_binary(&bytes);
    assert!(
        report.wasm_magic_ok,
        "the bytes are a WASM module: {names:?}"
    );
    assert_eq!(
        report.export_count as usize,
        names.len(),
        "every export in the table is counted"
    );
    assert_eq!(report.size_bytes as usize, bytes.len());
    for name in &names {
        assert!(
            report.exports_found.iter().any(|found| found == name),
            "a module that exports `{name}` must be reported as exporting it"
        );
    }
    assert!(
        report.valid,
        "a module with the whole ABI and nothing missing is accepted"
    );
}

// ---------------------------------------------------------------------------
// Rejections.
// ---------------------------------------------------------------------------

/// Something that is not a WASM module at all fails the first check, and the report
/// says so in the field that means it — the console distinguishes "wrong format" from
/// "wrong ABI" for the operator, and so does the validator.
#[test]
fn a_blob_that_is_not_wasm_fails_the_magic_check() {
    for candidate in [
        b"MZ\0\0 this is a PE, not a module".to_vec(),
        b"wat".to_vec(),
        b"\0asm".to_vec(),
        Vec::new(),
    ] {
        let report = WasmService::validate_binary(&candidate);
        assert!(!report.valid, "{candidate:?} is not a game module");
        if candidate.len() >= 4 && &candidate[..4] == b"\0asm" {
            // The truncated header: the magic is right and the module is still broken,
            // which is the one case `wasm_magic_ok` alone cannot answer.
            assert!(report.wasm_magic_ok, "{candidate:?} does carry the magic");
        } else {
            assert!(
                !report.wasm_magic_ok,
                "a blob that does not open with `\\0asm` is refused on the magic: {candidate:?}"
            );
        }
        assert_eq!(report.size_bytes as usize, candidate.len());
    }
}

/// The wrong magic, byte for byte: `msa\0` is the signature reversed, and it is refused
/// for the reason it should be rather than for a missing export it may also have.
#[test]
fn the_magic_is_checked_before_the_export_table_is_read() {
    let (mut bytes, _) = complete_wasm();
    bytes[0] = b'x';
    let report = WasmService::validate_binary(&bytes);
    assert!(!report.wasm_magic_ok);
    assert!(!report.valid);
}

/// A well formed module that exports nothing: the magic is fine, and the refusal names
/// every export the ABI needs. This is what an uploaded "hello world" or a stripped
/// binary looks like to the console.
#[test]
fn a_module_missing_the_whole_abi_names_every_required_export() {
    let bytes = empty_wasm();
    let report = WasmService::validate_binary(&bytes);
    assert!(report.wasm_magic_ok, "it is a WASM module");
    assert!(!report.valid, "and it is not a game host");
    assert_eq!(report.export_count, 0);
    assert!(
        report.exports_found.is_empty(),
        "an empty module exports nothing"
    );

    let declared = abi_exports();
    for name in &declared {
        assert!(
            report.exports_missing.iter().any(|missing| missing == name),
            "a module with no exports is missing `{name}`; reported {reported:?}",
            reported = report.exports_missing
        );
    }
    assert_eq!(
        report.exports_missing.len(),
        declared.len(),
        "and it is missing exactly the ABI, nothing invented"
    );
}

/// The interesting case for an uploader: a module that implements most of the ABI.
/// Dropping two handlers must report those two and only those two, so the console can
/// tell a developer which functions their guest is missing.
#[test]
fn a_module_missing_two_handlers_reports_exactly_those_two() {
    let declared = abi_exports();
    let missing = ["on_alloc".to_string(), "on_save".to_string()];
    let kept: Vec<String> = declared
        .iter()
        .filter(|name| !missing.iter().any(|drop| drop == *name))
        .cloned()
        .collect();
    assert!(
        kept.len() + 2 == declared.len(),
        "the fixture must actually drop two of the declared exports: {declared:?}"
    );

    let bytes = module_with_exports(&kept);
    let report = WasmService::validate_binary(&bytes);
    assert!(report.wasm_magic_ok);
    assert!(!report.valid, "a guest the host cannot drive is refused");

    for name in &missing {
        assert!(
            report.exports_missing.iter().any(|found| found == name),
            "`{name}` was not exported, so it must be reported missing; got {:?}",
            report.exports_missing
        );
    }
    assert_eq!(
        report.exports_missing.len(),
        missing.len(),
        "and nothing else is reported missing"
    );
    for name in &kept {
        assert!(
            !report.exports_missing.iter().any(|found| found == name),
            "`{name}` is exported, so it cannot be in the missing list"
        );
        assert!(
            report.exports_found.iter().any(|found| found == name),
            "`{name}` is exported, so it must be in the found list"
        );
    }
    assert_eq!(report.export_count as usize, kept.len());
}

/// An export under the right name but the wrong kind — a global called `on_tick`, say —
/// is not a handler. The table is read as a table, not as a bag of strings.
#[test]
fn an_export_of_the_wrong_kind_is_not_a_handler() {
    let mut bytes = empty_wasm();
    // Function section (3) with one function of type () -> (), so indices line up…
    bytes.extend(section(1, &functype_entry()));
    bytes.extend(section(3, &[0x01, 0x00]));
    // …and then export the single function as a *global* (kind 0x03) named `on_tick`.
    let mut exports = leb128(1);
    exports.extend(leb128("on_tick".len()));
    exports.extend(b"on_tick");
    exports.push(0x03);
    exports.extend(leb128(0));
    bytes.extend(section(7, &exports));
    bytes.extend(section(10, &[0x01, 0x02, 0x00, 0x0b]));

    let report = WasmService::validate_binary(&bytes);
    assert!(report.wasm_magic_ok, "the container is a module");
    assert!(
        !report.exports_found.iter().any(|found| found == "on_tick"),
        "a global is not an exported function: {:?}",
        report.exports_found
    );
    assert!(!report.valid, "so the module is still not a game host");
}

/// Two modules that differ only in their export names must not be confused, because the
/// console's answer to an upload is the export list it just read.
#[test]
fn the_report_counts_the_exports_a_module_actually_has() {
    let declared = abi_exports();
    let (full, _) = complete_wasm();
    let partial = module_with_exports(&declared[..declared.len() - 1]);

    let full_report = WasmService::validate_binary(&full);
    let partial_report = WasmService::validate_binary(&partial);
    assert_eq!(full_report.export_count as usize, declared.len());
    assert_eq!(partial_report.export_count as usize, declared.len() - 1);
    assert_eq!(
        partial_report.exports_missing.len(),
        1,
        "one export short of the ABI is one missing handler"
    );
    assert_eq!(
        partial_report.exports_missing[0],
        declared[declared.len() - 1],
        "and it is the one that was left off"
    );
}

// ---------------------------------------------------------------------------
// The request shape and the mounted routes.
// ---------------------------------------------------------------------------

/// `POST …/wasm/activate` names a version, and nothing else: the version is the whole
/// decision, so a client cannot choose which module is active by other means.
#[test]
fn an_activate_request_carries_only_a_version() {
    let request: ActivateRequest = serde_json::from_value(serde_json::json!({"version": 2}))
        .expect("the documented body deserializes");
    assert_eq!(request.version, 2);

    // A body that tries to say more than the version says nothing more: the shape has
    // no field for it.
    let greedy: ActivateRequest = serde_json::from_value(serde_json::json!({
        "version": 7,
        "filename": "room.wasm",
        "binary_bytes": "AAAA",
    }))
    .expect("extra fields are dropped rather than refused");
    let printed = format!("{greedy:?}");
    assert_eq!(greedy.version, 7);
    assert!(
        !printed.contains("room.wasm") && !printed.contains("AAAA"),
        "an activation cannot smuggle a payload: {printed}"
    );

    // An absent version is the zero version — a number no upload ever produces, since
    // `insert_module` starts at 1, so an empty body can only ever be a 404.
    let empty: ActivateRequest =
        serde_json::from_value(serde_json::json!({})).expect("the body has no required field");
    assert_eq!(empty.version, 0);
}

/// The module surface is mounted under its game, on every one of its paths — and it is
/// a console surface, so an anonymous caller is refused before anything else is read.
#[test]
fn every_wasm_route_demands_a_console_token() {
    let server = V2Server::start(v2_state());
    for (method, path) in wasm_paths("00000000-0000-0000-0000-000000000000") {
        let reply = server.send(&method, &path, None, "{}");
        assert_eq!(
            reply.refusal(UNAUTHENTICATED),
            "Console token required",
            "{method} {path}"
        );
    }
}

/// Every path of the module surface, with the method it answers. One list, because the
/// two tests below — the anonymous refusal and the malformed-id refusal — owe the same
/// routes an answer, and a path added to the controller belongs in both.
fn wasm_paths(game: &str) -> Vec<(&'static str, String)> {
    let base = format!("/v2/console/games/{game}/wasm");
    vec![
        ("GET", base.clone()),
        ("POST", base.clone()),
        ("POST", format!("{base}/upload")),
        ("POST", format!("{base}/activate")),
        ("POST", format!("{base}/rollback")),
        ("DELETE", format!("{base}/1")),
    ]
}

/// A game id that is not a UUID is refused as the game it names, on every module path,
/// before the mode gate or the pool is reached — the same wording the games CRUD uses,
/// because it is the same mistake.
#[test]
fn a_malformed_module_game_id_is_a_bad_request_naming_the_game() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    for (method, path) in wasm_paths("not-a-uuid") {
        let body = if method == "DELETE" || method == "GET" {
            String::new()
        } else {
            r#"{"version":1}"#.to_owned()
        };
        let reply = server.send(&method, &path, Some(&auth), &body);
        assert_eq!(
            reply.refusal(INVALID_ARGUMENT),
            "Invalid game id.",
            "{method} {path}"
        );
    }
}

/// With a token, each path is *registered*: axum answers 404 for a path no controller
/// claimed and 405 for a path claimed with another method, and neither is an acceptable
/// answer here. Everything else — the console's 503 for a plane with no database, a 400
/// for a body this harness deliberately does not form — is the handler's business.
#[test]
fn the_wasm_routes_are_mounted_under_their_game() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    let game = uuid::Uuid::new_v4().to_string();
    let paths = wasm_paths(&game);

    for (method, path) in paths {
        // An upload is multipart or nothing, and this harness deliberately forms none:
        // the framing refusal answers before the pool does. Every other route gets as
        // far as the database it needs, which is the mode gate's own query.
        let uploads = path.ends_with("/upload") || path.ends_with("/wasm");
        let body = match (method.as_ref(), uploads) {
            ("POST", true) => "{}".to_owned(),
            ("POST", false) => r#"{"version":1}"#.to_owned(),
            _ => String::new(),
        };
        let reply = server.send(&method, &path, Some(&auth), &body);
        assert_ne!(reply.status, 404, "{method} {path} is not mounted at all");
        assert_ne!(
            reply.status, 405,
            "{method} {path} is mounted but does not claim the method: {}",
            reply.body
        );
        if uploads && method == "POST" {
            let message = reply.refusal(INVALID_ARGUMENT);
            assert!(
                message.to_lowercase().contains("multipart"),
                "{method} {path} is mounted and refuses an unformed body by naming \
                 the framing: {message}"
            );
        } else {
            assert_eq!(
                reply.refusal(crate::error::UNAVAILABLE),
                "The database is not configured, set [control.session] database_url.",
                "{method} {path}"
            );
        }
    }
}

/// Listing a game's modules is a read the console answers from the database, so on a
/// plane with no DSN it is the database that stops it — the same sound the games reads
/// make, and the proof that the route reaches its handler.
#[test]
fn the_wasm_listing_reaches_the_database_it_needs() {
    let server = V2Server::start(v2_state());
    let auth = format!(
        "Bearer {}",
        console::mint("", "wasm-probe", "wasm@console.local", ConsoleRole::Admin)
    );
    let game = uuid::Uuid::new_v4();
    let reply = server.get(&format!("/v2/console/games/{game}/wasm"), Some(&auth));
    assert_eq!(
        reply.refusal(crate::error::UNAVAILABLE),
        "The database is not configured, set [control.session] database_url."
    );
}

// ---------------------------------------------------------------------------
// The refusal an invalid module earns — pure, and the text an operator reads.
// ---------------------------------------------------------------------------

/// Not-wasm and wrong-ABI are different mistakes and the gate says which, because the
/// developer's next action differs: re-export, or upload the right file.
#[test]
fn ensure_valid_names_the_mistake_the_report_found() {
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

// ---------------------------------------------------------------------------
// Database-gated: the engine-mode guard and the module lifecycle itself. Skipped with a
// printed reason when no Postgres is configured (see `games::db`).
//
// The guard comes first because it is the gate every one of these calls passes through:
// a `relay` game — which is what `unity`, `unreal` and `godot` register as — has no
// server-side module for a room to boot, so the surface refuses it outright rather than
// storing bytes no runtime would ever load. The lifecycle fixtures are therefore
// `db::create_wasm` games (`custom`, authoritative): the same rows, on the plane where
// `room_modules` means something.
// ---------------------------------------------------------------------------

/// A stored row's own SHA-256, computed the way the upload path computes it.
fn sha256_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

fn params(
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

fn skipped_without_database() {
    tracing::info!(
        "skipping: no Postgres configured — set OX_DATABASE_URL or DATABASE_URL to run the \
         WASM upload integration tests"
    );
}

/// `insert_module` numbers versions monotonically per game and moves the active flag in
/// the same transaction: two rows may never both claim to be what a room boots.
#[tokio::test]
async fn insert_module_versions_monotonically_and_deactivates_the_older() {
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

    // The games side reads the same rows: the detail view's aggregates and the upload
    // store cannot disagree about what a game owns.
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

    // A second game's versions count from one: monotonicity is per tenant, not global.
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

/// Activation is one flip in either direction: the newest version after an upload, an
/// older one after a rollback — and the database never holds two active rows.
#[tokio::test]
async fn activate_module_moves_the_flag_and_rollback_is_the_same_flip() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-activate").await;
    let (bytes, _) = complete_wasm();

    for version in [1, 2, 3] {
        service
            .insert_module(
                &state,
                game.id,
                params(&format!("room-v{version}.wasm"), &bytes, None),
            )
            .await
            .expect("three versions are stored");
    }
    let active: Vec<i32> = service
        .list_modules(&state, game.id)
        .await
        .expect("the listing reads")
        .iter()
        .filter(|module| module.is_active)
        .map(|module| module.version)
        .collect();
    assert_eq!(active, vec![3], "the third upload is the active one");

    // Roll back to 1: activation of an older number is the same operation, and the
    // reply is the row that is now active.
    let rolled_back = service
        .activate_module(&state, game.id, 1, engine_bus::ReloadKind::Rollback)
        .await
        .expect("an older version can be activated");
    assert_eq!(rolled_back.version, 1);
    assert!(rolled_back.is_active);
    let list = service
        .list_modules(&state, game.id)
        .await
        .expect("listing");
    assert_eq!(
        list.iter()
            .filter(|module| module.is_active)
            .map(|module| module.version)
            .collect::<Vec<_>>(),
        vec![1],
        "activating one version deactivates every other, including the newest"
    );

    // Forward again through the same call, and an unknown version is a refusal rather
    // than a flip that leaves the game with no active module at all.
    let forward = service
        .activate_module(&state, game.id, 3, engine_bus::ReloadKind::Upload)
        .await
        .expect("activation is not one-directional");
    assert_eq!(forward.version, 3);
    let err = service
        .activate_module(&state, game.id, 99, engine_bus::ReloadKind::Rollback)
        .await
        .expect_err("a version nobody uploaded cannot be activated");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");
    assert_eq!(
        service
            .list_modules(&state, game.id)
            .await
            .expect("the listing still reads")
            .iter()
            .filter(|module| module.is_active)
            .count(),
        1,
        "a failed activation leaves the active version active"
    );

    db::forget(&pool, game.id).await;
}

/// The truth a rollback needs: the bytes of an older version are still fetchable, and
/// they are byte-identical to what was uploaded.
#[tokio::test]
async fn get_module_bytes_round_trips_what_was_stored() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-bytes").await;
    let (bytes, _) = complete_wasm();

    service
        .insert_module(&state, game.id, params("room-v1.wasm", &bytes, None))
        .await
        .expect("version 1 is stored");

    let (filename, stored) = service
        .get_module_bytes(&state, game.id, 1)
        .await
        .expect("the hot-reload fetch reads the version it was told to load");
    assert_eq!(filename, "room-v1.wasm");
    assert_eq!(
        stored, bytes,
        "the database is a faithful store, not a transform"
    );

    let err = service
        .get_module_bytes(&state, game.id, 7)
        .await
        .expect_err("a version that was never uploaded has no bytes");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}

/// The active version is the one a live sim would reload from, so it is the one version
/// `delete_module` will not remove.
#[tokio::test]
async fn delete_module_refuses_the_active_version() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-delete").await;
    let (bytes, _) = complete_wasm();

    for version in [1, 2] {
        service
            .insert_module(
                &state,
                game.id,
                params(&format!("room-v{version}.wasm"), &bytes, None),
            )
            .await
            .expect("two versions, the second active");
    }

    // The stale one goes.
    service
        .delete_module(&state, game.id, 1)
        .await
        .expect("an inactive version is deletable");
    let list = service
        .list_modules(&state, game.id)
        .await
        .expect("listing");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].version, 2);

    // The active one does not.
    let err = service
        .delete_module(&state, game.id, 2)
        .await
        .expect_err("deleting the active module would strand every live room");
    assert_eq!(err.kind, crate::error::FAILED_PRECONDITION, "{err:?}");
    assert!(
        err.message.contains("active"),
        "and the refusal says what to do instead: {}",
        err.message
    );
    assert_eq!(
        service
            .list_modules(&state, game.id)
            .await
            .expect("listing")
            .len(),
        1,
        "the refusal did not delete anything"
    );

    // A newer upload takes the flag, which is what makes the older one removable: the
    // rule is about the flag, not about the version number.
    let fresh = service
        .insert_module(&state, game.id, params("room-v3.wasm", &bytes, None))
        .await
        .expect("a third upload becomes active");
    assert_eq!(fresh.version, 3);
    assert!(fresh.is_active);
    service
        .delete_module(&state, game.id, 2)
        .await
        .expect("version 2 is inactive now, and so is removable");

    let err = service
        .delete_module(&state, game.id, 42)
        .await
        .expect_err("deleting a version nobody uploaded is not a success");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    db::forget(&pool, game.id).await;
}

/// The whole upload as the console speaks it: the tenant is checked, the ABI gate runs
/// before anything is stored, and the report rides back on success as well as failure.
#[tokio::test]
async fn upload_module_gates_on_tenant_then_abi_then_stores() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_wasm(&state, "wasm-upload").await;
    let (bytes, _) = complete_wasm();

    // A game that does not exist is a 404 before the bytes are even looked at.
    let err = service
        .upload_module(
            &state,
            uuid::Uuid::new_v4(),
            "room.wasm".to_owned(),
            bytes.clone(),
            None,
        )
        .await
        .expect_err("no tenant, no module");
    assert_eq!(err.kind, crate::error::NOT_FOUND, "{err:?}");

    // An empty `file` field is the multipart framing's mistake, refused as such.
    let err = service
        .upload_module(&state, game.id, String::new(), bytes.clone(), None)
        .await
        .expect_err("an upload has to name its file");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");

    // A module that fails the ABI gate is refused *and stored nowhere*.
    let err = service
        .upload_module(&state, game.id, "hello.wasm".to_owned(), empty_wasm(), None)
        .await
        .expect_err("an empty module is not a game host");
    assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");
    assert_eq!(
        crate::services::games::GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        0,
        "a refused upload leaves no row behind"
    );

    // A good upload answers with the row and the verdict, and becomes the active
    // version at once.
    let response = service
        .upload_module(
            &state,
            game.id,
            "room-logic.wasm".to_owned(),
            bytes.clone(),
            Some("export function on_tick(dt: f64): i64 { return 0; }".to_owned()),
        )
        .await
        .expect("a module with the whole ABI uploads");
    assert_eq!(response.module.version, 1);
    assert!(response.module.is_active);
    assert_eq!(response.module.filename, "room-logic.wasm");
    assert_eq!(response.module.sha256, sha256_of(&bytes));
    assert!(response.abi.valid, "the report says what was checked");
    assert!(response.abi.exports_missing.is_empty());

    // The report rides back on the wire too — that is why `UploadResponse` carries it.
    let wire = serde_json::to_value(&response).expect("the upload response serializes");
    assert!(wire["module"]["version"].is_number(), "{wire}");
    assert_eq!(wire["abi"]["valid"], serde_json::json!(true));
    assert!(wire["abi"]["exports_found"].is_array(), "{wire}");

    // And the bytes the sim will hot-reload onto are the ones that were uploaded.
    let (filename, stored) = service
        .get_module_bytes(&state, game.id, 1)
        .await
        .expect("the stored version is fetchable");
    assert_eq!(filename, "room-logic.wasm");
    assert_eq!(stored, bytes);

    db::forget(&pool, game.id).await;
}

/// The mode guard, on every write and read this surface offers. A relay game's rooms
/// live in the C# relay, so a stored module is bytes no runtime would load: upload,
/// listing, activation and deletion all answer the same 400, and none of them leaves a
/// row behind. `insert_module` is deliberately *not* on this list — it is the store the
/// guarded `upload_module` calls, and the games side asserts its rows directly.
#[tokio::test]
async fn a_relay_game_is_refused_by_every_wasm_route_it_could_reach() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_engine(&state, "wasm-relay", Some("unity")).await;
    assert_eq!(game.backend_mode, "relay", "the fixture is a relay game");
    let (bytes, _) = complete_wasm();

    let refused = [
        service
            .upload_module(&state, game.id, "room.wasm".to_owned(), bytes.clone(), None)
            .await
            .map(|_| ())
            .expect_err("a relay game cannot upload a module"),
        service
            .list_modules(&state, game.id)
            .await
            .map(|_| ())
            .expect_err("and cannot list one it never stored"),
        service
            .activate_module(&state, game.id, 1, engine_bus::ReloadKind::Upload)
            .await
            .map(|_| ())
            .expect_err("or activate one"),
        service
            .delete_module(&state, game.id, 1)
            .await
            .map(|_| ())
            .expect_err("or delete one"),
    ];
    for err in refused {
        assert_eq!(err.kind, crate::error::INVALID_ARGUMENT, "{err:?}");
        assert_eq!(
            err.message, RELAY_REFUSAL,
            "the refusal is the plan's sentence"
        );
    }

    // Refused means refused: the surface stores nothing for a game it will not run.
    assert_eq!(
        crate::services::games::GamesService::module_count(&state, game.id)
            .await
            .expect("the count reads"),
        0,
        "a refused upload leaves no row behind"
    );

    // The same tenant on the other plane answers for the module it does have, so the
    // refusal is about the mode and not about the game's existence.
    let wasm_game = db::create_wasm(&state, "wasm-relay-control").await;
    service
        .upload_module(&state, wasm_game.id, "room.wasm".to_owned(), bytes, None)
        .await
        .expect("an authoritative game uploads the same bytes");
    assert_eq!(
        crate::services::games::GamesService::module_count(&state, wasm_game.id)
            .await
            .expect("the count reads"),
        1
    );

    db::forget(&pool, game.id).await;
    db::forget(&pool, wasm_game.id).await;
}

/// The guard reads the stored mode, not the mode the game was born with: a tenant that
/// moves from the relay to the authoritative plane can upload the moment the patch
/// lands, and loses the surface again the moment it moves back. A cached verdict would
/// strand a room that is already running on the other plane.
#[tokio::test]
async fn the_wasm_guard_follows_a_game_across_its_backend_mode() {
    let Some((state, pool)) = db::state().await else {
        return skipped_without_database();
    };
    let service = WasmService;
    let game = db::create_engine(&state, "wasm-switch", Some("godot")).await;
    let (bytes, _) = complete_wasm();

    let err = service
        .upload_module(&state, game.id, "room.wasm".to_owned(), bytes.clone(), None)
        .await
        .expect_err("a godot game is a relay game");
    assert_eq!(err.message, RELAY_REFUSAL);

    let patch: crate::dto::console::games::UpdateGameRequest =
        serde_json::from_value(serde_json::json!({"engine_type": "custom"}))
            .expect("an engine patch");
    let moved = crate::services::games::GamesService::update_game(&state, game.id, &patch)
        .await
        .expect("the game becomes authoritative");
    assert_eq!(moved.backend_mode, "wasm");

    service
        .upload_module(&state, game.id, "room.wasm".to_owned(), bytes, None)
        .await
        .expect("and the very next upload is accepted");

    let back: crate::dto::console::games::UpdateGameRequest =
        serde_json::from_value(serde_json::json!({"engine_type": "unity"})).expect("a patch back");
    crate::services::games::GamesService::update_game(&state, game.id, &back)
        .await
        .expect("the game moves back to the relay");
    let err = service
        .list_modules(&state, game.id)
        .await
        .expect_err("and its stored module is no longer this surface's business");
    assert_eq!(err.message, RELAY_REFUSAL);

    db::forget(&pool, game.id).await;
}
