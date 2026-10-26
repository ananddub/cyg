using System;
using System.Collections.Generic;

namespace CookYourGame.Sim.Headless
{
    public sealed class CYGHeadlessMatchOptions
    {
        public CYGMatchMode Mode = CYGMatchMode.Kinematic;
        public int Ticks = 600;
        public int TickRate = CYGDeterminismConfig.DefaultTickRate;
        public ulong Seed = 20261019UL;
        public ulong NetworkSeed = 0xC0FFEEUL;
        public int Redundancy = 12;
        public int MaxPredictionTicks = 20;
        public int MaxDrainTicks = 900;

        public CYGNetSimConfig Network = new CYGNetSimConfig
        {
            LatencyTicks = 3,
            JitterTicks = 2,
            LossPerMille = 50,
            DuplicatePerMille = 20,
            Reorder = true,
        };
    }

    public sealed class CYGHeadlessMatchResult
    {
        public CYGMatchMode Mode;
        public int Ticks;
        public int TickRate;
        public ulong Seed;
        public ulong NetworkSeed;
        public int WallTicks;
        public bool Finalized;
        public int ServerFrame;
        public uint ServerChecksum;
        public uint[] ClientChecksums;
        public int[] ClientFrames;
        public int[] ClientRollbacks;
        public int[] ClientRejectedRollbacks;
        public int[] ClientResimulatedTicks;
        public int[] ClientVerifiedFrames;
        public int[] ClientReportMismatches;
        public int[] ClientFirstMismatchFrames;
        public int[] ClientGeneratedInputs;
        public int[] ClientSentBatches;
        public int[] ClientCatchUpBatches;
        public int[] ClientAppliedRelayBatches;
        public int ServerAcceptedBatches;
        public int ServerAcceptedInputs;
        public int ServerRejectedBatches;
        public int ServerBackfillsSent;
        public int NetworkSent;
        public int NetworkDelivered;
        public int NetworkDropped;
        public int NetworkDuplicated;
        public long ElapsedMilliseconds;
    }

    /// <summary>
    /// Runs one match: an authoritative server, two predicting clients, a
    /// deterministic bot per player, and a simulated lossy network. Input
    /// batches use the redundant encoding from <see cref="CYGInputRedundancy"/>
    /// and are relayed by the server, so a dropped client packet can be
    /// repaired from the next batch or from the server relay.
    /// </summary>
    public static class CYGHeadlessMatch
    {
        public static CYGHeadlessMatchResult Run(CYGHeadlessMatchOptions options)
        {
            if (options == null)
            {
                throw new ArgumentNullException(nameof(options));
            }
            if (options.Ticks < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(options.Ticks));
            }
            if (options.Redundancy < 1 || options.Redundancy > CYGInputRedundancy.MaxInputs)
            {
                throw new ArgumentOutOfRangeException(nameof(options.Redundancy));
            }
            if (options.Network == null)
            {
                throw new ArgumentNullException(nameof(options.Network));
            }
            options.Network.Validate();

            var stopwatch = System.Diagnostics.Stopwatch.StartNew();

            int rollbackCapacity = Math.Max(64, options.Ticks + options.MaxPredictionTicks + 16);
            int inputCapacity = NextPowerOfTwo(Math.Max(CYGInputRingBuffer.DefaultCapacity, options.Ticks + 64));
            var config = new CYGDeterminismConfig(
                tickRate: options.TickRate,
                maxPlayers: 2,
                seed: options.Seed,
                checksumInterval: 30,
                rollbackCapacity: rollbackCapacity,
                maxPredictionTicks: options.MaxPredictionTicks);

            CYGNavMesh navMesh = options.Mode == CYGMatchMode.Navigation
                ? CYGMatchWorldFactory.BuildGridNavMesh()
                : null;

            CYGMatchWorldFactory.Build(options.Mode, config, navMesh, out CYGWorld serverWorld);
            CYGSystemRunner serverSystems = CYGMatchWorldFactory.CreateSystems(
                serverWorld, config, options.Mode, navMesh);
            CYGMatchServer server = new CYGMatchServer(
                serverWorld, serverSystems, config, options.Ticks, inputCapacity);

            CYGMatchClient[] clients =
            {
                CreateClient(1, options, config, navMesh, inputCapacity),
                CreateClient(2, options, config, navMesh, inputCapacity),
            };

            CYGNetworkSimulator network = new CYGNetworkSimulator(options.Network, options.NetworkSeed);
            List<CYGNetPacket> inbox = new List<CYGNetPacket>(32);
            byte[] relayBuffer = new byte[512];
            byte[] statusBuffer = new byte[CYGMatchProtocol.StatusSize];
            byte[] backfillBuffer = new byte[512];
            byte[] reportBuffer = new byte[CYGMatchProtocol.ReportHeaderSize + options.Ticks * 4];

