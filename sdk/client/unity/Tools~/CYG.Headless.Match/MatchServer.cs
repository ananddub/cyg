using System;

namespace CookYourGame.Sim.Headless
{
    /// <summary>
    /// Authoritative match server. It never predicts: a tick is stepped only
    /// when confirmed inputs for every player are present in the ring.
    /// </summary>
    internal sealed class CYGMatchServer
    {
        private readonly CYGWorld _world;
        private readonly CYGSystemRunner _systems;
        private readonly CYGWorldRunner _runner;
        private readonly CYGInputRingBuffer _inputs;
        private readonly CYGPlayerInput[] _decoded = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
        private readonly CYGPlayerInput[] _tickInputs = new CYGPlayerInput[2];
        private readonly int[] _tickInputPlayerIds = { 1, 2 };
        private readonly CYGPlayerInput[] _historyFirst;
        private readonly CYGPlayerInput[] _historySecond;
        private readonly bool[] _hasFirst;
        private readonly bool[] _hasSecond;
        private readonly CYGPlayerInput[] _backfillWindow = new CYGPlayerInput[CYGInputRedundancy.MaxInputs];
        private readonly byte[] _backfillBody = new byte[CYGInputRedundancy.HeaderSize
            + CYGInputRedundancy.MaxInputs * CYGPlayerInput.SizeInBytes];
        private readonly uint[] _checksums;
        private readonly int _totalTicks;

        public CYGMatchServer(
            CYGWorld world,
            CYGSystemRunner systems,
            CYGDeterminismConfig config,
            int totalTicks,
            int inputBufferSize)
        {
            if (totalTicks < 1)
            {
                throw new ArgumentOutOfRangeException(nameof(totalTicks));
            }

            _world = world;
            _systems = systems;
            _totalTicks = totalTicks;
            _checksums = new uint[totalTicks];
            _inputs = new CYGInputRingBuffer(inputBufferSize, config.MaxPlayers);

            // The server keeps the complete accepted input history so it can
            // backfill a client that lost an early redundant window.
            _historyFirst = new CYGPlayerInput[totalTicks];
            _historySecond = new CYGPlayerInput[totalTicks];
            _hasFirst = new bool[totalTicks];
            _hasSecond = new bool[totalTicks];

            _runner = new CYGWorldRunner(world, config);
            _systems.Start();
        }

        public int Frame
        {
            get { return _runner.Frame; }
        }

        public uint[] Checksums
        {
            get { return _checksums; }
        }

        public int AcceptedInputBatches { get; private set; }

        public int AcceptedInputs { get; private set; }

        public int RejectedInputBatches { get; private set; }

        public int BackfillsSent { get; private set; }

        /// <summary>
        /// Decodes an input message (type byte included) and stores all
        /// confirmed inputs. Returns the player id, or -1 for a rejected batch.
        /// The client's requested peer tick is returned for gap repair.
        /// </summary>
        public int AcceptInputBatch(byte[] payload, int length, out int requestedPeerTick)
        {
            requestedPeerTick = -1;
            if (payload == null || length < CYGMatchProtocol.InputHeaderSize + CYGInputRedundancy.HeaderSize)
            {
                RejectedInputBatches++;
                return -1;
            }

            int request = CYGWire.ReadInt32(payload, 1);
            if (request >= 0 && request < _totalTicks)
            {
                requestedPeerTick = request;
            }

            int playerId;
            int count;
            if (!CYGInputRedundancy.TryReadPacket(
                    payload,
                    CYGMatchProtocol.InputHeaderSize,
                    length - CYGMatchProtocol.InputHeaderSize,
                    _decoded,
                    out playerId,
                    out count))
            {
                RejectedInputBatches++;
                return -1;
            }
            if (playerId != 1 && playerId != 2)
            {
                RejectedInputBatches++;
                return -1;
            }

            for (int i = 0; i < count; i++)
            {
                int tick = _decoded[i].Tick;
                if (tick < 0 || tick >= _totalTicks)
                {
                    RejectedInputBatches++;
                    return -1;
                }
            }

            CYGPlayerInput[] history = playerId == 1 ? _historyFirst : _historySecond;
            bool[] hasHistory = playerId == 1 ? _hasFirst : _hasSecond;
            for (int i = 0; i < count; i++)
            {
                int tick = _decoded[i].Tick;
                _inputs.SetInput(playerId, tick, _decoded[i], isPredicted: false);
                history[tick] = _decoded[i];
                hasHistory[tick] = true;
            }

            AcceptedInputBatches++;
            AcceptedInputs += count;
            return playerId;
        }

