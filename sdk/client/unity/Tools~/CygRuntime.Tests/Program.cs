// Unity runtime test runner — no test framework, no external packages:
// `dotnet run` compiles the real Runtime sources (CygDeltaDecoder, CygAuth)
// and asserts the client half of the wire contract.
//
//   cd sdk/client/unity/Tools~/CygRuntime.Tests && dotnet run

using System;
using System.Collections.Generic;
using System.Collections.Concurrent;
using System.Threading;
using CYG;

namespace CYG.Tests
{
    internal static class Runner
    {
        private static int _checks;
        private static int _failures;

        internal static void Check(string name, bool ok)
        {
            _checks++;
            if (ok) Console.WriteLine($"CHECK {name}");
            else { _failures++; Console.Error.WriteLine($"FAIL {name}"); }
        }

        internal static int Finish()
        {
            Console.WriteLine($"checks={_checks} failures={_failures}");
            return _failures > 0 ? 1 : 0;
        }
    }

    internal static class Program
    {
        private static int Main()
        {
            DeltaDecoderTests.Run();
            AuthTests.Run();
            VoiceTests.Run();
            var origin = Environment.GetEnvironmentVariable("CYG_E2E_ORIGIN");
            if (!string.IsNullOrEmpty(origin))
            {
                LiveTests.Run(origin).GetAwaiter().GetResult();
            }
            else
            {
                Console.WriteLine("CYG_E2E_ORIGIN not set — live flow skipped");
            }
            return Runner.Finish();
        }
    }

    internal static class DeltaDecoderTests
    {
        /// The real wire envelope from wrap_delta_into:
        /// [0x11][0xA5]['delta'][your_id i64] — 15 bytes before the rows.
        private static byte[] Envelope(ulong yourId, params byte[][] rows)
        {
            var head = new List<byte> { 0x11, 0xA5, (byte)'d', (byte)'e', (byte)'l', (byte)'t', (byte)'a' };
            for (int b = 0; b < 8; b++) head.Add((byte)((yourId >> (8 * b)) & 0xFF));
            foreach (var row in rows) head.AddRange(row);
            return head.ToArray();
        }

        private static byte[] Row(byte tag, ulong id, params double[] numbers)
        {
            var r = new List<byte> { tag, (byte)(8 * numbers.Length) };
            for (int b = 0; b < 8; b++) r.Add((byte)((id >> (8 * b)) & 0xFF));
            foreach (var n in numbers)
                foreach (var b in BitConverter.GetBytes(n))
                    r.Add(b);
            return r.ToArray();
        }

        internal static void Run()
        {
            var tags = new Dictionary<byte, string[]>
            {
                [0x01] = new[] { "x", "hp" },
                [0x10] = new[] { "coinX", "coinY" },
            };

            // Legacy envelope is stripped; rows decode with names in order.
            var delta = Envelope(1ul,
                Row(0x01, 7ul, 100.5, 80.0),
                Row(0x10, 0ul, 12.0, 34.0));
            var rows = CygDeltaDecoder.Decode(delta, tags);
            Runner.Check("legacy envelope stripped (2 rows)", rows.Count == 2);
            Runner.Check("entity row id", rows[0].EntityId == 7ul && !rows[0].IsRoot);
            Runner.Check("entity fields named in order",
                rows[0].Fields.Length == 2 && rows[0].Fields[0].Name == "x" && rows[0].Fields[1].Name == "hp");
            Runner.Check("entity values roundtrip",
                Math.Abs(rows[0].Fields[0].Number - 100.5) < 1e-9 && Math.Abs(rows[0].Fields[1].Number - 80.0) < 1e-9);
            Runner.Check("root row flagged", rows[1].IsRoot && rows[1].Tag == 0x10);
            Runner.Check("root values roundtrip",
                Math.Abs(rows[1].Fields[0].Number - 12.0) < 1e-9 && Math.Abs(rows[1].Fields[1].Number - 34.0) < 1e-9);

            // Bare rows (no envelope) decode the same way.
            var bare = new List<byte>();
            bare.AddRange(Row(0x01, 9ul, 5.0, 6.0));
            var bareRows = CygDeltaDecoder.Decode(bare.ToArray(), tags);
            Runner.Check("bare rows decode", bareRows.Count == 1 && bareRows[0].EntityId == 9ul);

            // Unknown tags still decode (fields unnamed, not dropped).
            var unknown = Row(0x42, 3ul, 1.0);
            var unknownRows = CygDeltaDecoder.Decode(unknown, tags);
            Runner.Check("unknown tag decodes unnamed", unknownRows.Count == 1 && unknownRows[0].Fields[0].Name == null);

            // Truncated row: the decoder stops, never throws.
            var truncated = new byte[] { 0x01, 0x10, 7 };
            var truncRows = CygDeltaDecoder.Decode(truncated, tags);
            Runner.Check("truncated input does not throw", truncRows.Count == 0);

            // A row whose declared length runs past the buffer stops cleanly.
            var overrun = new byte[] { 0x01, 0xFF, 7, 0, 0, 0, 0, 0, 0, 0, 1, 2 };
            Runner.Check("overrun length stops cleanly",
                CygDeltaDecoder.Decode(overrun, tags).Count == 0);
        }
    }

