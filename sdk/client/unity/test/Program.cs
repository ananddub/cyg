// Live parity harness: run the captured fixture bytes through the NATIVE
// cyg-core lib (libcyg_core_rs.so) via the Unity C# wrapper — same bytes,
// same expectations as tests/live_parity.rs.
using System;
using System.IO;
using System.Text.Json;

string root = "/mnt/SSD/Devloper/pannel/openoxide-gaming";
string fix = Path.Combine(root, "crates/cyg-core-rs/tests/fixtures");

var descriptor = File.ReadAllBytes(Path.Combine(fix, "descriptor.bin"));
var payloadBlob = File.ReadAllBytes(Path.Combine(fix, "payloads.bin"));
var expected = JsonDocument.Parse(File.ReadAllText(Path.Combine(fix, "expected.json")));

using var room = new Cyg.CygRoom(descriptor);
Console.WriteLine($"abi ok, entities: {room.EntityCount()}");

// length-prefixed frames
int applied = 0;
int at = 0;
while (at + 4 <= payloadBlob.Length)
{
    int len = BitConverter.ToInt32(payloadBlob, at);
    if (at + 4 + len > payloadBlob.Length) break;
    var frame = new byte[len];
    Array.Copy(payloadBlob, at + 4, frame, 0, len);
    int events = room.ApplyPayload(frame);
    if (events >= 0) applied++;
    at += 4 + len;
}
Console.WriteLine($"frames applied: {applied}, ready: {room.StateReady()}, entities: {room.EntityCount()}");
if (!room.StateReady()) { Console.WriteLine("FAIL: never ready"); return 1; }

var players = expected.RootElement.GetProperty("players");
foreach (var want in players.EnumerateArray())
{
    uint id = (uint)want.GetProperty("id").GetUInt32();
    int xi = room.FieldIndex("x");
    int yi = room.FieldIndex("y");
    int ni = room.FieldIndex("name");
    int gi = room.FieldIndex("grounded");
    double x = room.GetF64(id, xi);
    double y = room.GetF64(id, yi);
    string name = room.GetString(id, ni);
    var gb = room.GetBool(id, gi);
    bool ok = Math.Abs(x - want.GetProperty("x").GetDouble()) < 1e-6
        && Math.Abs(y - want.GetProperty("y").GetDouble()) < 1e-6
        && name == want.GetProperty("name").GetString()
        && gb.HasValue && gb.Value == want.GetProperty("grounded").GetBoolean();
    Console.WriteLine($"player {id}: x={x} y={y} name={name} grounded={gb.Value} {(ok ? "OK" : "MISMATCH")}");
    if (!ok) return 1;
}
Console.WriteLine("C# NATIVE PARITY OK");
return 0;
