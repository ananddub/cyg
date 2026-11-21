# cyg-godot — GDExtension native core (OPTIONAL companion to CYGGodot)

Godot already ships a full GDScript SDK in `sdk/client/godot/` (the
`addons/cookyourgame/` addon — no native plugin needed). This GDExtension is
the OPTIONAL native core for projects that want cyg-core-rs (the same Rust
protocol core as web/Unity/Unreal) instead of the GDScript decoder.

## Build

```bash
cd sdk/client/godot/cyg-godot
cargo build --release
# → target/release/libcyg_godot.so
```

> gdext (`godot = "0.5"`) must support your Godot version (repo tested on
> Godot 4.7). If the pin lags, bump the version / api feature.

## Register in a Godot project

`<project>/cyg.gdextension`:

```ini
[configuration]
entry_symbol = "gdext_rust_init"
compatibility_minimum = "4.3"

[libraries]
linux.release.x86_64   = "res://../cyg-godot/target/release/libcyg_godot.so"
windows.release.x86_64 = "res://../cyg-godot/target/release/cyg_godot.dll"
macos.release          = "res://../cyg-godot/target/release/libcyg_godot.dylib"
```

## GDScript usage

```gdscript
var room := CygRoom.new()
assert(room.load_descriptor(FileAccess.get_file_as_bytes("res://cyg.schema.bin")))

# every WebSocketPeer packet from the room-data lane:
room.apply_payload(frame)

if room.ready():
    var x := room.get_f64(entity_id, room.field_index("x"))
    var input := room.encode_schema_input(seq, PackedInt64Array([1, 0]))
```

## API

| Method | Returns | Notes |
|---|---|---|
| `load_descriptor(bytes)` | bool | parse `cyg.schema.bin`; false = bad |
| `apply_payload(bytes)` | int | events (>=0), -1 decode error |
| `ready()` | bool | kind-1 full snapshot applied |
| `entity_count()` | int | primary collection rows |
| `entity_id_at(pos)` | int | -1 = none |
| `field_index(name)` | int | -1 = not found |
| `get_f64(id, idx)` | float | NaN = absent |
| `get_string(id, idx)` | String | "" = absent |
| `get_bool(id, idx)` | bool | false = absent |
| `encode_schema_input(seq, values)` | PackedByteArray | raw i64 bit patterns |

## Choosing between CYGGodot (GDScript) and this

- **CYGGodot addon**: zero native build, full feature set (RPCs, rollback,
  Netfox-style lag comp), pure GDScript — the default choice.
- **cyg-godot (this)**: one shared decoder with all other platforms, no
  GDScript decoder drift, potentially faster at high CCU. Choose when
  cross-platform byte-exactness matters most.