            int wallTicks = 0;
            bool finalized = false;
            int limit = options.Ticks + Math.Max(options.MaxDrainTicks, 1);

            for (int iteration = 0; iteration < limit; iteration++)
            {
                network.Advance();
                wallTicks++;

                inbox.Clear();
                network.Receive(0, inbox);
                for (int i = 0; i < inbox.Count; i++)
                {
                    CYGNetPacket packet = inbox[i];
                    if (packet.Length < 1 || packet.Payload[0] != (byte)CYGMatchMessage.Input)
                    {
                        continue;
                    }

                    int requestedPeerTick;
                    int playerId = server.AcceptInputBatch(packet.Payload, packet.Length, out requestedPeerTick);
                    if (playerId < 0)
                    {
                        continue;
                    }

                    int relayLength = CYGMatchProtocol.WriteRelay(
                        relayBuffer,
                        packet.Payload,
                        CYGMatchProtocol.InputHeaderSize,
                        packet.Length - CYGMatchProtocol.InputHeaderSize);
                    network.Send(0, 3 - playerId, relayBuffer, 0, relayLength);

                    if (requestedPeerTick >= 0)
                    {
                        int backfillLength;
                        if (server.TryWriteBackfill(backfillBuffer, playerId, requestedPeerTick, out backfillLength))
                        {
                            network.Send(0, playerId, backfillBuffer, 0, backfillLength);
                        }
                    }
                }

                for (int c = 0; c < clients.Length; c++)
                {
                    inbox.Clear();
                    network.Receive(clients[c].PlayerId, inbox);
                    for (int i = 0; i < inbox.Count; i++)
                    {
                        clients[c].Receive(inbox[i]);
                    }
                }

                for (int c = 0; c < clients.Length; c++)
                {
                    clients[c].Advance(network);
                }

                server.StepAvailable();

                int missingPlayerId;
                int missingTick;
                if (!server.TryGetMissingInput(out missingPlayerId, out missingTick))
                {
                    missingPlayerId = 0;
                    missingTick = -1;
                }
                int statusLength = CYGMatchProtocol.WriteStatus(
                    statusBuffer, server.Frame, missingPlayerId, missingTick);
                network.Send(0, 1, statusBuffer, 0, statusLength);
                network.Send(0, 2, statusBuffer, 0, statusLength);

                if (server.Frame >= options.Ticks)
                {
                    int reportLength = server.WriteFinalReport(reportBuffer);
                    network.Send(0, 1, reportBuffer, 0, reportLength);
                    network.Send(0, 2, reportBuffer, 0, reportLength);
                }

                if (server.Frame >= options.Ticks
                    && clients[0].ReportVerified
                    && clients[1].ReportVerified)
                {
                    finalized = true;
                    break;
                }
            }

            stopwatch.Stop();

            CYGHeadlessMatchResult result = BuildResult(
                options, server, clients, network, wallTicks, finalized, stopwatch.ElapsedMilliseconds);
            Validate(options, result, clients);
            return result;
        }

        private static CYGMatchClient CreateClient(
            int playerId,
            CYGHeadlessMatchOptions options,
            CYGDeterminismConfig config,
            CYGNavMesh navMesh,
            int inputCapacity)
        {
            CYGMatchWorldFactory.Build(options.Mode, config, navMesh, out CYGWorld world);
            CYGSystemRunner systems = CYGMatchWorldFactory.CreateSystems(
                world, config, options.Mode, navMesh);

            CYGWorldRollbackSession session = new CYGWorldRollbackSession(
                world,
                config,
                inputBufferSize: inputCapacity,
                maxPlayers: 2,
                defaultCharacterMovement: false);
            session.LocalPlayerId = playerId;
            session.SetSystems(systems);

            return new CYGMatchClient(
                playerId,
                session,
                options.Ticks,
                options.Redundancy,
                catchUpThreshold: Math.Max(4, options.Redundancy / 2));
        }