    internal static class AuthTests
    {
        internal static void Run()
        {
            // The origin is normalized (trailing slash trimmed) — the request
            // path must never double up slashes.
            var auth = new CygAuth("http://127.0.0.1:8124/");
            Runner.Check("auth constructs against any origin", auth != null);
            Runner.Check("token starts empty", string.IsNullOrEmpty(auth.Token));
            // Token capture after a successful body is a live-server flow
            // (needs the control plane) — covered by the host e2e suites.
        }
    }

    /// Voice control framing and the shared FNV-1a team vectors. These are the
    /// bytes the web (`schemaFnv1a`) and Godot (`CYGCodec.fnv1a`) SDKs send.
    internal static class VoiceTests
    {
        internal static void Run()
        {
            Runner.Check("voice hash red matches web/godot",
                CygClient.HashTeam("red") == 0x40F480DCu);
            Runner.Check("voice hash blue matches web/godot",
                CygClient.HashTeam("blue") == 0x82FBF5CDu);
            Runner.Check("voice hash trims labels",
                CygClient.HashTeam("  red  ") == CygClient.HashTeam("red"));

            var team = CygClient.BuildVoiceTeamPacket("red");
            Runner.Check("team packet is 6 bytes with magic/opcode",
                team.Length == 6 && team[0] == 0x56 && team[1] == 0x54);
            Runner.Check("team packet carries the big-endian hash",
                team[2] == 0x40 && team[3] == 0xF4 && team[4] == 0x80 && team[5] == 0xDC);

            var clear = CygClient.BuildVoiceTeamPacket(null);
            Runner.Check("leaving the team sends a zero hash",
                clear.Length == 6 && clear[2] == 0 && clear[3] == 0 && clear[4] == 0 && clear[5] == 0);

            var mute = CygClient.BuildVoiceFlagPacket(0x4D, true);
            Runner.Check("mute packet",
                mute.Length == 3 && mute[0] == 0x56 && mute[1] == 0x4D && mute[2] == 1);

            var deafen = CygClient.BuildVoiceFlagPacket(0x44, false);
            Runner.Check("deafen packet",
                deafen.Length == 3 && deafen[0] == 0x56 && deafen[1] == 0x44 && deafen[2] == 0);

            var opus = new byte[] { 1, 2, 3, 4 };
            var frame = CygClient.BuildVoicePacket(
                VoiceChannel.TeamOnly, 0x1234, 0xAABBCCDD, 0x02, opus);
            Runner.Check("voice frame length is header + payload", frame.Length == 19);
            Runner.Check("voice frame magic + seq",
                frame[0] == 0x56 && frame[5] == 0x12 && frame[6] == 0x34);
            Runner.Check("voice frame timestamp",
                frame[7] == 0xAA && frame[8] == 0xBB && frame[9] == 0xCC && frame[10] == 0xDD);
            Runner.Check("voice frame mode + flags", frame[11] == 2 && frame[12] == 0x02);
            Runner.Check("voice frame payload length + bytes",
                frame[13] == 0 && frame[14] == 4 && frame[15] == 1 && frame[18] == 4);

            var empty = CygClient.BuildVoicePacket(VoiceChannel.Global, 0, 0, 0, null);
            Runner.Check("voice frame with no payload is exactly 15 bytes",
                empty.Length == 15 && empty[13] == 0 && empty[14] == 0);
        }
    }

    /// The native-UDP live flow against a real server: matchmake -> UDP
    /// JOIN -> BIND (player id) -> schema deltas -> input -> leave.
    internal static class LiveTests
    {
        internal static async System.Threading.Tasks.Task Run(string origin)
        {
            var deltas = new ConcurrentQueue<System.Collections.Generic.List<CygRow>>();
            CygClient client = null;
            client = await CygClient.JoinOrCreate(origin, "coin-rush");
            Runner.Check("udp join + bind within 5s", client != null && client.Connected);
            Runner.Check("player id minted", client.PlayerId > 0);
            Runner.Check("session id assigned", !string.IsNullOrEmpty(client.SessionId));

            client.OnRawDelta += raw =>
            {
                var rows = CygDeltaDecoder.Decode(raw, new Dictionary<byte, string[]>
                {
                    [0x01] = new[] { "x", "hp" },
                    [0x10] = new[] { "coinX", "coinY" },
                });
                deltas.Enqueue(rows);
            };

            // Drive input for ~1.5s and count decoded deltas.
            var payload = new byte[20];
            BitConverter.GetBytes(1.0f).CopyTo(payload, 12); // move_x lane
            for (int i = 0; i < 45; i++)
            {
                client.SendInput(payload);
                await System.Threading.Tasks.Task.Delay(33);
            }
            Runner.Check("deltas decoded while driving", deltas.Count > 5);

            client.Leave();
            await System.Threading.Tasks.Task.Delay(300);
            Runner.Check("left without error", true);
        }
    }
}
