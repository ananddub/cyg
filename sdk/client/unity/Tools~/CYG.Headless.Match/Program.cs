using System;
using System.Collections.Generic;
using System.Globalization;

namespace CookYourGame.Sim.Headless
{
    internal static class Program
    {
        private static int Main(string[] args)
        {
            try
            {
                ParsedArgs parsed = ParseArgs(args);
                if (parsed.Help)
                {
                    PrintUsage();
                    return 0;
                }

                CYGMatchMode[] modes = parsed.Modes;
                int passed = 0;
                for (int i = 0; i < modes.Length; i++)
                {
                    CYGHeadlessMatchOptions options = parsed.CreateOptions();
                    options.Mode = modes[i];
                    CYGHeadlessMatchResult result = CYGHeadlessMatch.Run(options);
                    ValidateFaultInjection(options, result);
                    PrintResult(result, parsed.Quiet);
                    passed++;
                }

                Console.WriteLine("CYG headless match: " + passed + "/" + modes.Length + " PASS");
                return 0;
            }
            catch (Exception ex)
            {
                Console.Error.WriteLine("CYG headless match FAILED: " + ex.GetType().Name + ": " + ex.Message);
                if (ex.StackTrace != null)
                {
                    Console.Error.WriteLine(ex.StackTrace);
                }
                return 1;
            }
        }

        private static void ValidateFaultInjection(
            CYGHeadlessMatchOptions options,
            CYGHeadlessMatchResult result)
        {
            if (options.Network.LossPerMille > 0 && result.NetworkDropped == 0)
            {
                throw new InvalidOperationException(
                    "loss was configured but no packet was dropped; the link model did not run.");
            }
            if (options.Network.DuplicatePerMille > 0 && result.NetworkDuplicated == 0)
            {
                throw new InvalidOperationException(
                    "duplication was configured but no packet was duplicated.");
            }

            bool predicts = options.Network.LatencyTicks > 0
                || options.Network.JitterTicks > 0
                || options.Network.LossPerMille > 0;
            if (predicts && result.ClientRollbacks[0] + result.ClientRollbacks[1] == 0)
            {
                throw new InvalidOperationException(
                    "the link cannot deliver remote input before prediction, yet no rollback happened.");
            }
        }

        private static void PrintResult(CYGHeadlessMatchResult result, bool quiet)
        {
            if (quiet)
            {
                Console.WriteLine(
                    "[CYG-MATCH] " + result.Mode + " " + result.Ticks + "t PASS"
                    + " wall=" + result.WallTicks
                    + " rollbacks=" + (result.ClientRollbacks[0] + result.ClientRollbacks[1])
                    + " dropped=" + result.NetworkDropped
                    + " dup=" + result.NetworkDuplicated
                    + " ms=" + result.ElapsedMilliseconds);
                return;
            }

            Console.WriteLine(
                "[CYG-MATCH] mode=" + result.Mode
                + " ticks=" + result.Ticks
                + " seed=" + result.Seed
                + " networkSeed=" + result.NetworkSeed);
            Console.WriteLine(
                "  server    frame=" + result.ServerFrame
                + " checksum=0x" + result.ServerChecksum.ToString("X8")
                + " accepted=" + result.ServerAcceptedBatches + " batches/" + result.ServerAcceptedInputs + " inputs"
                + " rejected=" + result.ServerRejectedBatches
                + " backfills=" + result.ServerBackfillsSent);

            for (int i = 0; i < result.ClientFrames.Length; i++)
            {
                Console.WriteLine(
                    "  client " + (i + 1)
                    + "  frame=" + result.ClientFrames[i]
                    + " checksum=0x" + result.ClientChecksums[i].ToString("X8")
                    + " rollbacks=" + result.ClientRollbacks[i]
                    + " rejected=" + result.ClientRejectedRollbacks[i]
                    + " resim=" + result.ClientResimulatedTicks[i]
                    + " verified=" + result.ClientVerifiedFrames[i] + "/" + result.Ticks
                    + " inputs=" + result.ClientGeneratedInputs[i]
                    + " batches=" + result.ClientSentBatches[i]
                    + " relays=" + result.ClientAppliedRelayBatches[i]
                    + " catchUp=" + result.ClientCatchUpBatches[i]);
            }

            Console.WriteLine(
                "  network   sent=" + result.NetworkSent
                + " delivered=" + result.NetworkDelivered
                + " dropped=" + result.NetworkDropped
                + " duplicated=" + result.NetworkDuplicated);
            Console.WriteLine(
                "  result    PASS wall=" + result.WallTicks
                + " elapsed=" + result.ElapsedMilliseconds + "ms");
        }