        /// <summary>Reports which input the next authoritative tick is missing, if any.</summary>
        public bool TryGetMissingInput(out int playerId, out int tick)
        {
            if (_runner.Frame < _totalTicks)
            {
                int next = _runner.Frame;
                if (!_hasFirst[next])
                {
                    playerId = 1;
                    tick = next;
                    return true;
                }
                if (!_hasSecond[next])
                {
                    playerId = 2;
                    tick = next;
                    return true;
                }
            }

            playerId = 0;
            tick = -1;
            return false;
        }

        /// <summary>
        /// Writes a relay batch starting at the requester's missing peer tick.
        /// Returns false when the server has no matching input yet.
        /// </summary>
        public bool TryWriteBackfill(
            byte[] buffer,
            int requesterPlayerId,
            int requestedTick,
            out int length)
        {
            length = 0;
            if (requesterPlayerId != 1 && requesterPlayerId != 2)
            {
                return false;
            }
            if (requestedTick < 0 || requestedTick >= _totalTicks)
            {
                return false;
            }

            int peer = 3 - requesterPlayerId;
            CYGPlayerInput[] history = peer == 1 ? _historyFirst : _historySecond;
            bool[] hasHistory = peer == 1 ? _hasFirst : _hasSecond;
            if (!hasHistory[requestedTick])
            {
                return false;
            }

            int count = 0;
            for (int i = 0; i < CYGInputRedundancy.MaxInputs; i++)
            {
                int tick = requestedTick + i;
                if (tick >= _totalTicks || !hasHistory[tick])
                {
                    break;
                }
                _backfillWindow[count++] = history[tick];
            }

            int bodyLength = CYGInputRedundancy.WritePacket(
                _backfillBody, 0, peer, _backfillWindow, count);
            length = CYGMatchProtocol.WriteRelay(buffer, _backfillBody, 0, bodyLength);
            BackfillsSent++;
            return true;
        }

        /// <summary>Steps every tick whose inputs are all confirmed. Returns the number stepped.</summary>
        public int StepAvailable()
        {
            int stepped = 0;
            while (_runner.Frame < _totalTicks)
            {
                int tick = _runner.CurrentTick;
                CYGPlayerInput first;
                CYGPlayerInput second;
                bool firstPredicted;
                bool secondPredicted;
                if (!_inputs.TryGetInput(1, tick, out first, out firstPredicted) || firstPredicted)
                {
                    break;
                }
                if (!_inputs.TryGetInput(2, tick, out second, out secondPredicted) || secondPredicted)
                {
                    break;
                }

                _tickInputs[0] = first;
                _tickInputs[1] = second;
                _runner.Tick(_systems, false, _tickInputs, _tickInputPlayerIds, 2);
                _checksums[tick] = _world.ComputeChecksum();
                stepped++;
            }

            return stepped;
        }

        public int WriteFinalReport(byte[] buffer)
        {
            if (_runner.Frame < _totalTicks)
            {
                throw new InvalidOperationException(
                    "Final report requires all ticks; server frame is " + _runner.Frame + "/" + _totalTicks + ".");
            }
            return CYGMatchProtocol.WriteReport(buffer, _checksums, _totalTicks);
        }

        public uint FinalChecksum
        {
            get
            {
                if (_runner.Frame < 1)
                {
                    return 0u;
                }
                return _checksums[Math.Min(_runner.Frame, _totalTicks) - 1];
            }
        }
    }
}
