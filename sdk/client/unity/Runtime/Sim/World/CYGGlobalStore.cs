using System;
using System.Runtime.CompilerServices;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Non-generic view over the optional global-state value. Worlds keep this
    /// form so snapshots and checksums do not allocate or box per tick.
    /// </summary>
    public interface ICYGGlobalStore
    {
        Type ValueType { get; }

        int WireSize { get; }

        void WriteTo(byte[] buffer, int offset);

        void ReadFrom(byte[] buffer, int offset);

        void ComputeChecksum(ref uint crc);

        void Clear();
    }

    /// <summary>
    /// Typed holder for simulation-global state. The generated <c>CYGGlobals</c>
    /// struct implements <see cref="ICYGValue"/>, so no reflection or boxing is
    /// used on the tick path.
    /// </summary>
    public sealed class CYGGlobalStore<T> : ICYGGlobalStore
        where T : struct, ICYGValue
    {
        public T Value;

        public Type ValueType
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return typeof(T); }
        }

        public int WireSize
        {
            [MethodImpl(MethodImplOptions.AggressiveInlining)]
            get { return Value.WireSize; }
        }

        public void WriteTo(byte[] buffer, int offset)
        {
            Value.WriteTo(buffer, offset);
        }

        public void ReadFrom(byte[] buffer, int offset)
        {
            Value.ReadFrom(buffer, offset);
        }

        public void ComputeChecksum(ref uint crc)
        {
            Value.ComputeChecksum(ref crc);
        }

        public void Clear()
        {
            Value = default;
        }
    }
}
