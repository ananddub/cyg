using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    public enum CYGNetReliability
    {
        Unreliable,
        Reliable
    }

    /// <summary>Lag/loss model for deterministic netcode tests. Values are in whole ticks.</summary>
    public sealed class CYGNetSimConfig
    {
        public int LatencyTicks = 4;
        public int JitterTicks;
        public int LossPerMille;
        public int DuplicatePerMille;
        public bool Reorder = true;

        public void Validate()
        {
            if (LatencyTicks < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(LatencyTicks));
            }
            if (JitterTicks < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(JitterTicks));
            }
            if (LossPerMille < 0 || LossPerMille > 1000)
            {
                throw new ArgumentOutOfRangeException(nameof(LossPerMille));
            }
            if (DuplicatePerMille < 0 || DuplicatePerMille > 1000)
            {
                throw new ArgumentOutOfRangeException(nameof(DuplicatePerMille));
            }
        }
    }

    public sealed class CYGNetPacket
    {
        public int From;
        public int To;
        public int Sequence;
        public int SentTick;
        public int DeliveryTick;
        public CYGNetReliability Reliability;
        public byte[] Payload;
        public int Length;
    }

    /// <summary>
    /// Simulated one-way links with latency, jitter, loss, duplication, and
    /// out-of-order delivery. Delivery order is sorted by delivery tick and
    /// send sequence, so the same seed always produces the same receive order.
    /// </summary>
    public sealed class CYGNetworkSimulator
    {
        private readonly CYGNetSimConfig _config;
        private readonly List<CYGNetPacket> _inFlight = new List<CYGNetPacket>();
        private CYGRngState _rng;
        private int _nextSequence;

        public CYGNetworkSimulator(CYGNetSimConfig config, ulong seed = 1UL)
        {
            if (config == null)
            {
                throw new ArgumentNullException(nameof(config));
            }
            config.Validate();
            _config = config;
            _rng = new CYGRngState(seed);
        }

        public int CurrentTick { get; private set; }

        public int SentCount { get; private set; }

        public int DeliveredCount { get; private set; }

        public int DroppedCount { get; private set; }

        public int DuplicatedCount { get; private set; }

        public int InFlightCount
        {
            get { return _inFlight.Count; }
        }

        /// <summary>Advances the transport clock by one tick.</summary>
        public void Advance()
        {
            CurrentTick++;
        }

        public void Send(
            int from,
            int to,
            byte[] payload,
            int offset,
            int length,
            CYGNetReliability reliability = CYGNetReliability.Unreliable)
        {
            if (from < 0 || to < 0)
            {
                throw new ArgumentOutOfRangeException(nameof(from));
            }
            if (payload == null)
            {
                throw new ArgumentNullException(nameof(payload));
            }
            CYGWire.RequireReadable(payload, offset, length);

            SentCount++;
            if (_config.LossPerMille > 0 && _rng.NextInt(0, 1000) < _config.LossPerMille)
            {
                DroppedCount++;
                return;
            }

            Enqueue(from, to, payload, offset, length, reliability, 0);
            if (_config.DuplicatePerMille > 0 && _rng.NextInt(0, 1000) < _config.DuplicatePerMille)
            {
                DuplicatedCount++;
                Enqueue(from, to, payload, offset, length, reliability, _rng.NextInt(0, _config.JitterTicks + 2));
            }
        }

        /// <summary>Collects packets addressed to <paramref name="to"/> due at the current tick.</summary>
        public int Receive(int to, List<CYGNetPacket> output)
        {
            if (output == null)
            {
                throw new ArgumentNullException(nameof(output));
            }

            int before = output.Count;
            for (int i = _inFlight.Count - 1; i >= 0; i--)
            {
                CYGNetPacket packet = _inFlight[i];
                if (packet.To == to && packet.DeliveryTick <= CurrentTick)
                {
                    _inFlight.RemoveAt(i);
                    output.Add(packet);
                }
            }

            int added = output.Count - before;
            if (added > 1)
            {
                output.Sort(before, added, PacketComparer.Instance);
            }
            DeliveredCount += added;
            return added;
        }

        public void Clear()
        {
            _inFlight.Clear();
            CurrentTick = 0;
            SentCount = 0;
            DeliveredCount = 0;
            DroppedCount = 0;
            DuplicatedCount = 0;
            _nextSequence = 0;
            _rng = new CYGRngState(1UL);
        }

        private void Enqueue(
            int from,
            int to,
            byte[] payload,
            int offset,
            int length,
            CYGNetReliability reliability,
            int extraDelay)
        {
            int jitter = _config.JitterTicks > 0 ? _rng.NextInt(0, _config.JitterTicks + 1) : 0;
            int delay = _config.LatencyTicks + jitter + extraDelay;
            if (delay < 1)
            {
                delay = 1;
            }

            byte[] copy = new byte[length];
            Array.Copy(payload, offset, copy, 0, length);
            _inFlight.Add(new CYGNetPacket
            {
                From = from,
                To = to,
                Sequence = _nextSequence++,
                SentTick = CurrentTick,
                DeliveryTick = CurrentTick + delay,
                Reliability = reliability,
                Payload = copy,
                Length = length
            });
        }

        private sealed class PacketComparer : IComparer<CYGNetPacket>
        {
            public static readonly PacketComparer Instance = new PacketComparer();

            public int Compare(CYGNetPacket left, CYGNetPacket right)
            {
                int byDelivery = left.DeliveryTick.CompareTo(right.DeliveryTick);
                if (byDelivery != 0)
                {
                    return byDelivery;
                }
                return left.Sequence.CompareTo(right.Sequence);
            }
        }
    }
}
