using CookYourGame.Sim;

namespace CYG.CodeGen.Tests
{
    public enum TestMode : sbyte
    {
        Minus = -1,
        Plus = 1
    }

    [CYGComponent(Id = 1002)]
    public partial struct PrimitiveState
    {
        public byte A;
        public sbyte B;
        public short C;
        public ushort D;
        public int E;
        public uint F;
        public long G;
        public ulong H;
        public char I;
        public bool J;
        public TestMode K;
    }
}
