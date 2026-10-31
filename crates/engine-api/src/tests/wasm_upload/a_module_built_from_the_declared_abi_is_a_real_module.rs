use super::*;

#[test]
pub(crate) fn a_module_built_from_the_declared_abi_is_a_real_module() {
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
#[test]
pub(crate) fn a_blob_that_is_not_wasm_fails_the_magic_check() {
    for candidate in [
        b"MZ\0\0 this is a PE, not a module".to_vec(),
        b"wat".to_vec(),
        b"\0asm".to_vec(),
        Vec::new(),
    ] {
        let report = WasmService::validate_binary(&candidate);
        assert!(!report.valid, "{candidate:?} is not a game module");
        if candidate.len() >= 4 && &candidate[..4] == b"\0asm" {
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
#[test]
pub(crate) fn the_magic_is_checked_before_the_export_table_is_read() {
    let (mut bytes, _) = complete_wasm();
    bytes[0] = b'x';
    let report = WasmService::validate_binary(&bytes);
    assert!(!report.wasm_magic_ok);
    assert!(!report.valid);
}
#[test]
pub(crate) fn a_module_missing_the_whole_abi_names_every_required_export() {
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
#[test]
pub(crate) fn a_module_missing_two_handlers_reports_exactly_those_two() {
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
#[test]
pub(crate) fn an_export_of_the_wrong_kind_is_not_a_handler() {
    let mut bytes = empty_wasm();
    bytes.extend(section(1, &functype_entry()));
    bytes.extend(section(3, &[0x01, 0x00]));
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