        private sealed class ParsedArgs
        {
            public bool Help;
            public bool Quiet;
            public CYGMatchMode[] Modes = { CYGMatchMode.Kinematic, CYGMatchMode.Physics3D, CYGMatchMode.Physics2D, CYGMatchMode.Navigation };
            public int Ticks = 600;
            public int TickRate = CYGDeterminismConfig.DefaultTickRate;
            public ulong Seed = 20261019UL;
            public ulong NetworkSeed = 0xC0FFEEUL;
            public int Redundancy = 12;
            public int MaxPredictionTicks = 20;
            public int MaxDrainTicks = 900;
            public int Latency = 3;
            public int Jitter = 2;
            public int Loss = 50;
            public int Duplicate = 20;

            public CYGHeadlessMatchOptions CreateOptions()
            {
                return new CYGHeadlessMatchOptions
                {
                    Ticks = Ticks,
                    TickRate = TickRate,
                    Seed = Seed,
                    NetworkSeed = NetworkSeed,
                    Redundancy = Redundancy,
                    MaxPredictionTicks = MaxPredictionTicks,
                    MaxDrainTicks = MaxDrainTicks,
                    Network = new CYGNetSimConfig
                    {
                        LatencyTicks = Latency,
                        JitterTicks = Jitter,
                        LossPerMille = Loss,
                        DuplicatePerMille = Duplicate,
                        Reorder = true,
                    },
                };
            }
        }

        private static ParsedArgs ParseArgs(string[] args)
        {
            ParsedArgs parsed = new ParsedArgs();
            for (int i = 0; i < args.Length; i++)
            {
                string arg = args[i];
                switch (arg)
                {
                    case "--help":
                    case "-h":
                        parsed.Help = true;
                        return parsed;
                    case "--quiet":
                        parsed.Quiet = true;
                        break;
                    case "--mode":
                        parsed.Modes = ParseModes(RequireValue(args, ref i, arg));
                        break;
                    case "--ticks":
                        parsed.Ticks = ParseInt(RequireValue(args, ref i, arg), arg, 1);
                        break;
                    case "--tick-rate":
                        parsed.TickRate = ParseInt(RequireValue(args, ref i, arg), arg, 1);
                        break;
                    case "--seed":
                        parsed.Seed = ParseULong(RequireValue(args, ref i, arg), arg);
                        break;
                    case "--network-seed":
                        parsed.NetworkSeed = ParseULong(RequireValue(args, ref i, arg), arg);
                        break;
                    case "--latency":
                        parsed.Latency = ParseInt(RequireValue(args, ref i, arg), arg, 0);
                        break;
                    case "--jitter":
                        parsed.Jitter = ParseInt(RequireValue(args, ref i, arg), arg, 0);
                        break;
                    case "--loss":
                        parsed.Loss = ParseInt(RequireValue(args, ref i, arg), arg, 0);
                        break;
                    case "--duplicate":
                        parsed.Duplicate = ParseInt(RequireValue(args, ref i, arg), arg, 0);
                        break;
                    case "--redundancy":
                        parsed.Redundancy = ParseInt(RequireValue(args, ref i, arg), arg, 1);
                        break;
                    case "--prediction":
                        parsed.MaxPredictionTicks = ParseInt(RequireValue(args, ref i, arg), arg, 1);
                        break;
                    case "--drain":
                        parsed.MaxDrainTicks = ParseInt(RequireValue(args, ref i, arg), arg, 1);
                        break;
                    default:
                        throw new ArgumentException("unknown argument '" + arg + "' (use --help)");
                }
            }

            if (parsed.Loss > 1000 || parsed.Duplicate > 1000)
            {
                throw new ArgumentException("--loss and --duplicate are per mille and must be in [0, 1000].");
            }
            if (parsed.Redundancy > CYGInputRedundancy.MaxInputs)
            {
                throw new ArgumentException("--redundancy must be at most " + CYGInputRedundancy.MaxInputs + ".");
            }
            return parsed;
        }

