using System;
using System.Collections.Generic;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>
    /// Base class for networked behaviours on a CYGView. Provides ownership queries,
    /// spawn/despawn/owner-change hooks and SyncVar plumbing.
    /// SyncVar block wire format (written by Serialize when the behaviour owns registered vars):
    ///   [byte dirtyCount] then per dirty var: [byte varIndex] [value encoded per var type].
    /// Subclasses that override Serialize/Deserialize and also use SyncVars must call the base
    /// first and let the reader mirror that order.
    /// </summary>
    public abstract class CYGBehaviour : MonoBehaviour, ICYGObservable
    {
        private CYGView _view;
        private List<ICYGSyncVar> _syncVars;
        private bool _isObservable;

        /// <summary>The view this behaviour belongs to (found on first access).</summary>
        public CYGView View
        {
            get { return _view ?? (_view = GetComponentInParent<CYGView>(true)); }
        }

        /// <summary>True when the local client may write state for this behaviour's view.</summary>
        public bool HasAuthority
        {
            get { return View != null && View.HasAuthority; }
        }

        /// <summary>True when the local player owns this behaviour's view.</summary>
        public bool IsMine
        {
            get { return View != null && View.IsMine; }
        }

        /// <summary>Called after the view spawns (override point).</summary>
        protected virtual void OnSpawned() { }

        /// <summary>Called before the view despawns (override point).</summary>
        protected virtual void OnDespawned() { }

        /// <summary>Called when the view's owner changed (override point).</summary>
        protected virtual void OnOwnerChanged(CYGPlayer newOwner) { }

        internal void InvokeSpawned()
        {
            OnSpawned();
        }

        internal void InvokeDespawned()
        {
            OnDespawned();
        }

        internal void InvokeOwnerChanged(CYGPlayer newOwner)
        {
            OnOwnerChanged(newOwner);
        }

        /// <summary>Adds SyncVars to the delta pump, assigning wire indexes, and registers this behaviour as observed.</summary>
        protected void RegisterSyncVars(params ICYGSyncVar[] syncVars)
        {
            if (syncVars == null) return;
            for (int i = 0; i < syncVars.Length; i++)
            {
                ICYGSyncVar variable = syncVars[i];
                if (variable == null) continue;
                if (_syncVars == null) _syncVars = new List<ICYGSyncVar>(4);
                if (ContainsVar(variable)) continue;
                if (_syncVars.Count >= 255)
                {
                    Debug.LogWarning(GetType().Name + " on " + gameObject.name + ": exceeded 255 sync vars, ignored " + variable);
                    continue;
                }
                variable.Index = (byte)_syncVars.Count;
                _syncVars.Add(variable);
            }
            RegisterAsObservable();
        }

        private bool ContainsVar(ICYGSyncVar variable)
        {
            if (_syncVars == null) return false;
            for (int i = 0; i < _syncVars.Count; i++)
            {
                if (ReferenceEquals(_syncVars[i], variable)) return true;
            }
            return false;
        }

        /// <summary>Adds this behaviour to its view's observed components (idempotent).</summary>
        protected void RegisterAsObservable()
        {
            if (_isObservable) return;
            CYGView view = View;
            if (view == null)
            {
                Debug.LogWarning(GetType().Name + " on " + gameObject.name + " has no parent CYGView; state will not sync.");
                return;
            }
            view.AddObservable(this);
            _isObservable = true;
        }

        /// <summary>Removes this behaviour from its view's observed components.</summary>
        protected void UnregisterAsObservable()
        {
            if (!_isObservable) return;
            if (_view != null) _view.RemoveObservable(this);
            _isObservable = false;
        }

        /// <summary>Copies every dirty registered sync var into the list.</summary>
        public void CollectDirtySyncVars(List<ICYGSyncVar> into)
        {
            if (into == null || _syncVars == null) return;
            for (int i = 0; i < _syncVars.Count; i++)
            {
                if (_syncVars[i].IsDirty) into.Add(_syncVars[i]);
            }
        }

        /// <summary>True when at least one registered sync var is dirty.</summary>
        public bool HasDirtySyncVars()
        {
            if (_syncVars == null) return false;
            for (int i = 0; i < _syncVars.Count; i++)
            {
                if (_syncVars[i].IsDirty) return true;
            }
            return false;
        }

        /// <inheritdoc/>
        public virtual bool HasDelta()
        {
            return HasDirtySyncVars();
        }

        /// <inheritdoc/>
        public virtual void Serialize(CYGBitWriter writer)
        {
            if (_syncVars == null || _syncVars.Count == 0) return;
            byte dirtyCount = 0;
            for (int i = 0; i < _syncVars.Count; i++)
            {
                if (_syncVars[i].IsDirty) dirtyCount++;
            }
            writer.WriteByte(dirtyCount);
            for (int i = 0; i < _syncVars.Count; i++)
            {
                ICYGSyncVar variable = _syncVars[i];
                if (!variable.IsDirty) continue;
                writer.WriteByte(variable.Index);
                variable.PackDirty(writer);
            }
        }

        /// <inheritdoc/>
        public virtual void Deserialize(CYGBitReader reader)
        {
            if (_syncVars == null || _syncVars.Count == 0) return;
            ApplySyncVarDeltas(reader, reader.ReadByte());
        }

        /// <summary>Reads a [dirtyCount]-headed sync var block and applies each indexed value.</summary>
        internal void ApplySyncVarDeltas(CYGBitReader reader, byte count)
        {
            for (int i = 0; i < count; i++)
            {
                byte index = reader.ReadByte();
                ICYGSyncVar variable = FindVarByIndex(index);
                if (variable == null)
                {
                    // Cannot skip an unknown-length value: the rest of this payload is undecodable.
                    Debug.LogError(GetType().Name + " on " + gameObject.name + ": delta references sync var index " + index + " that is not registered");
                    return;
                }
                variable.ApplyRemoteAndClear(reader);
            }
        }

        private ICYGSyncVar FindVarByIndex(byte index)
        {
            if (_syncVars == null) return null;
            for (int i = 0; i < _syncVars.Count; i++)
            {
                if (_syncVars[i].Index == index) return _syncVars[i];
            }
            return null;
        }
    }
}