        private static CYGHeadlessMatchResult BuildResult(
            CYGHeadlessMatchOptions options,
            CYGMatchServer server,
            CYGMatchClient[] clients,
            CYGNetworkSimulator network,
            int wallTicks,
            bool finalized,
            long elapsedMilliseconds)
        {
            CYGHeadlessMatchResult result = new CYGHeadlessMatchResult
            {
                Mode = options.Mode,
                Ticks = options.Ticks,
                TickRate = options.TickRate,
                Seed = options.Seed,
                NetworkSeed = options.NetworkSeed,
                WallTicks = wallTicks,
                Finalized = finalized,
                ServerFrame = server.Frame,
                ServerChecksum = server.FinalChecksum,
                ClientChecksums = new uint[clients.Length],
                ClientFrames = new int[clients.Length],
                ClientRollbacks = new int[clients.Length],
                ClientRejectedRollbacks = new int[clients.Length],
                ClientResimulatedTicks = new int[clients.Length],
                ClientVerifiedFrames = new int[clients.Length],
                ClientReportMismatches = new int[clients.Length],
                ClientFirstMismatchFrames = new int[clients.Length],
                ClientGeneratedInputs = new int[clients.Length],
                ClientSentBatches = new int[clients.Length],
                ClientCatchUpBatches = new int[clients.Length],
                ClientAppliedRelayBatches = new int[clients.Length],
                ServerAcceptedBatches = server.AcceptedInputBatches,
                ServerAcceptedInputs = server.AcceptedInputs,
                ServerRejectedBatches = server.RejectedInputBatches,
                ServerBackfillsSent = server.BackfillsSent,
                NetworkSent = network.SentCount,
                NetworkDelivered = network.DeliveredCount,
                NetworkDropped = network.DroppedCount,
                NetworkDuplicated = network.DuplicatedCount,
                ElapsedMilliseconds = elapsedMilliseconds,
            };

            for (int i = 0; i < clients.Length; i++)
            {
                CYGMatchClient client = clients[i];
                result.ClientChecksums[i] = client.Session.Checksum;
                result.ClientFrames[i] = client.Session.Frame;
                result.ClientRollbacks[i] = client.Session.TotalRollbacks;
                result.ClientRejectedRollbacks[i] = client.Session.RejectedRollbacks;
                result.ClientResimulatedTicks[i] = client.Session.TotalResimulatedTicks;
                result.ClientReportMismatches[i] = client.ReportMismatches;
                result.ClientFirstMismatchFrames[i] = client.FirstMismatchFrame;
                result.ClientGeneratedInputs[i] = client.GeneratedInputs;
                result.ClientSentBatches[i] = client.SentBatches;
                result.ClientCatchUpBatches[i] = client.CatchUpBatches;
                result.ClientAppliedRelayBatches[i] = client.AppliedRelayBatches;

                int verified = 0;
                for (int frame = 1; frame <= options.Ticks; frame++)
                {
                    if (client.Session.VerifiedFrames.IsVerified(frame))
                    {
                        verified++;
                    }
                }
                result.ClientVerifiedFrames[i] = verified;
            }

            return result;
        }

        private static void Validate(
            CYGHeadlessMatchOptions options,
            CYGHeadlessMatchResult result,
            CYGMatchClient[] clients)
        {
            if (!result.Finalized)
            {
                throw new InvalidOperationException(
                    "match did not settle within " + result.WallTicks + " wall ticks "
                    + "(server frame " + result.ServerFrame + "/" + options.Ticks
                    + ", clients " + result.ClientFrames[0] + "/" + result.ClientFrames[1] + ").");
            }

            if (result.ServerFrame != options.Ticks)
            {
                throw new InvalidOperationException(
                    "server completed " + result.ServerFrame + " of " + options.Ticks + " ticks.");
            }

            uint authoritative = result.ServerChecksum;
            for (int i = 0; i < clients.Length; i++)
            {
                CYGMatchClient client = clients[i];
                int playerId = client.PlayerId;

                if (client.Session.Frame != options.Ticks)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " stopped at frame " + client.Session.Frame + "/" + options.Ticks + ".");
                }
                if (!client.ReportVerified)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " never verified the server checksum report.");
                }
                if (client.ReportMismatches != 0)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " desynced at frame " + client.FirstMismatchFrame
                        + " (" + client.ReportMismatches + " mismatched frames).");
                }
                if (client.Session.RejectedRollbacks != 0)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " could not roll back " + client.Session.RejectedRollbacks
                        + " late input(s); the prediction window is too far ahead of confirmation.");
                }
                if (client.Session.VerifiedFrames.HasDesync)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " reported a desync: " + client.Session.VerifiedFrames.FirstDesync);
                }
                if (result.ClientVerifiedFrames[i] != options.Ticks)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " verified " + result.ClientVerifiedFrames[i]
                        + "/" + options.Ticks + " frames.");
                }
                if (result.ClientChecksums[i] != authoritative)
                {
                    throw new InvalidOperationException(
                        "client " + playerId + " final checksum 0x" + result.ClientChecksums[i].ToString("X8")
                        + " differs from server 0x" + authoritative.ToString("X8") + ".");
                }
            }

            if (result.ClientChecksums[0] != result.ClientChecksums[1])
            {
                throw new InvalidOperationException("clients disagree on the final checksum.");
            }
        }

        private static int NextPowerOfTwo(int value)
        {
            int result = 1;
            while (result < value)
            {
                result <<= 1;
            }
            return result;
        }
    }
}
