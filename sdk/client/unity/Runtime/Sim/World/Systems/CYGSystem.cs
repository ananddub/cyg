using System;

namespace CookYourGame.Sim
{
    /// <summary>Per-tick data passed to every system. Never store this struct.</summary>
    public readonly struct CYGSystemContext
    {
        public readonly CYGWorld World;
        public readonly int Tick;
        public readonly FP DeltaTime;
        public readonly CYGDeterminismConfig Config;
        public readonly bool IsResimulating;
        public readonly CYGPlayerInput[] Inputs;
        public readonly int[] InputPlayerIds;
        public readonly int InputCount;

        public CYGSystemContext(
            CYGWorld world,
            int tick,
            FP deltaTime,
            CYGDeterminismConfig config,
            bool isResimulating)
            : this(world, tick, deltaTime, config, isResimulating, null, null, 0)
        {
        }

        public CYGSystemContext(
            CYGWorld world,
            int tick,
            FP deltaTime,
            CYGDeterminismConfig config,
            bool isResimulating,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount)
        {
            World = world;
            Tick = tick;
            DeltaTime = deltaTime;
            Config = config;
            IsResimulating = isResimulating;
            Inputs = inputs;
            InputPlayerIds = inputPlayerIds;
            InputCount = inputCount;
        }

        /// <summary>Finds the input for a player in this tick, or false when none was supplied.</summary>
        public bool TryGetInput(int playerId, out CYGPlayerInput input)
        {
            if (Inputs != null && InputPlayerIds != null)
            {
                int count = InputCount < Inputs.Length ? InputCount : Inputs.Length;
                if (count > InputPlayerIds.Length)
                {
                    count = InputPlayerIds.Length;
                }

                for (int i = 0; i < count; i++)
                {
                    if (InputPlayerIds[i] == playerId)
                    {
                        input = Inputs[i].Clamped();
                        return true;
                    }
                }
            }

            input = default;
            return false;
        }
    }

    /// <summary>
    /// Stateless simulation system. Order and group membership are assigned by
    /// <see cref="CYGSystemRunner"/>; systems must not keep per-tick state that
    /// is not stored in the world, otherwise rollback resimulation diverges.
    /// </summary>
    public abstract class CYGSystem
    {
        private CYGSystemGroup _group;
        private int _order;
        private bool _enabledSelf = true;

        public CYGSystemGroup Group
        {
            get { return _group; }
        }

        public int Order
        {
            get { return _order; }
        }

        public bool EnabledSelf
        {
            get { return _enabledSelf; }
        }

        public bool IsEnabledInHierarchy
        {
            get { return _enabledSelf && (_group == null || _group.IsEnabledInHierarchy); }
        }

        internal void Attach(CYGSystemGroup group, int order)
        {
            _group = group;
            _order = order;
        }

        internal void SetEnabledSelf(bool enabled)
        {
            _enabledSelf = enabled;
        }

        public virtual void OnInit(CYGWorld world)
        {
        }

        public virtual void OnUpdate(in CYGSystemContext context)
        {
        }

        public virtual void OnEnabled(CYGWorld world)
        {
        }

        public virtual void OnDisabled(CYGWorld world)
        {
        }
    }

    /// <summary>
    /// Named group that can enable or disable a subtree of systems in one call.
    /// Group membership is fixed when a system is added to the runner.
    /// </summary>
    public sealed class CYGSystemGroup
    {
        private bool _enabledSelf = true;

        public CYGSystemGroup(string name, CYGSystemGroup parent = null)
        {
            Name = name ?? throw new ArgumentNullException(nameof(name));
            Parent = parent;
        }

        public string Name { get; }

        public CYGSystemGroup Parent { get; }

        public bool EnabledSelf
        {
            get { return _enabledSelf; }
        }

        public bool IsEnabledInHierarchy
        {
            get { return _enabledSelf && (Parent == null || Parent.IsEnabledInHierarchy); }
        }

        internal void SetEnabledSelf(bool enabled)
        {
            _enabledSelf = enabled;
        }
    }
}
