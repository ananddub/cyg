using CookYourGame.Sim;

namespace CYG.CodeGen.Tests
{
    [CYGComponent(Id = 1001)]
    public partial struct CharacterState
    {
        public FP Health;
        public FPVector3 Velocity;
        public bool Grounded;
    }
}
