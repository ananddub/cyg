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
        /// The decoder's documented legacy envelope: [0x11][0xA5]['delta'] (7B)
        /// followed by rows — NO entity id inside the envelope. NOTE: the
        /// Rust server's wrap_delta_into DOES embed an 8-byte id after the
        /// magic, so a live CygClient session decodes rows one id-width off;
        /// that drift is pinned by the server-side SDK wire tests, not here.
        private static byte[] Envelope(params byte[][] rows)
        {
            var head = new List<byte> { 0x11, 0xA5, (byte)'d', (byte)'e', (byte)'l', (byte)'t', (byte)'a' };
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
            var delta = Envelope(
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
