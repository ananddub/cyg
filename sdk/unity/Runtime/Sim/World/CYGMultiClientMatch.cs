using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>Configuration for an in-process multi-client deterministic match.</summary>
    [Serializable]
    public sealed class CYGMultiClientOptions
    {
        public int ClientCount = 2;
        public int PlayersPerClient = 2;
        public int Ticks = 300;
        public ulong Seed = 2026UL;

        public CYGMultiClientOptions Clone()
        {
            return new CYGMultiClientOptions
            {
                ClientCount = ClientCount,
                PlayersPerClient = PlayersPerClient,
                Ticks = Ticks,
                Seed = Seed,
            };
        }
    }

    /// <summary>Snapshot of one client for the editor tool and test assertions.</summary>
    public struct CYGMultiClientStatus
    {
        public int Client;
        public int Tick;
        public int EntityCount;
        public uint Checksum;
        public FPVector3 Position;
        public bool Alive;
    }

    /// <summary>
    /// Runs two to four independent deterministic worlds side by side in one
    /// process. Each client owns its own <see cref="CYGWorld"/>, system runner
    /// and rollback runner, receives the same bot input stream, and is stepped
    /// in lockstep. A checksum mismatch is reported at the first divergent
    /// tick, which is exactly what an in-editor multi-client test needs.
    ///
    /// This is a test/tooling class, not a transport. It does not replace the
    /// network input hub or the server-authoritative path.
    /// </summary>
    public sealed class CYGMultiClientMatch
    {
        private sealed class Client
        {
            public CYGWorld World;
            public CYGSystemRunner Systems;
            public CYGWorldRunner Runner;
            public CYGEntityRef[] Entities;
            public int LocalPlayerId;
        }

        private readonly CYGMultiClientOptions _options;
        private readonly List<Client> _clients = new List<Client>();
        private readonly CYGPlayerInput[] _inputs;
        private readonly int[] _inputIds;
        private int _tick;
        private bool _diverged;
        private string _divergence = string.Empty;

        public CYGMultiClientMatch(CYGMultiClientOptions options)
        {
            _options = (options ?? new CYGMultiClientOptions()).Clone();
            _options.ClientCount = Clamp(_options.ClientCount, 1, 8);
            _options.PlayersPerClient = Clamp(_options.PlayersPerClient, 1, 8);
            _options.Ticks = Math.Max(0, _options.Ticks);

            _inputs = new CYGPlayerInput[_options.PlayersPerClient];
            _inputIds = new int[_options.PlayersPerClient];
            Build();
        }

        public CYGMultiClientOptions Options => _options;

        public int ClientCount => _clients.Count;

        public int Tick => _tick;

        public int TotalTicks => _options.Ticks;

        public bool IsComplete => _tick >= _options.Ticks;

        public bool Diverged => _diverged;

        public string Divergence => _divergence;

        public bool StepOnce()
        {
            if (IsComplete)
            {
                return false;
            }

            for (int p = 0; p < _inputs.Length; p++)
            {
                int playerId = p + 1;
                _inputIds[p] = playerId;
                _inputs[p] = CYGDeterministicBot.Sample(playerId, _tick);
            }

            for (int c = 0; c < _clients.Count; c++)
            {
                Client client = _clients[c];
                int tick = _tick;
                client.Runner.Tick(_ => client.Systems.Tick(tick, false, _inputs, _inputIds, _inputs.Length));
            }

            _tick++;
            CheckDivergence();
            return true;
        }

        public void RunToEnd()
        {
            while (StepOnce())
            {
            }
        }

        public void Reset()
        {
            _tick = 0;
            _diverged = false;
            _divergence = string.Empty;
            _clients.Clear();
            Build();
        }

        public CYGMultiClientStatus GetStatus(int clientIndex)
        {
            if ((uint)clientIndex >= (uint)_clients.Count)
            {
                throw new ArgumentOutOfRangeException(nameof(clientIndex));
            }

            Client client = _clients[clientIndex];
            CYGMultiClientStatus status = new CYGMultiClientStatus
            {
                Client = clientIndex,
                Tick = _tick,
                EntityCount = client.World.EntityCount,
                Checksum = client.World.ComputeChecksum(),
            };

            if (client.Entities.Length > 0)
            {
                CYGSimAgent agent;
                if (client.World.TryGet(client.Entities[0], out agent))
                {
                    status.Position = agent.Position;
                    status.Alive = agent.Health > 0;
                }
            }
            return status;
        }

        public uint[] GetChecksums()
        {
            uint[] checksums = new uint[_clients.Count];
            for (int i = 0; i < _clients.Count; i++)
            {
                checksums[i] = _clients[i].World.ComputeChecksum();
            }
            return checksums;
        }

        private void Build()
        {
            for (int c = 0; c < _options.ClientCount; c++)
            {
                CYGDeterminismConfig config = CYGDeterminismConfig.Default;
                CYGWorld world = new CYGWorld(_options.Seed);
                world.Register<CYGSimAgent>();

                CYGEntityRef[] entities = new CYGEntityRef[_options.PlayersPerClient];
                for (int p = 0; p < entities.Length; p++)
                {
                    int playerId = p + 1;
                    FPVector3 position = new FPVector3(
                        FP.FromInt(playerId * 2 - 3),
                        FP.Zero,
                        FP.Zero);
                    entities[p] = world.CreateEntity();
                    world.Set(entities[p], CYGSimAgent.Create(playerId, position));
                }

                CYGSystemRunner systems = new CYGSystemRunner(world, config);
                systems.AddSystem(new CYGCharacterMovementSystem());
                systems.Start();

                _clients.Add(new Client
                {
                    World = world,
                    Systems = systems,
                    Runner = new CYGWorldRunner(world, config),
                    Entities = entities,
                    LocalPlayerId = 1,
                });
            }
        }

        private void CheckDivergence()
        {
            if (_diverged || _clients.Count < 2)
            {
                return;
            }

            uint expected = _clients[0].World.ComputeChecksum();
            for (int i = 1; i < _clients.Count; i++)
            {
                uint actual = _clients[i].World.ComputeChecksum();
                if (actual != expected)
                {
                    _diverged = true;
                    _divergence = "Client " + i + " diverged at tick " + _tick
                        + ": expected 0x" + expected.ToString("X8")
                        + ", actual 0x" + actual.ToString("X8") + ".";
                    return;
                }
            }
        }

        /// <summary>Deterministic bot input stream; identical on every client.</summary>
        /// <summary>Delegates to <see cref="CYGDeterministicBot"/>.</summary>
        public static CYGPlayerInput Sample(int playerId, int tick)
        {
            return CYGDeterministicBot.Sample(playerId, tick);
        }

        private static int Clamp(int value, int min, int max)
        {
            if (value < min) return min;
            if (value > max) return max;
            return value;
        }
    }
}
