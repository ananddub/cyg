use super::*;

pub(crate) const HOST_SOURCE: &str =
    include_str!("../../../../../crates/engine-server/src/rooms.rs");
pub(crate) fn abi_exports() -> Vec<String> {
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
pub(crate) fn empty_wasm() -> Vec<u8> {
    vec![0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00]
}
pub(crate) fn leb128(mut value: usize) -> Vec<u8> {
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
pub(crate) fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![id];
    out.extend(leb128(payload.len()));
    out.extend(payload);
    out
}
pub(crate) fn functype_entry() -> Vec<u8> {
    let mut out = leb128(1);
    out.extend([0x60u8, 0x00, 0x00]);
    out
}
pub(crate) fn module_with_exports(names: &[String]) -> Vec<u8> {
    let mut out = empty_wasm();

    out.extend(section(1, &functype_entry()));

    let functions: Vec<u8> = leb128(names.len())
        .into_iter()
        .chain(std::iter::repeat_n(0u8, names.len()))
        .collect();
    out.extend(section(3, &functions));

    let mut exports = leb128(names.len());
    for (index, name) in names.iter().enumerate() {
        exports.extend(leb128(name.len()));
        exports.extend(name.as_bytes());
        exports.push(0x00);
        exports.extend(leb128(index));
    }
    out.extend(section(7, &exports));

    let mut bodies = leb128(names.len());
    for _ in names {
        bodies.extend(leb128(2));
        bodies.extend([0x00, 0x0b]);
    }
    out.extend(section(10, &bodies));

    out
}
pub(crate) fn complete_wasm() -> (Vec<u8>, Vec<String>) {
    let names = abi_exports();
    assert!(
        !names.is_empty(),
        "the host declares at least one ABI export"
    );
    (module_with_exports(&names), names)
}
#[test]
pub(crate) fn the_upload_gate_requires_exactly_what_the_host_invokes() {
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
#[test]
pub(crate) fn the_required_exports_are_the_ones_the_host_declares() {
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
