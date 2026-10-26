using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Networked Transform. The authority samples pose on each fixed network tick and sends
    /// threshold-passed deltas; proxies buffer incoming snapshots and interpolate at
    /// renderTime = now - 100ms. Authoritative receives are decoded but never written to the
    /// Transform (the local simulation owns the pose).
    /// Payload after the view's component walk: [byte flags] + pose fields, and base sync var
    /// block first when this behaviour registers sync vars.
    /// </summary>
    [RequireComponent(typeof(Transform))]
    [AddComponentMenu("CookYourGame/CYG Transform Sync")]
    public class CYGTransformSync : CYGBehaviour
    {
        private const int BufferCapacity = 32;
        private const int BufferMask = BufferCapacity - 1;
        private const float InterpolationDelay = 0.07f;
        private const float MaxExtrapolation = 0.15f;
        private const float PositionThresholdSqr = 0.002f * 0.002f;
        private const float RotationThresholdDeg = 0.02f;
        private const float ScaleThresholdSqr = 1e-6f;
        // WHY: a >5m jump almost always means respawn/teleport, not motion; interpolating the gap
        // looks like flying across the map, so both sides snap and clear the buffer instead.
        private const float TeleportDistanceSqr = 5f * 5f;

        private const byte FlagPosition = 1;
        private const byte FlagRotation = 2;
        private const byte FlagScale = 4;
        private const byte FlagTeleport = 8;

        private struct Snapshot
        {
            public double time;
            public Vector3 position;
            public Quaternion rotation;
            public Vector3 velocity;
        }

        /// <summary>Also sync localScale (off by default; usually authored, not simulated).</summary>
        public bool syncScale;

        private Transform _transform;
        private readonly Snapshot[] _buffer = new Snapshot[BufferCapacity];
        private int _head;
        private int _count;

        private bool _poseDirty;
        private byte _outFlags;
        private Vector3 _outPosition;
        private Quaternion _outRotation;
        private Vector3 _outScale;

        private Vector3 _lastSentPosition;
        private Quaternion _lastSentRotation;
        private Vector3 _lastSentScale;
        private bool _hasSentBaseline;

        private Vector3 _lastIncomingPosition;
        private Quaternion _lastIncomingRotation;
        private double _lastIncomingTime;
        private bool _hasIncomingBaseline;

        protected virtual void Awake()
        {
            _transform = transform;
        }

        protected virtual void OnEnable()
        {
            if (_transform == null) _transform = transform;
            RegisterAsObservable();
            CYGNetworkManager.OnFixedNetworkTick += FixedTick;
        }

        protected virtual void OnDisable()
        {
            CYGNetworkManager.OnFixedNetworkTick -= FixedTick;
            UnregisterAsObservable();
        }

        protected override void OnSpawned()
        {
            _hasSentBaseline = false;
            _hasIncomingBaseline = false;
            _poseDirty = false;
            _outFlags = 0;
            _head = 0;
            _count = 0;
        }

        private void FixedTick()
        {
            if (!HasAuthority || _transform == null) return;

            Vector3 position = _transform.position;
            Quaternion rotation = _transform.rotation;
            Vector3 scale = _transform.localScale;

            if (!_hasSentBaseline)
            {
                _lastSentPosition = position;
                _lastSentRotation = rotation;
                _lastSentScale = scale;
                _hasSentBaseline = true;
                return;
            }

            byte flags = 0;
            float positionDeltaSqr = (position - _lastSentPosition).sqrMagnitude;
            if (positionDeltaSqr >= PositionThresholdSqr) flags |= FlagPosition;
            if (Quaternion.Angle(_lastSentRotation, rotation) > RotationThresholdDeg) flags |= FlagRotation;
            if (syncScale && (scale - _lastSentScale).sqrMagnitude >= ScaleThresholdSqr) flags |= FlagScale;
            if (flags == 0) return;
            if (positionDeltaSqr >= TeleportDistanceSqr) flags |= FlagTeleport;

            _outFlags |= flags;
            _outPosition = position;
            _outRotation = rotation;
            _outScale = scale;
            _lastSentPosition = position;
            _lastSentRotation = rotation;
            _lastSentScale = scale;
            _poseDirty = true;
        }

        public override bool HasDelta()
        {
            return _poseDirty || base.HasDelta();
        }

        public override void Serialize(CYGBitWriter writer)
        {
            base.Serialize(writer);
            // The flags byte is written even when zero so Deserialize's fixed layout stays symmetric
            // with sync-var-only deltas.
            writer.WriteByte(_outFlags);
            if (!_poseDirty) return;

            if ((_outFlags & FlagPosition) != 0) writer.WriteVector3Packed(_outPosition);
            if ((_outFlags & FlagRotation) != 0) writer.WriteQuaternion(_outRotation);
            if ((_outFlags & FlagScale) != 0) writer.WriteVector3(_outScale);
            _outFlags = 0;
            _poseDirty = false;
        }

        public override void Deserialize(CYGBitReader reader)
        {
            base.Deserialize(reader);

            byte flags = reader.ReadByte();
            Vector3 position = _hasIncomingBaseline
                ? _lastIncomingPosition
                : (_transform != null ? _transform.position : Vector3.zero);
            Quaternion rotation = _hasIncomingBaseline
                ? _lastIncomingRotation
                : (_transform != null ? _transform.rotation : Quaternion.identity);

            if ((flags & FlagPosition) != 0) position = reader.ReadVector3Packed();
            if ((flags & FlagRotation) != 0) rotation = reader.ReadQuaternion();
            bool hasScale = (flags & FlagScale) != 0;
            Vector3 scale = hasScale ? reader.ReadVector3() : Vector3.zero;

            if (HasAuthority) return;

            if (hasScale && _transform != null) _transform.localScale = scale;

            double now = Time.unscaledTimeAsDouble;
            Vector3 velocity = Vector3.zero;
            if (_hasIncomingBaseline)
            {
                double deltaTime = now - _lastIncomingTime;
                if (deltaTime > 0.000001 && deltaTime < 1.0)
                {
                    velocity = (position - _lastIncomingPosition) / (float)deltaTime;
                }
            }
            _lastIncomingPosition = position;
            _lastIncomingRotation = rotation;
            _lastIncomingTime = now;
            _hasIncomingBaseline = true;

            bool teleport = (flags & FlagTeleport) != 0 ||
                            (_count > 0 && (position - _buffer[(_head - 1) & BufferMask].position).sqrMagnitude >= TeleportDistanceSqr);
            if (teleport || _count == 0)
            {
                _head = 0;
                _count = 0;
                PushSnapshot(position, rotation, Vector3.zero);
                if (_transform != null)
                {
                    _transform.position = position;
                    _transform.rotation = rotation;
                }
                return;
            }

            PushSnapshot(position, rotation, velocity);
        }

        private void PushSnapshot(Vector3 position, Quaternion rotation, Vector3 velocity)
        {
            Snapshot snapshot = default;
            snapshot.time = Time.unscaledTimeAsDouble;
            snapshot.position = position;
            snapshot.rotation = rotation;
            snapshot.velocity = velocity;
            _buffer[_head] = snapshot;
            _head = (_head + 1) & BufferMask;
            if (_count < BufferCapacity) _count++;
        }

        private Snapshot At(int indexFromOldest)
        {
            return _buffer[(_head - _count + indexFromOldest) & BufferMask];
        }

        private void Update()
        {
            if (_transform == null || _count == 0 || HasAuthority) return;

            double renderTime = Time.unscaledTimeAsDouble - InterpolationDelay;
            Snapshot newest = At(_count - 1);
            Snapshot oldest = At(0);

            if (renderTime >= newest.time)
            {
                double extrap = renderTime - newest.time;
                if (extrap > MaxExtrapolation) extrap = MaxExtrapolation;
                float damp = Mathf.Clamp01(1f - (float)(extrap / MaxExtrapolation));
                _transform.position = newest.position + newest.velocity * (float)extrap * damp;
                _transform.rotation = newest.rotation;
                return;
            }

            if (renderTime <= oldest.time)
            {
                _transform.position = oldest.position;
                _transform.rotation = oldest.rotation;
                return;
            }

            for (int i = 0; i < _count - 1; i++)
            {
                Snapshot a = At(i);
                Snapshot b = At(i + 1);
                if (renderTime < a.time || renderTime > b.time) continue;

                double span = b.time - a.time;
                if (span <= 0.000001)
                {
                    _transform.position = b.position;
                    _transform.rotation = b.rotation;
                    return;
                }

                float t = Mathf.Clamp01((float)((renderTime - a.time) / span));
                _transform.position = Vector3.Lerp(a.position, b.position, t);
                _transform.rotation = Quaternion.Slerp(a.rotation, b.rotation, t);
                return;
            }
        }

        /// <summary>Teleports the pose and broadcasts the teleport flag so proxies skip interpolation.</summary>
        public void SetPosition(Vector3 worldPosition)
        {
            if (_transform == null) return;
            _transform.position = worldPosition;
            _lastSentPosition = worldPosition;

            if (HasAuthority)
            {
                _outPosition = worldPosition;
                _outRotation = _transform.rotation;
                _outScale = _transform.localScale;
                _outFlags |= FlagPosition | FlagTeleport;
                _poseDirty = true;
            }
            else
            {
                _head = 0;
                _count = 0;
                PushSnapshot(worldPosition, _transform.rotation, Vector3.zero);
                _lastIncomingPosition = worldPosition;
                _lastIncomingRotation = _transform.rotation;
                _lastIncomingTime = Time.unscaledTimeAsDouble;
                _hasIncomingBaseline = true;
            }
        }

        /// <summary>Teleports the rotation and broadcasts the teleport-tagged pose.</summary>
        public void SetRotation(Quaternion worldRotation)
        {
            if (_transform == null) return;
            _transform.rotation = worldRotation;
            _lastSentRotation = worldRotation;

            if (HasAuthority)
            {
                _outPosition = _transform.position;
                _outRotation = worldRotation;
                _outScale = _transform.localScale;
                _outFlags |= FlagRotation | FlagTeleport;
                _poseDirty = true;
            }
            else
            {
                _head = 0;
                _count = 0;
                PushSnapshot(_transform.position, worldRotation, Vector3.zero);
                _lastIncomingPosition = _transform.position;
                _lastIncomingRotation = worldRotation;
                _lastIncomingTime = Time.unscaledTimeAsDouble;
                _hasIncomingBaseline = true;
            }
        }
    }
}
