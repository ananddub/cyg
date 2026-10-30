use super::*;

impl WasmService {
    pub fn validate_binary(bytes: &[u8]) -> AbiReport {
        let mut report = AbiReport {
            valid: false,
            wasm_magic_ok: bytes.starts_with(b"\0asm"),
            exports_found: Vec::new(),
            exports_missing: Vec::new(),
            export_count: 0,
            size_bytes: bytes.len(),
        };
        if !report.wasm_magic_ok {
            report.exports_missing = REQUIRED_EXPORTS.iter().map(|s| (*s).to_owned()).collect();
            return report;
        }

        let mut functions: Vec<String> = Vec::new();
        let mut parsed = true;
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            match payload {
                Ok(wasmparser::Payload::ExportSection(section)) => {
                    for entry in section {
                        match entry {
                            Ok(export) if export.kind == wasmparser::ExternalKind::Func => {
                                functions.push(export.name.to_owned());
                            }
                            Ok(_) => {}
                            Err(_) => {
                                parsed = false;
                                break;
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    parsed = false;
                    break;
                }
            }
        }
        report.export_count = functions.len() as u32;

        let validated = parsed && wasmparser::Validator::new().validate_all(bytes).is_ok();
        if validated {
            for name in REQUIRED_EXPORTS {
                if functions.iter().any(|f| f == name) {
                    report.exports_found.push(name.to_owned());
                } else {
                    report.exports_missing.push(name.to_owned());
                }
            }
        } else {
            report.exports_missing = REQUIRED_EXPORTS.iter().map(|s| (*s).to_owned()).collect();
        }
        report.valid = validated && report.exports_missing.is_empty();
        report
    }
    pub fn ensure_valid(report: &AbiReport) -> Result<(), V2Error> {
        if report.valid {
            return Ok(());
        }
        if !report.wasm_magic_ok {
            return Err(V2Error::invalid_argument(
                "Upload is not a WebAssembly binary: missing the \\0asm magic header.",
            ));
        }
        Err(V2Error::invalid_argument(format!(
            "Module does not satisfy the room ABI — missing required exports: {}. \
             A guest must provide {}.",
            report.exports_missing.join(", "),
            REQUIRED_EXPORTS.join(", ")
        )))
    }
    pub(super) fn publish_reload(
        state: &ApiState,
        game_id: Uuid,
        game_key: &str,
        module: &ModuleDto,
        kind: ReloadKind,
        bytes: Vec<u8>,
    ) {
        let _ = state.bus.reloads_tx.try_send(WasmReload {
            game_id: game_id.to_string(),
            game_key: game_key.to_owned(),
            version: module.version,
            sha256: module.sha256.clone(),
            bytes,
            kind,
        });
    }
}
