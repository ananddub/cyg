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
            "export function on_leave(id: i64, code: i32): void {{\n    __room_instance.pluginOnLeave(id, code);\n    __room_instance.onLeave({}, code);\n}}",
            takes_client_for("onLeave", "id")
        )
    } else {
        "export function on_leave(id: i64, code: i32): void {\n    __room_instance.pluginOnLeave(id, code);\n}".to_string()
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
        "export function on_before_shutdown(): void {\n    __room_instance.onBeforeShutdown();\n}"
            .to_string()
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
    let plugin_auth = if room_takes_auth { "auth" } else { "\"\"" };
    let on_join_body = format!(
        "    const __plugin_options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : \"\";\n{on_join_body}\n    __room_instance.pluginOnJoin(id, __plugin_options, {plugin_auth});"
    );

    // onInputFrame is the hot path: when the room takes a Client, the
    // bridge dispatches through ONE reusable handle — no per-frame alloc.
    // When the room takes the typed frame (any param but InputReader), the
    // bridge calls the generated decodeInput itself — the room never sees a
    // reader. Reader-style rooms keep full control.
    let room_has_input_frame = content.contains("onInputFrame(");
    let input_takes_client = takes_client("onInputFrame");
    let gen_has_input_decoder = std::fs::read_to_string(&gen_path)
        .map(|generated| generated.contains("export function decodeInput"))
        .unwrap_or(false);
    let input_reader_style = room_has_input_frame && content.contains(": InputReader");
    // Reuse one `Input` + one `InputReader` for every input frame instead of
    // allocating a pair per call. The guest runtime's GC would eventually
    // reclaim them, but at the input rate that garbage kept the collector busy
    // and inflated the room's linear memory between collections.
    let input_frame_expr = if input_reader_style || !gen_has_input_decoder {
        "__input_reader.reset(body_ptr, body_len)".to_string()
    } else {
        "decodeInputInto(__input_frame, __input_reader.reset(body_ptr, body_len))".to_string()
    };
    let on_input_body = if room_has_input_frame {
        // Timeline recording allocates a copy per accepted frame. Only emit it
        // when the room's source actually touches `this.inputs`; rooms that do
        // not use the timeline keep the old zero-allocation input hot path.
        let record_input = if content.contains("this.inputs") {
            "    // Copy the frame into the SDK timeline before the room hook runs.\n    __room_instance.recordInput(id, seq, body_ptr, body_len);\n"
        } else {
            ""
        };
        let client_decl = if input_takes_client {
            "const __input_client = new Client(0, __room_instance);\n"
        } else {
            ""
        };
        let frame_decl = if !input_reader_style && gen_has_input_decoder {
            "const __input_frame = new Input();\n"
        } else {
            ""
        };
        let dispatch = if input_takes_client {
            format!(
                "__input_client.id = id;\n    __room_instance.onInputFrame(\n        __input_client,\n        {input_frame_expr}\n    );"
            )
        } else {
            format!(
                "__room_instance.onInputFrame(\n        id,\n        {input_frame_expr}\n    );"
            )
        };
        format!(
            r#"{client_decl}const __input_last_seq = new Map<i64, u16>();
{frame_decl}const __input_reader = new InputReader(0, 0);

function __emit_input(id: i64, seq: u16, body_ptr: i32, body_len: i32): void {{
    const prev = __input_last_seq.has(id) ? __input_last_seq.get(id) : 0;
    const diff: u16 = prev - seq;
    if (__input_last_seq.has(id) && diff < 32768) return;   // stale
    __input_last_seq.set(id, seq);
{record_input}    {dispatch}
}}

export function on_input(id: i64, ptr: i32, len: i32): void {{
    __room_instance.pluginOnInput(id);
    if (len >= 9 && load<u8>(ptr) == 0x05) {{
        const type_len: i32 = load<u8>(ptr + 1) as i32;
        // Batch: [0x05][13]["__input_batch"][count][seq u16][len u16][frame]*
        // The host peels the RTT stamp trailer before this runs.
        if (type_len == 13 && len >= 3 + type_len
            && load<u8>(ptr + 2) == 0x5F && load<u8>(ptr + 3) == 0x5F
            && load<u8>(ptr + 4) == 0x69 && load<u8>(ptr + 5) == 0x6E
            && load<u8>(ptr + 6) == 0x70 && load<u8>(ptr + 7) == 0x75
            && load<u8>(ptr + 8) == 0x74 && load<u8>(ptr + 9) == 0x5F
            && load<u8>(ptr + 10) == 0x62 && load<u8>(ptr + 11) == 0x61
            && load<u8>(ptr + 12) == 0x74 && load<u8>(ptr + 13) == 0x63
            && load<u8>(ptr + 14) == 0x68) {{
            let p: i32 = ptr + 2 + type_len;
            const end: i32 = ptr + len;
            if (p + 1 > end) return;
            const count: i32 = load<u8>(p) as i32;
            p += 1;
            if (count < 1 || count > 8) return;
            for (let i: i32 = 0; i < count; i++) {{
                if (p + 4 > end) return;
                const seq: u16 = load<u16>(p);
                const frame_len: i32 = load<u16>(p + 2) as i32;
                p += 4;
                if (frame_len <= 0 || p + frame_len > end) return;
                __emit_input(id, seq, p, frame_len);
                p += frame_len;
            }}
            return;
        }}
        // Reserved __input lane: [0x05][7]["__input"][seq u16 LE][frame]
        if (type_len == 7 && len >= 4 + type_len
            && load<u8>(ptr + 2) == 0x5F && load<u8>(ptr + 3) == 0x5F
            && load<u8>(ptr + 4) == 0x69 && load<u8>(ptr + 5) == 0x6E
            && load<u8>(ptr + 6) == 0x70 && load<u8>(ptr + 7) == 0x75
            && load<u8>(ptr + 8) == 0x74) {{
            const seq: u16 = (load<u8>(ptr + 2 + type_len) as u16) | ((load<u8>(ptr + 3 + type_len) as u16) << 8);
            __emit_input(id, seq, ptr + 4 + type_len, len - 4 - type_len);
            return;
        }}
    }}
    __room_instance.onInput(id, ptr as usize, len);
}}"#
        )
    } else {
        "export function on_input(id: i64, ptr: i32, len: i32): void {\n    __room_instance.pluginOnInput(id);\n    __room_instance.onInput(id, ptr as usize, len);\n}".to_string()
    };

    let needs_client = [
        "onJoin",
        "onAuth",
        "onLeave",
        "onDrop",
        "onReconnect",
        "onInputFrame",
    ]
    .iter()
    .any(|hook| takes_client(hook));
    let gen_imports = {
        let mut imports = Vec::new();
        if typed_join {
            imports.push("decodeJoinOptions");
        }
        if !input_reader_style && gen_has_input_decoder {
            imports.push("decodeInputInto");
            imports.push("Input");
        }
        if imports.is_empty() {
            String::new()
        } else {
            format!(
                "import {{ {} }} from \"../gen/cyg.schema\";\n",
                imports.join(", ")
            )
        }
    };
    // The generated bridge always decodes input through `InputReader` when the
    // room declares `onInputFrame`, even in the raw-reader style. Importing it
    // only for typed-join was a compile error in reader-style rooms.
    let needs_input_reader = typed_join || room_has_input_frame;
    let join_options_import = if needs_input_reader {
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
    // The host passes the operator's `max_clients` as `max`; seat it into
    // `maxPlayers` before the game's own `onCreate` runs. Without this every
    // room that does not remember to do it itself keeps the SDK default of 16
    // and `onIsFull` silently rejects session 17 onwards, whatever
    // config.toml says. A game may still override it inside onCreate.
    let on_create_body = if room_takes_create_options {
        "export function on_create(max: i32, ptr: i32, len: i32): void {
    __room_instance.maxPlayers = max;
    const options = len > 0 ? String.UTF8.decodeUnsafe(ptr as usize, len) : \"\";\n    __room_instance.onCreate(max, options);\n    __room_instance.pluginOnCreate(max, options);\n}"
    } else {
        "export function on_create(max: i32, ptr: i32, len: i32): void {\n    __room_instance.maxPlayers = max;\n    __room_instance.onCreate(max);\n    __room_instance.pluginOnCreate(max, \"\");\n}"
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
    __room_instance.pluginOnTick(dt);
    __room_instance.flushDeferredMessages();
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
    __room_instance.pluginOnDispose();
    __room_instance.onDispose();
}}

/// What the round reported when it ended, or 0 when it never did. The host
/// reads this just before disposing the room and scores it with the same
/// service the REST endpoint uses.
export function on_match_end(): i64 {{
    return __room_instance.on_match_end();
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
        // Room instances are per-room: start the linear memory at 2 pages and
        // let it grow — wasmtime commits only touched pages, but a fat initial
        // declaration makes every pooled slot over-reserve.
        "--initialMemory".into(),
        "2".into(),
        // A declared linear-memory maximum is required by the pooling
        // allocator (wasm_pooling): bounded guests get pooled instance slots.
        "--maximumMemory".into(),
        format!("{}", max_memory_mb.max(1) * 16).into(), // MiB -> 64 KiB pages
        // "stub" has no allocator GC: every object a hook creates is permanent
        // garbage. Under input traffic a room grew from 128 KiB to MiB of linear
        // memory within minutes (measured 16 clients @ 20 Hz: 1 MiB and rising),
        // which is what made 10k CCU need ~200 KiB per client. "incremental"
        // reclaims that garbage and holds a room flat at its initial memory.
        "--runtime".into(),
        "incremental".into(),
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
