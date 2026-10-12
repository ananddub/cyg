# OpenOxide Gaming SDK for Godot 4

Sub-millisecond, low-latency UDP multiplayer networking client for Godot 4.x (GDScript & C#).

## Features
- Direct UDP socket streaming with zero allocation overhead.
- 20-byte packed input streaming.
- 60Hz delta alignment with interpolation.
- Full support for tactical AI bots and spectators.

## Usage
1. Copy `addons/openoxide` into your Godot project's `addons/` directory.
2. In Project Settings -> Plugins, enable `OpenOxide Gaming SDK`.
3. In GDScript:
```gdscript
var client: OpenOxideClient = OpenOxideClient.new()
add_child(client)

client.connect_to_server("127.0.0.1", 7778, "arena")
client.on_delta.connect(_on_delta_received)

func _physics_process(_delta):
    # Stream input
    client.send_input(16, 0.0, 0.0, 0.0, 1.0)

func _on_delta_received(entities, bullets, local_id):
    print("Received combatants: ", entities.size())
```
