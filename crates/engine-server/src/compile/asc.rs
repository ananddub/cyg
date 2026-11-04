use std::path::Path;

use crate::compile::toolchain::detect_room_class;
use crate::compile::toolchain::run;
use crate::compile::toolchain::which;

pub(crate) fn compile_asc(
    project_dir: &Path,
    src: &Path,
    out: &Path,
    max_memory_mb: u32,
) -> std::io::Result<()> {
    let content = std::fs::read_to_string(src)?;

    // The generated bindings decide how the bridge calls onJoin: a schema
    // with a `JoinOptions` def exports `decodeJoinOptions` and the room's
    // onJoin is typed; otherwise the join options arrive as raw JSON.
    let gen_path = project_dir.join("gen").join("cyg.schema.ts");
    let typed_join = std::fs::read_to_string(&gen_path)
        .map(|generated| generated.contains("export function decodeJoinOptions"))
        .unwrap_or(false);
    // The bridge calls the developer's methods directly, so it must match
    // the room's own parameter counts: onJoin(id, options) or
    // onJoin(id, options, auth); onCreate(max) or onCreate(max, options).
    // The hooks may take the raw player id or a Colyseus-shaped Client
    // handle — the bridge matches whichever the room declared.
    let takes_client = |hook: &str| content.contains(&format!("{hook}(client: Client"));
    let room_join_takes_client = takes_client("onJoin");
    let room_takes_auth = content.contains("onJoin(") && content.contains(", auth: string)");
    let room_takes_create_options = content.contains("onCreate(max: i32, options: string)");

    let on_join_body = if typed_join {
        if room_takes_auth {
            "    const auth = len2 > 0 ? String.UTF8.decodeUnsafe(auth_ptr as usize, len2) : \"\";
    __room_instance.onJoin(
        {join_client},
        decodeJoinOptions(new InputReader(ptr, len)),
        auth
    );"
        } else {
            "    __room_instance.onJoin({join_client}, decodeJoinOptions(new InputReader(ptr, len)));"
        }
    } else {
                if room_takes_auth {
            "    const options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : \"\";
    const auth = len2 > 0 ? String.UTF8.decodeUnsafe(auth_ptr as usize, len2) : \"\";
    __room_instance.onJoin({join_client}, options, auth);"
        } else {
            "    const options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : \"\";
    __room_instance.onJoin({join_client}, options);"
        }
    };

    // Optional lifecycle hooks are exported only when the room declares
    // them — the base class no longer ships defaults (strict AS overrides),
    // and the host probes these exports optionally.
    let takes_client_for = |hook: &str, fallback: &str| -> String {
        if takes_client(hook) {
            "new Client(id, __room_instance)".to_string()
        } else {
            fallback.to_string()
        }
    };
    let on_leave_body = if content.contains("onLeave(") {
        format!(
            "export function on_leave(id: i64, code: i32): void {{\n    __room_instance.onLeave({}, code);\n}}",
            takes_client_for("onLeave", "id")
        )
    } else {
        "export function on_leave(id: i64, code: i32): void {}".to_string()
    };
    let on_drop_body = if content.contains("onDrop(") {
        format!(
            "export function on_drop(id: i64, code: i32): void {{\n    __room_instance.onDrop({}, code);\n}}",
            takes_client_for("onDrop", "id")
        )
    } else {
        String::new()
    };
    let on_before_shutdown_body = if content.contains("onBeforeShutdown(") {
        "export function on_before_shutdown(): void {\n    __room_instance.onBeforeShutdown();\n}".to_string()
    } else {
        String::new()
    };
    let on_uncaught_exception_body = if content.contains("onUncaughtException(") {
        "export function on_uncaught_exception(msg_ptr: i32, msg_len: i32, method_ptr: i32, method_len: i32): void {\n    const message = msg_len > 0 ? String.UTF8.decodeUnsafe(msg_ptr as usize, msg_len) : \"\";\n    const method = method_len > 0 ? String.UTF8.decodeUnsafe(method_ptr as usize, method_len) : \"\";\n    __room_instance.onUncaughtException(message, method);\n}".to_string()
    } else {
        String::new()
    };
    let on_auth_body = if content.contains("onAuth(") {
        format!(
            "export function on_auth(id: i64, opt_ptr: i32, opt_len: i32, ctx_ptr: i32, ctx_len: i32): i64 {{\n    const options = opt_len > 0 ? String.UTF8.decodeUnsafe(opt_ptr as usize, opt_len) : \"\";\n    const context = ctx_len > 0 ? String.UTF8.decodeUnsafe(ctx_ptr as usize, ctx_len) : \"\";\n    const auth = __room_instance.onAuth({}, options, context);   // throws = reject\n    const bytes = String.UTF8.encode(auth);\n    const out = __room_instance.onAlloc(bytes.byteLength);\n    memory.copy(out, changetype<usize>(bytes), bytes.byteLength);\n    return ((out as i64) << 32) | (bytes.byteLength as i64);\n}}",
            takes_client_for("onAuth", "id")
        )
    } else {
        "export function on_auth(id: i64, opt_ptr: i32, opt_len: i32, ctx_ptr: i32, ctx_len: i32): i64 {\n    return 0;\n}".to_string()
    };
    let on_reconnect_body = if content.contains("onReconnect(") {
        format!(
            "export function on_reconnect(id: i64): void {{\n    __room_instance.onReconnect({});\n}}",
            takes_client_for("onReconnect", "id")
        )
    } else {
        String::new()
    };

    // The join bodies carry a {join_client} placeholder — substitute the
    // argument matching the room's declared first parameter.
    let join_client_expr = if room_join_takes_client {
        "new Client(id, __room_instance)"
    } else {
        "id"
    };
    let on_join_body = on_join_body.replace("{join_client}", join_client_expr);

    // onInputFrame is the hot path: when the room takes a Client, the
    // bridge dispatches through ONE reusable handle — no per-frame alloc.
    // When the room takes the typed frame (any param but InputReader), the
    // bridge calls the generated decodeInput itself — the room never sees a
    // reader. Reader-style rooms keep full control.
    let input_takes_client = takes_client("onInputFrame");
    let gen_has_input_decoder = std::fs::read_to_string(&gen_path)
        .map(|generated| generated.contains("export function decodeInput"))
        .unwrap_or(false);
    let input_reader_style = content.contains("onInputFrame(") && content.contains(": InputReader");
    let input_frame_expr = if input_reader_style || !gen_has_input_decoder {
        "new InputReader(ptr + 4 + type_len, len - 4 - type_len)".to_string()
    } else {
        "decodeInput(new InputReader(ptr + 4 + type_len, len - 4 - type_len))".to_string()
    };
    let on_input_body = if input_takes_client {
        format!(
            r#"const __input_client = new Client(0, __room_instance);
const __input_last_seq = new Map<i64, u16>();

export function on_input(id: i64, ptr: i32, len: i32): void {{
    // reserved __input lane: [0x05][type_len]["__input"][seq u16 LE][frame]
    if (len >= 9 && load<u8>(ptr) == 0x05) {{
        const type_len: i32 = load<u8>(ptr + 1) as i32;
        if (type_len == 7 && len >= 4 + type_len && String.UTF8.decodeUnsafe(ptr + 2, 7) == "__input") {{
            const seq: u16 = (load<u8>(ptr + 2 + type_len) as u16) | ((load<u8>(ptr + 3 + type_len) as u16) << 8);
            const prev = __input_last_seq.has(id) ? __input_last_seq.get(id) : 0;
            const diff: u16 = prev - seq;
            if (__input_last_seq.has(id) && diff < 32768) return;   // stale
            __input_last_seq.set(id, seq);
            __input_client.id = id;
            __room_instance.onInputFrame(
                __input_client,
                {input_frame_expr}
            );
            return;
        }}
    }}
    __room_instance.onInput(id, ptr as usize, len);
}}"#
        )
    } else {
        "export function on_input(id: i64, ptr: i32, len: i32): void {\n    __room_instance.onInput(id, ptr as usize, len);\n}".to_string()
    };

    let needs_client = ["onJoin", "onAuth", "onLeave", "onDrop", "onReconnect", "onInputFrame"]
        .iter()
        .any(|hook| takes_client(hook));
    let gen_imports = {
        let mut imports = Vec::new();
        if typed_join {
            imports.push("decodeJoinOptions");
        }
        if !input_reader_style && gen_has_input_decoder {
            imports.push("decodeInput");
        }
        if imports.is_empty() {
            String::new()
        } else {
            format!("import {{ {} }} from \"../gen/cyg.schema\";\n", imports.join(", "))
        }
    };
    let join_options_import = if typed_join || (!input_reader_style && gen_has_input_decoder) {
        if needs_client {
            format!("import {{ InputReader, Client }} from \"../pkg/cyg-server\";\n{gen_imports}")
        } else {
            format!("import {{ InputReader }} from \"../pkg/cyg-server\";\n{gen_imports}")
        }
    } else if needs_client {
        "import { Client } from \"../pkg/cyg-server\";".to_string()
    } else {
        String::new()
    };
    let on_create_body = if room_takes_create_options {
        "export function on_create(max: i32, ptr: i32, len: i32): void {
    const options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : \"\";\n    __room_instance.onCreate(max, options);\n}"
    } else {
        "export function on_create(max: i32, ptr: i32, len: i32): void {\n    __room_instance.onCreate(max);\n}"
    };

    let has_manual_flat_bridge =
        content.contains("export function on_tick") || content.contains("export function tick");

    let (entry_to_compile, temp_entry_path) = if !has_manual_flat_bridge {
        if let Some(class_name) = detect_room_class(&content) {
            let src_stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("room");
            let parent = src.parent().unwrap_or_else(|| Path::new("."));
            let temp_entry = parent.join(format!("._{}_entry.ts", src_stem));

            let bridge_code = format!(
                r#"// ⚡ Auto-generated by CYG Engine compiler
// WebAssembly Flat C-ABI Bridge: developers write pure Room classes!
import {{ {class_name} }} from "./{src_stem}";
{join_options_import}

const __room_instance = new {class_name}();

{on_create_body}

export function on_join(id: i64, ptr: i32, len: i32, auth_ptr: i32, len2: i32): void {{
{on_join_body}
}}

{on_leave_body}

{on_drop_body}

{on_before_shutdown_body}

{on_uncaught_exception_body}

{on_input_body}

export function on_tick(dt: f64): i64 {{
    __room_instance.onBeforePatch(dt);
    const packed = __room_instance.onTick(dt);
    __room_instance.onAfterPatch(dt);
    return packed;
}}

export function on_patch_due(due: i32): void {{
    __room_instance.setPatchDue(due != 0);
}}

export function on_save(): i64 {{
    return __room_instance.onSave();
}}

export function on_load(ptr: i32, len: i32): void {{
    __room_instance.onLoad(ptr as usize, len);
}}

export function on_alloc(len: i32): i32 {{
    return __room_instance.onAlloc(len);
}}

export function on_is_full(count: i32): bool {{
    return __room_instance.onIsFull(count);
}}

{on_auth_body}

{on_reconnect_body}

export function on_dispose(): void {{
    __room_instance.onDispose();
}}

export function on_allow_reconnection(id: i64): i32 {{
    return __room_instance.allowReconnection(id);
}}

export function on_patch_rate(): i32 {{
    return __room_instance.getPatchRate();
}}

export function on_simulation_rate(): i32 {{
    return __room_instance.getSimulationRate();
}}
"#,
                class_name = class_name,
                src_stem = src_stem,
                join_options_import = join_options_import,
                on_join_body = on_join_body,
                on_create_body = on_create_body,
                on_leave_body = on_leave_body,
                on_drop_body = on_drop_body,
                on_before_shutdown_body = on_before_shutdown_body,
                on_uncaught_exception_body = on_uncaught_exception_body,
                on_auth_body = on_auth_body,
                on_reconnect_body = on_reconnect_body,
                on_input_body = on_input_body,
            );

            std::fs::write(&temp_entry, bridge_code)?;
            tracing::info!(
                "[build] 🪄 auto-generating WebAssembly Flat ABI bridge for '{class_name}'..."
            );
            (temp_entry.clone(), Some(temp_entry))
        } else {
            (src.to_path_buf(), None)
        }
    } else {
        (src.to_path_buf(), None)
    };

    let args = [
        entry_to_compile.to_string_lossy().into_owned(),
        "-Ospeed".into(),
        "--noAssert".into(),
        // A declared linear-memory maximum is required by the pooling
        // allocator (wasm_pooling): bounded guests get pooled instance slots.
        "--maximumMemory".into(),
        format!("{}", max_memory_mb.max(1) * 16).into(), // MiB -> 64 KiB pages
        "--runtime".into(),
        "stub".into(),
        "-o".into(),
        out.to_string_lossy().into_owned(),
    ];

    let res = if which("asc") {
        run("asc", &args)
    } else {
        tracing::info!(
            "[build] asc not on PATH — using npx assemblyscript (first run downloads)..."
        );
        let mut npx_args = vec![
            "-y".into(),
            "-p".into(),
            "assemblyscript@0.28.20".into(),
            "asc".into(),
        ];
        npx_args.extend(args.iter().cloned());
        run("npx", &npx_args)
    };

    if let Some(ref temp_path) = temp_entry_path {
        let _ = std::fs::remove_file(temp_path);
    }

    res
}
