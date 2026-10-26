using System;
using System.Collections.Generic;

namespace CookYourGame.Sim
{
    /// <summary>
    /// Deterministic system scheduler. Update order is ascending
    /// <see cref="CYGSystem.Order"/> with insertion order as the tie-break, and
    /// entity filters iterate slots by index. Enable/disable callbacks only
    /// fire when the effective hierarchy state changes.
    /// </summary>
    public sealed class CYGSystemRunner
    {
        private readonly CYGWorld _world;
        private readonly CYGDeterminismConfig _config;
        private readonly List<CYGSystem> _systems = new List<CYGSystem>();
        private bool _started;

        public CYGSystemRunner(CYGWorld world, CYGDeterminismConfig config)
        {
            _world = world ?? throw new ArgumentNullException(nameof(world));
            _config = config ?? throw new ArgumentNullException(nameof(config));
        }

        public CYGWorld World
        {
            get { return _world; }
        }

        public CYGDeterminismConfig Config
        {
            get { return _config; }
        }

        public int Count
        {
            get { return _systems.Count; }
        }

        public IReadOnlyList<CYGSystem> Systems
        {
            get { return _systems; }
        }

        public void AddSystem(CYGSystem system, CYGSystemGroup group = null, int order = 0)
        {
            if (system == null)
            {
                throw new ArgumentNullException(nameof(system));
            }
            if (_systems.Contains(system))
            {
                throw new InvalidOperationException("System is already registered: " + system.GetType().Name);
            }

            system.Attach(group, order);
            int index = _systems.Count;
            for (int i = 0; i < _systems.Count; i++)
            {
                if (_systems[i].Order > order)
                {
                    index = i;
                    break;
                }
            }
            _systems.Insert(index, system);

            if (_started)
            {
                system.OnInit(_world);
            }
        }

        public bool RemoveSystem(CYGSystem system)
        {
            if (system == null)
            {
                return false;
            }
            return _systems.Remove(system);
        }

        public void Clear()
        {
            _systems.Clear();
        }

        public void Start()
        {
            if (_started)
            {
                return;
            }

            _started = true;
            for (int i = 0; i < _systems.Count; i++)
            {
                _systems[i].OnInit(_world);
            }
        }

        public void Tick(int tick, bool isResimulating = false)
        {
            Tick(tick, isResimulating, null, null, 0);
        }

        public void Tick(
            int tick,
            bool isResimulating,
            CYGPlayerInput[] inputs,
            int[] inputPlayerIds,
            int inputCount)
        {
            Start();
            CYGSystemContext context = new CYGSystemContext(
                _world,
                tick,
                _config.FixedDeltaTime,
                _config,
                isResimulating,
                inputs,
                inputPlayerIds,
                inputCount);

            for (int i = 0; i < _systems.Count; i++)
            {
                CYGSystem system = _systems[i];
                if (system.IsEnabledInHierarchy)
                {
                    system.OnUpdate(in context);
                }
            }
        }

        public void SetSystemEnabled(CYGSystem system, bool enabled)
        {
            RequireRegistered(system);
            bool before = system.IsEnabledInHierarchy;
            system.SetEnabledSelf(enabled);
            NotifyTransition(system, before, system.IsEnabledInHierarchy);
        }

        public void SetGroupEnabled(CYGSystemGroup group, bool enabled)
        {
            if (group == null)
            {
                throw new ArgumentNullException(nameof(group));
            }

            bool[] before = new bool[_systems.Count];
            for (int i = 0; i < _systems.Count; i++)
            {
                before[i] = _systems[i].IsEnabledInHierarchy;
            }

            group.SetEnabledSelf(enabled);

            for (int i = 0; i < _systems.Count; i++)
            {
                NotifyTransition(_systems[i], before[i], _systems[i].IsEnabledInHierarchy);
            }
        }

        private void NotifyTransition(CYGSystem system, bool before, bool after)
        {
            if (!_started || before == after)
            {
                return;
            }

            if (after)
            {
                system.OnEnabled(_world);
            }
            else
            {
                system.OnDisabled(_world);
            }
        }

        private void RequireRegistered(CYGSystem system)
        {
            if (system == null)
            {
                throw new ArgumentNullException(nameof(system));
            }
            if (!_systems.Contains(system))
            {
                throw new InvalidOperationException("System is not registered: " + system.GetType().Name);
            }
        }
    }
}
