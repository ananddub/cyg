using System;

namespace CookYourGame.Sim
{
    /// <summary>Stable identity of one event in a simulation tick.</summary>
    public readonly struct CYGEventKey : IEquatable<CYGEventKey>
    {
        public readonly int Tick;
        public readonly int TypeId;
        public readonly int Ordinal;

        public CYGEventKey(int tick, int typeId, int ordinal)
        {
            Tick = tick;
            TypeId = typeId;
            Ordinal = ordinal;
        }

        public bool Equals(CYGEventKey other)
        {
            return Tick == other.Tick && TypeId == other.TypeId && Ordinal == other.Ordinal;
        }

        public override bool Equals(object obj)
        {
            return obj is CYGEventKey other && Equals(other);
        }

        public override int GetHashCode()
        {
            unchecked
            {
                int hash = Tick;
                hash = (hash * 397) ^ TypeId;
                hash = (hash * 397) ^ Ordinal;
                return hash;
            }
        }

        public static bool operator ==(CYGEventKey left, CYGEventKey right) => left.Equals(right);

        public static bool operator !=(CYGEventKey left, CYGEventKey right) => !left.Equals(right);
    }
}