        private static CYGMatchMode[] ParseModes(string value)
        {
            if (string.Equals(value, "all", StringComparison.OrdinalIgnoreCase))
            {
                return new[]
                {
                    CYGMatchMode.Kinematic,
                    CYGMatchMode.Physics3D,
                    CYGMatchMode.Physics2D,
                    CYGMatchMode.Navigation,
                };
            }

            CYGMatchMode mode;
            if (!TryParseMode(value, out mode))
            {
                throw new ArgumentException("unknown mode '" + value + "'.");
            }
            return new[] { mode };
        }

        private static bool TryParseMode(string value, out CYGMatchMode mode)
        {
            string[] names = Enum.GetNames(typeof(CYGMatchMode));
            for (int i = 0; i < names.Length; i++)
            {
                if (string.Equals(names[i], value, StringComparison.OrdinalIgnoreCase))
                {
                    mode = (CYGMatchMode)Enum.Parse(typeof(CYGMatchMode), names[i]);
                    return true;
                }
            }
            mode = default(CYGMatchMode);
            return false;
        }

        private static string RequireValue(string[] args, ref int index, string name)
        {
            if (index + 1 >= args.Length)
            {
                throw new ArgumentException("missing value for " + name);
            }
            index++;
            return args[index];
        }

        private static int ParseInt(string value, string name, int minimum)
        {
            int result;
            if (!int.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out result)
                || result < minimum)
            {
                throw new ArgumentException(name + " expects an integer >= " + minimum + ".");
            }
            return result;
        }

        private static ulong ParseULong(string value, string name)
        {
            ulong result;
            if (!ulong.TryParse(value, NumberStyles.Integer, CultureInfo.InvariantCulture, out result))
            {
                throw new ArgumentException(name + " expects an unsigned integer.");
            }
            return result;
        }

        private static void PrintUsage()
        {
            Console.WriteLine("CYG headless two-client match");
            Console.WriteLine();
            Console.WriteLine("Usage: CYG.Headless.Match [options]");
            Console.WriteLine();
            Console.WriteLine("  --mode <name>        all | kinematic | physics3d | physics2d | navigation (default all)");
            Console.WriteLine("  --ticks <n>          simulation ticks per match (default 600)");
            Console.WriteLine("  --tick-rate <n>      fixed tick rate in Hz (default 60)");
            Console.WriteLine("  --seed <n>           simulation seed (default 20261019)");
            Console.WriteLine("  --network-seed <n>   transport seed (default 12648430)");
            Console.WriteLine("  --latency <ticks>    one-way latency (default 3)");
            Console.WriteLine("  --jitter <ticks>     extra delivery delay (default 2)");
            Console.WriteLine("  --loss <per-mille>   packet loss (default 50)");
            Console.WriteLine("  --duplicate <per-mille> packet duplication (default 20)");
            Console.WriteLine("  --redundancy <n>     inputs per packet, 1-32 (default 12)");
            Console.WriteLine("  --prediction <n>     client prediction budget (default 20)");
            Console.WriteLine("  --drain <ticks>      extra transport ticks after the match (default 900)");
            Console.WriteLine("  --quiet              one line per mode");
            Console.WriteLine("  --help               show this help");
        }
    }
}
