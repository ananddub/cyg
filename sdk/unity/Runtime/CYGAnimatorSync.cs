using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Networked Animator. The authority polls cached animator parameters on each fixed network
    /// tick and broadcasts a 32-bit dirty mask plus changed values; triggers are application-level
    /// events sent through SetTrigger (never polled) and the layer state block is emitted once on
    /// the first delta after spawn so late joiners recover their animation state.
    /// Payload: [byte flags: bit0 hasParams, bit1 hasState, bit2 hasTriggers]
    ///   params block:   [uint dirtyMask varint][per set bit ascending: value per parameter type]
    ///   triggers block: [byte triggerCount][uint full nameHash per trigger]
    ///   state block:    [byte layerCount][per layer: int fullPathHash, half normalizedTime]
    /// Blocks appear in the order params, triggers, state whenever their flag bit is set.
    /// Full 32-bit name hashes are sent rather than a truncated ushort because Animator.SetTrigger
    /// only matches the exact StringToHash value the game code uses locally.
    /// </summary>
    [AddComponentMenu("CookYourGame/CYG Animator Sync")]
    public class CYGAnimatorSync : CYGBehaviour
    {
        private const int MaxParams = 32;
        private const int TriggerQueueCapacity = 8;
        private const int TriggerQueueMask = TriggerQueueCapacity - 1;
        private const int MaxLayers = 8;
        private const float FloatEpsilon = 1e-4f;

        private const byte FlagParams = 1;
        private const byte FlagState = 2;
        private const byte FlagTriggers = 4;

        private const byte KindInt = 0;
        private const byte KindFloat = 1;
        private const byte KindBool = 2;

        private Animator _animator;
        private readonly int[] _paramHashes = new int[MaxParams];
        private readonly byte[] _paramKinds = new byte[MaxParams];
        private readonly int[] _lastInts = new int[MaxParams];
        private readonly float[] _lastFloats = new float[MaxParams];
        private readonly bool[] _lastBools = new bool[MaxParams];
        private int _paramCount;

        private readonly int[] _triggerQueue = new int[TriggerQueueCapacity];
        private int _triggerHead;
        private int _triggerCount;

        private bool _paramsDirty;
        private uint _dirtyMask;
        private bool _needFullState;

        protected virtual void Awake()
        {
            _animator = GetComponent<Animator>();
            if (_animator == null)
            {
                Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": no Animator found, disabling self.");
                enabled = false;
                return;
            }
            CacheParameters();
        }

        private void CacheParameters()
        {
            AnimatorControllerParameter[] parameters = _animator.parameters;
            _paramCount = 0;
            for (int i = 0; i < parameters.Length && _paramCount < MaxParams; i++)
            {
                AnimatorControllerParameterType type = parameters[i].type;
                byte kind;
                // WHY: triggers are edge events, not state; polling them would consume the
                // Animator's one-shot flag, so they only travel through the explicit SetTrigger API.
                if (type == AnimatorControllerParameterType.Int) kind = KindInt;
                else if (type == AnimatorControllerParameterType.Float) kind = KindFloat;
                else if (type == AnimatorControllerParameterType.Bool) kind = KindBool;
                else continue;

                _paramHashes[_paramCount] = parameters[i].nameHash;
                _paramKinds[_paramCount] = kind;
                _lastInts[_paramCount] = parameters[i].type == AnimatorControllerParameterType.Int
                    ? _animator.GetInteger(parameters[i].nameHash)
                    : 0;
                _lastFloats[_paramCount] = 0f;
                _lastBools[_paramCount] = false;
                _paramCount++;
            }
            if (parameters.Length > MaxParams)
            {
                Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": more than " + MaxParams +
                                 " animator parameters, extras are not synced.");
            }
        }

        protected virtual void OnEnable()
        {
            CYGNetworkManager.OnFixedNetworkTick += FixedTick;
            RegisterAsObservable();
        }

        protected virtual void OnDisable()
        {
            CYGNetworkManager.OnFixedNetworkTick -= FixedTick;
            UnregisterAsObservable();
        }

        protected override void OnSpawned()
        {
            if (HasAuthority)
            {
                _needFullState = true;
                RecordBaseline();
            }
        }

        protected override void OnOwnerChanged(CYGPlayer newOwner)
        {
            if (HasAuthority)
            {
                _needFullState = true;
                RecordBaseline();
            }
            else
            {
                _paramsDirty = false;
                _dirtyMask = 0;
                _triggerHead = 0;
                _triggerCount = 0;
            }
        }

        private void RecordBaseline()
        {
            if (_animator == null) return;
            for (int i = 0; i < _paramCount; i++)
            {
                switch (_paramKinds[i])
                {
                    case KindInt: _lastInts[i] = _animator.GetInteger(_paramHashes[i]); break;
                    case KindFloat: _lastFloats[i] = _animator.GetFloat(_paramHashes[i]); break;
                    case KindBool: _lastBools[i] = _animator.GetBool(_paramHashes[i]); break;
                }
            }
        }

        private void FixedTick()
        {
            if (!HasAuthority || _animator == null) return;

            uint mask = 0;
            for (int i = 0; i < _paramCount; i++)
            {
                switch (_paramKinds[i])
                {
                    case KindInt:
                    {
                        int value = _animator.GetInteger(_paramHashes[i]);
                        if (value != _lastInts[i]) { _lastInts[i] = value; mask |= 1u << i; }
                        break;
                    }
                    case KindFloat:
                    {
                        float value = _animator.GetFloat(_paramHashes[i]);
                        if (Mathf.Abs(value - _lastFloats[i]) > FloatEpsilon) { _lastFloats[i] = value; mask |= 1u << i; }
                        break;
                    }
                    case KindBool:
                    {
                        bool value = _animator.GetBool(_paramHashes[i]);
                        if (value != _lastBools[i]) { _lastBools[i] = value; mask |= 1u << i; }
                        break;
                    }
                }
            }
            if (mask != 0)
            {
                _dirtyMask |= mask;
                _paramsDirty = true;
            }
        }

        /// <summary>Fires the trigger locally and, with authority, queues it for broadcast.</summary>
        public void SetTrigger(string name)
        {
            if (_animator == null) return;
            int hash = Animator.StringToHash(name);
            _animator.SetTrigger(hash);
            if (!HasAuthority) return;
            if (_triggerCount == TriggerQueueCapacity)
            {
                Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": trigger queue full, dropped " + name);
                return;
            }
            _triggerQueue[(_triggerHead + _triggerCount) & TriggerQueueMask] = hash;
            _triggerCount++;
        }

        /// <summary>Fires the trigger by hash locally and, with authority, queues it for broadcast.</summary>
        public void SetTrigger(int nameHash)
        {
            if (_animator == null) return;
            _animator.SetTrigger(nameHash);
            if (!HasAuthority) return;
            if (_triggerCount == TriggerQueueCapacity)
            {
                Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": trigger queue full, dropped a trigger");
                return;
            }
            _triggerQueue[(_triggerHead + _triggerCount) & TriggerQueueMask] = nameHash;
            _triggerCount++;
        }

        public override bool HasDelta()
        {
            return _paramsDirty || _triggerCount > 0 || _needFullState || base.HasDelta();
        }

        public override void Serialize(CYGBitWriter writer)
        {
            base.Serialize(writer);

            byte flags = 0;
            if (_paramsDirty) flags |= FlagParams;
            if (_needFullState) flags |= FlagState;
            if (_triggerCount > 0) flags |= FlagTriggers;
            // The flags byte is written even when zero so Deserialize's fixed layout stays symmetric
            // with sync-var-only deltas.
            writer.WriteByte(flags);
            if (flags == 0) return;

            if (_paramsDirty)
            {
                writer.WriteUInt32(_dirtyMask);
                for (int i = 0; i < _paramCount; i++)
                {
                    if ((_dirtyMask & (1u << i)) == 0) continue;
                    switch (_paramKinds[i])
                    {
                        case KindInt: writer.WriteInt32(_lastInts[i]); break;
                        case KindFloat: writer.WriteSingle(_lastFloats[i]); break;
                        case KindBool: writer.WriteBool(_lastBools[i]); break;
                    }
                }
                _dirtyMask = 0;
                _paramsDirty = false;
            }

            if (_triggerCount > 0)
            {
                writer.WriteByte((byte)_triggerCount);
                for (int i = 0; i < _triggerCount; i++)
                {
                    writer.WriteUInt32((uint)_triggerQueue[(_triggerHead + i) & TriggerQueueMask]);
                }
                _triggerHead = 0;
                _triggerCount = 0;
            }

            if (_needFullState)
            {
                WriteLayerStates(writer);
                _needFullState = false;
            }
        }

        private void WriteLayerStates(CYGBitWriter writer)
        {
            int layers = Mathf.Min(_animator != null ? _animator.layerCount : 0, MaxLayers);
            writer.WriteByte((byte)layers);
            for (int layer = 0; layer < layers; layer++)
            {
                AnimatorStateInfo state = _animator.GetCurrentAnimatorStateInfo(layer);
                writer.WriteInt32(state.fullPathHash);
                writer.WriteHalf(Mathf.Clamp01(state.normalizedTime));
            }
        }

        public override void Deserialize(CYGBitReader reader)
        {
            base.Deserialize(reader);
            if (_animator == null)
            {
                Debug.LogError(GetType().Name + " on " + gameObject.name + ": received animator delta but no Animator is attached");
            }

            // Authority decodes to keep stream parsing valid but never overwrites its own animator state.
            bool apply = _animator != null && !HasAuthority;
            byte flags = reader.ReadByte();

            if ((flags & FlagParams) != 0)
            {
                uint mask = reader.ReadUInt32();
                for (int i = 0; i < _paramCount; i++)
                {
                    if ((mask & (1u << i)) == 0) continue;
                    switch (_paramKinds[i])
                    {
                        case KindInt:
                        {
                            int value = reader.ReadInt32();
                            if (apply) _animator.SetInteger(_paramHashes[i], value);
                            break;
                        }
                        case KindFloat:
                        {
                            float value = reader.ReadSingle();
                            if (apply) _animator.SetFloat(_paramHashes[i], value);
                            break;
                        }
                        case KindBool:
                        {
                            bool value = reader.ReadBool();
                            if (apply) _animator.SetBool(_paramHashes[i], value);
                            break;
                        }
                    }
                }
                if (mask != 0 && _paramCount < MaxParams && (mask >> _paramCount) != 0)
                {
                    // Sender knows parameters this client trimmed away (count mismatch); stream is now skewed.
                    Debug.LogError(GetType().Name + " on " + gameObject.name + ": param mask exceeds local parameter set");
                }
            }

            if ((flags & FlagTriggers) != 0)
            {
                byte count = reader.ReadByte();
                for (int i = 0; i < count; i++)
                {
                    int triggerHash = (int)reader.ReadUInt32();
                    if (apply) _animator.SetTrigger(triggerHash);
                }
            }

            if ((flags & FlagState) != 0)
            {
                byte layers = reader.ReadByte();
                for (int layer = 0; layer < layers; layer++)
                {
                    int fullPathHash = reader.ReadInt32();
                    float normalizedTime = reader.ReadHalf();
                    if (apply && layer < _animator.layerCount)
                    {
                        _animator.Play(fullPathHash, layer, normalizedTime);
                    }
                }
            }
        }
    }
}
