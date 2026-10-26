using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>A tracked value that a CYGBehaviour packs into its delta when dirty.</summary>
    public interface ICYGSyncVar
    {
        /// <summary>Slot index assigned by the owning behaviour; wire identity of this var.</summary>
        byte Index { get; set; }
        /// <summary>True when Value changed since the last pack/apply.</summary>
        bool IsDirty { get; }
        /// <summary>Marks this var dirty so it will be included in the next outgoing delta.</summary>
        void SetDirty();
        /// <summary>Writes the current value and clears the dirty flag.</summary>
        void PackDirty(CYGBitWriter writer);
        /// <summary>Reads a value from the stream, applies it, clears dirty and fires OnChanged.</summary>
        void ApplyRemoteAndClear(CYGBitReader reader);
    }

    /// <summary>Packs a sync value into a bit writer.</summary>
    public delegate void PackAction<T>(CYGBitWriter writer, T value);

    /// <summary>Unpacks a sync value from a bit reader.</summary>
    public delegate T UnpackAction<T>(CYGBitReader reader);

    /// <summary>Registry of wire codecs for the generic CYGSyncVar&lt;T&gt;.</summary>
    public static class CYGSyncVar
    {
        internal struct Packer
        {
            public Action<CYGBitWriter, object> Pack;
            public Func<CYGBitReader, object> Unpack;
        }

        private static readonly Dictionary<Type, Packer> s_Packers = new Dictionary<Type, Packer>();

        /// <summary>Registers the wire codec used by CYGSyncVar&lt;T&gt; for a type.</summary>
        public static void RegisterPacker<T>(PackAction<T> pack, UnpackAction<T> unpack)
        {
            if (pack == null || unpack == null)
            {
                Debug.LogError("CYGSyncVar.RegisterPacker: null codec for " + typeof(T));
                return;
            }
            s_Packers[typeof(T)] = new Packer
            {
                Pack = (w, o) => pack(w, (T)o),
                Unpack = r => unpack(r)
            };
        }

        /// <summary>True when a codec is registered for the type.</summary>
        public static bool HasPacker(Type type)
        {
            return s_Packers.ContainsKey(type);
        }

        internal static Packer GetPacker(Type type)
        {
            if (s_Packers.TryGetValue(type, out Packer packer)) return packer;
            Debug.LogError("CYGSyncVar: no packer registered for " + type + ". Call CYGSyncVar.RegisterPacker or use a concrete *Var type.");
            return default;
        }

        static CYGSyncVar()
        {
            RegisterPacker<bool>((w, v) => w.WriteBool(v), r => r.ReadBool());
            RegisterPacker<int>((w, v) => w.WriteInt32(v), r => r.ReadInt32());
            RegisterPacker<uint>((w, v) => w.WriteUInt32(v), r => r.ReadUInt32());
            RegisterPacker<long>((w, v) => w.WriteInt64(v), r => r.ReadInt64());
            RegisterPacker<ulong>((w, v) => w.WriteUInt64(v), r => r.ReadUInt64());
            RegisterPacker<float>((w, v) => w.WriteSingle(v), r => r.ReadSingle());
            RegisterPacker<string>((w, v) => w.WriteString(v), r => r.ReadString());
            RegisterPacker<Vector3>((w, v) => w.WriteVector3(v), r => r.ReadVector3());
            RegisterPacker<Quaternion>((w, v) => w.WriteQuaternion(v), r => r.ReadQuaternion());
        }
    }

    /// <summary>Base for value-tracked sync vars: equality-based dirty marking and change events.</summary>
    public abstract class CYGSyncVarBase<T> : ICYGSyncVar where T : IEquatable<T>
    {
        private T _value;
        private bool _dirty;

        /// <summary>When true, the local owner may mutate this var. When false, only server/master client may write.</summary>
        public bool OwnerAuth { get; set; }

        /// <summary>Minimum interval in seconds between sync updates across the network (0 = every tick).</summary>
        public float SendInterval { get; set; }

        private float _lastSendTime;

        /// <inheritdoc/>
        public byte Index { get; set; }

        /// <inheritdoc/>
        public bool IsDirty
        {
            get
            {
                if (!_dirty) return false;
                if (SendInterval > 0f && Time.time - _lastSendTime < SendInterval) return false;
                return true;
            }
        }

        /// <summary>Raised with (old, new) when a remote apply changes the value.</summary>
        public event Action<T, T> OnChanged;

        /// <summary>Current value; assigning a different value marks the var dirty for the next pack.</summary>
        public T Value
        {
            get { return _value; }
            set
            {
                if (EqualityComparer<T>.Default.Equals(_value, value)) return;
                _value = value;
                _dirty = true;
            }
        }

        /// <summary>Marks this var dirty so it will be packed on the next send tick.</summary>
        public void SetDirty()
        {
            _dirty = true;
        }

        /// <summary>Writes a value without the change hook firing (init/hydration only).</summary>
        public void SetSilent(T newValue)
        {
            _value = newValue;
        }

        /// <inheritdoc/>
        public abstract void PackDirty(CYGBitWriter writer);

        /// <inheritdoc/>
        public abstract void ApplyRemoteAndClear(CYGBitReader reader);

        protected T Current
        {
            get { return _value; }
        }

        protected void Write(T newValue)
        {
            _value = newValue;
        }

        protected void ClearDirty()
        {
            _dirty = false;
            _lastSendTime = Time.time;
        }

        protected void RaiseChanged(T oldValue, T newValue)
        {
            if (OnChanged != null) OnChanged(oldValue, newValue);
        }
    }

    /// <summary>Generic sync var driven by a registered packer; boxing per pack, prefer concrete types on hot paths.</summary>
    public sealed class CYGSyncVar<T> : CYGSyncVarBase<T> where T : IEquatable<T>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            CYGSyncVar.GetPacker(typeof(T)).Pack(writer, Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            T oldValue = Current;
            T newValue = (T)CYGSyncVar.GetPacker(typeof(T)).Unpack(reader);
            Write(newValue);
            ClearDirty();
            if (!EqualityComparer<T>.Default.Equals(oldValue, newValue)) RaiseChanged(oldValue, newValue);
        }
    }

    /// <summary>Synced int (zigzag varint on the wire).</summary>
    public sealed class CYGIntVar : CYGSyncVarBase<int>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteInt32(Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            int oldValue = Current;
            int newValue = reader.ReadInt32();
            Write(newValue);
            ClearDirty();
            if (oldValue != newValue) RaiseChanged(oldValue, newValue);
        }

        /// <summary>Reads the value as its base type.</summary>
        public static implicit operator int(CYGIntVar v) { return v.Value; }

        /// <summary>Wraps a value as a clean (non-dirty) initial state.</summary>
        public static implicit operator CYGIntVar(int v)
        {
            CYGIntVar variable = new CYGIntVar();
            variable.SetSilent(v);
            return variable;
        }
    }

    /// <summary>Synced float (4-byte IEEE on the wire).</summary>
    public sealed class CYGFloatVar : CYGSyncVarBase<float>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteSingle(Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            float oldValue = Current;
            float newValue = reader.ReadSingle();
            Write(newValue);
            ClearDirty();
            if (oldValue != newValue) RaiseChanged(oldValue, newValue);
        }

        /// <summary>Reads the value as its base type.</summary>
        public static implicit operator float(CYGFloatVar v) { return v.Value; }

        /// <summary>Wraps a value as a clean (non-dirty) initial state.</summary>
        public static implicit operator CYGFloatVar(float v)
        {
            CYGFloatVar variable = new CYGFloatVar();
            variable.SetSilent(v);
            return variable;
        }
    }

    /// <summary>Synced bool (one bit on the wire).</summary>
    public sealed class CYGBoolVar : CYGSyncVarBase<bool>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteBool(Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            bool oldValue = Current;
            bool newValue = reader.ReadBool();
            Write(newValue);
            ClearDirty();
            if (oldValue != newValue) RaiseChanged(oldValue, newValue);
        }

        /// <summary>Reads the value as its base type.</summary>
        public static implicit operator bool(CYGBoolVar v) { return v.Value; }

        /// <summary>Wraps a value as a clean (non-dirty) initial state.</summary>
        public static implicit operator CYGBoolVar(bool v)
        {
            CYGBoolVar variable = new CYGBoolVar();
            variable.SetSilent(v);
            return variable;
        }
    }

    /// <summary>Synced string (length-prefixed UTF-8 on the wire).</summary>
    public sealed class CYGStringVar : CYGSyncVarBase<string>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteString(Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            string oldValue = Current;
            string newValue = reader.ReadString();
            Write(newValue);
            ClearDirty();
            if (oldValue != newValue) RaiseChanged(oldValue, newValue);
        }

        /// <summary>Reads the value as its base type.</summary>
        public static implicit operator string(CYGStringVar v) { return v.Value; }

        /// <summary>Wraps a value as a clean (non-dirty) initial state.</summary>
        public static implicit operator CYGStringVar(string v)
        {
            CYGStringVar variable = new CYGStringVar();
            variable.SetSilent(v);
            return variable;
        }
    }

    /// <summary>Synced enum as a zigzag varint of its int value; converters cached for IL2CPP safety.</summary>
    public sealed class CYGEnumVar<T> : CYGSyncVarBase<T> where T : struct, IEquatable<T>
    {
        private readonly Func<T, int> _toInt;
        private readonly Func<int, T> _fromInt;

        /// <summary>Takes explicit int&lt;-&gt;enum converters (one per enum type, allocated once).</summary>
        public CYGEnumVar(Func<T, int> toInt, Func<int, T> fromInt, T initialValue = default)
        {
            _toInt = toInt;
            _fromInt = fromInt;
            SetSilent(initialValue);
        }

        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteInt32(_toInt(Current));
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            T oldValue = Current;
            T newValue = _fromInt(reader.ReadInt32());
            Write(newValue);
            ClearDirty();
            if (!oldValue.Equals(newValue)) RaiseChanged(oldValue, newValue);
        }
    }

    /// <summary>Synced Vector3 (3 floats; use TransformSync for poses, this is for arbitrary vectors).</summary>
    public sealed class CYGVector3Var : CYGSyncVarBase<Vector3>
    {
        /// <inheritdoc/>
        public override void PackDirty(CYGBitWriter writer)
        {
            writer.WriteVector3(Current);
            ClearDirty();
        }

        /// <inheritdoc/>
        public override void ApplyRemoteAndClear(CYGBitReader reader)
        {
            Vector3 oldValue = Current;
            Vector3 newValue = reader.ReadVector3();
            Write(newValue);
            ClearDirty();
            if (!oldValue.Equals(newValue)) RaiseChanged(oldValue, newValue);
        }

        /// <summary>Reads the value as its base type.</summary>
        public static implicit operator Vector3(CYGVector3Var v) { return v.Value; }

        /// <summary>Wraps a value as a clean (non-dirty) initial state.</summary>
        public static implicit operator CYGVector3Var(Vector3 v)
        {
            CYGVector3Var variable = new CYGVector3Var();
            variable.SetSilent(v);
            return variable;
        }
    }
}
