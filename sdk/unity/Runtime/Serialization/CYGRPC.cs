using System;
using System.Collections.Generic;
using System.Reflection;
using UnityEngine;

namespace CookYourGame
{
    /// <summary>Selects which clients execute an RPC.</summary>
    public enum CYGRPCTarget : byte
    {
        /// <summary>Every client in the room, including the sender (local echo).</summary>
        All = 0,
        /// <summary>Every client except the sender (never echoed locally).</summary>
        Others = 1,
        /// <summary>Server-side spectators only (never echoed locally).</summary>
        Observers = 2,
        /// <summary>Only the master client (echoed locally when we are master).</summary>
        MasterClient = 3,
        /// <summary>Only the view's owner (echoed locally when the view is ours).</summary>
        Owner = 4,
        /// <summary>Only the given player (echoed locally when targetPlayer is us).</summary>
        Target = 5,
    }

    /// <summary>Marks a void MonoBehaviour method as a network RPC.</summary>
    [AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
    public class CYGRPCAttribute : Attribute
    {
        /// <summary>Which clients execute this RPC.</summary>
        public CYGRPCTarget target;
        /// <summary>Reserved channel hint; the transport currently relays all RPCs on one channel.</summary>
        public int channel;

        /// <summary>Creates the attribute with the given target filter.</summary>
        public CYGRPCAttribute(CYGRPCTarget target = CYGRPCTarget.All)
        {
            this.target = target;
            this.channel = 0;
        }
    }

    /// <summary>Executes on the server/host (Client -> Server).</summary>
    [AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
    public sealed class CYGServerRpcAttribute : CYGRPCAttribute
    {
        public CYGServerRpcAttribute() : base(CYGRPCTarget.MasterClient) { }
    }

    /// <summary>Executes on all observing clients (Server -> Clients / Client -> Server -> Clients).</summary>
    [AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
    public sealed class CYGObserversRpcAttribute : CYGRPCAttribute
    {
        public CYGObserversRpcAttribute() : base(CYGRPCTarget.Observers) { }
    }

    /// <summary>Executes on a specific target client only (Server -> Target Client).</summary>
    [AttributeUsage(AttributeTargets.Method, AllowMultiple = false)]
    public sealed class CYGTargetRpcAttribute : CYGRPCAttribute
    {
        public CYGTargetRpcAttribute() : base(CYGRPCTarget.Target) { }
    }

    /// <summary>Routes CUSTOM_RPC frames to [CYGRPC] methods bound per CYGView.</summary>
    public static class CYGRpcRouter
    {
        // WHY: 4 raw big-endian bytes viewId + 2 raw big-endian bytes method hash + 1 target byte.
        const int HeaderLength = 7;

        sealed class RpcBinding
        {
            public ushort Hash;
            public Action<object, CYGBitReader> Invoker;
        }

        static readonly Dictionary<string, RpcBinding> bindingCache = new Dictionary<string, RpcBinding>();
        static readonly Dictionary<ushort, string> hashOwners = new Dictionary<ushort, string>();

        // WHY: keyed by the CYGView instance, not the view id: the core assigns network ids after
        // Instantiate returns, so a bind from Awake would otherwise register the transient id 0.
        static readonly Dictionary<CYGView, Dictionary<ushort, Action<object, CYGBitReader>>> dispatchTable =
            new Dictionary<CYGView, Dictionary<ushort, Action<object, CYGBitReader>>>();

        // WHY: non-readonly fields so a struct- or class-based writer mutates in place either way.
        static CYGBitWriter s_writer = new CYGBitWriter();
        static CYGBitReader s_reader = new CYGBitReader();

        [RuntimeInitializeOnLoadMethod(RuntimeInitializeLoadType.BeforeSceneLoad)]
        static void Boot()
        {
            bindingCache.Clear();
            hashOwners.Clear();
            dispatchTable.Clear();
            CYGNetwork.OnRpcReceived -= HandleRpcReceived;
            CYGNetwork.OnRpcReceived += HandleRpcReceived;
            CYGNetwork.OnConnectedEvent -= HandleConnected;
            CYGNetwork.OnConnectedEvent += HandleConnected;
        }

        /// <summary>No-op kept so the router stays subscribed across reconnects.</summary>
        static void HandleConnected()
        {
        }

        /// <summary>FNV-1a 16-bit hash of an ASCII method name.</summary>
        public static ushort Fnv1a(string name)
        {
            // WHY: FNV-1a 16-bit constants: offset basis 0x811C, prime 0x011D; masked after each multiply.
            uint hash = 0x811C;
            for (int i = 0; i < name.Length; i++)
            {
                hash ^= (byte)name[i];
                hash = (hash * 0x011D) & 0xFFFF;
            }

            return (ushort)hash;
        }

        /// <summary>Binds every [CYGRPC] method on the owner GameObject's components for its CYGView.</summary>
        public static void Bind(Component owner)
        {
            if (owner == null)
            {
                Debug.LogError("CYGRpcRouter.Bind: owner component is null.");
                return;
            }

            CYGView view = owner.GetComponent<CYGView>();
            if (view == null)
            {
                Debug.LogError($"CYGRpcRouter.Bind: no CYGView on GameObject '{owner.gameObject.name}'.", owner);
                return;
            }

            if (!dispatchTable.TryGetValue(view, out Dictionary<ushort, Action<object, CYGBitReader>> byHash))
            {
                byHash = new Dictionary<ushort, Action<object, CYGBitReader>>();
                dispatchTable[view] = byHash;
            }

            MonoBehaviour[] components = owner.GetComponents<MonoBehaviour>();
            for (int c = 0; c < components.Length; c++)
            {
                MonoBehaviour component = components[c];
                Type type = component.GetType();
                MethodInfo[] methods = type.GetMethods(BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic);
                for (int m = 0; m < methods.Length; m++)
                {
                    MethodInfo method = methods[m];
                    CYGRPCAttribute attribute = method.GetCustomAttribute<CYGRPCAttribute>();
                    if (attribute == null)
                    {
                        continue;
                    }

                    string key = type.FullName + "::" + method.Name;
                    if (!bindingCache.TryGetValue(key, out RpcBinding binding))
                    {
                        // WHY: rejected methods cache a null so their bind-time error logs exactly once.
                        binding = BuildBinding(key, method);
                        bindingCache[key] = binding;
                    }

                    if (binding == null)
                    {
                        continue;
                    }

                    MonoBehaviour captured = component;
                    byHash[binding.Hash] = (instance, reader) =>
                    {
                        if (captured != null)
                        {
                            binding.Invoker(captured, reader);
                        }
                    };
                }
            }
        }

        /// <summary>Removes every RPC registration for a destroyed view.</summary>
        public static void Unbind(CYGView view)
        {
            // WHY: reference check, not Unity's fake-null, because OnDestroy may run after the
            // view component is already destroyed and would otherwise leave the entry behind.
            if ((object)view != null)
            {
                dispatchTable.Remove(view);
            }
        }

        /// <summary>Sends an RPC, then runs it locally when the target includes this client.</summary>
        public static void Invoke(CYGView view, string methodName, CYGRPCTarget target = CYGRPCTarget.All,
            Action<CYGBitWriter> packArgs = null, CYGPlayer targetPlayer = null)
        {
            if (view == null)
            {
                Debug.LogError("CYGRpcRouter.Invoke: view is null.");
                return;
            }

            if (!CYGNetwork.IsConnected)
            {
                return;
            }

            ushort hash = Fnv1a(methodName);
            int viewId = view.ViewId;

            s_writer.Reset();
            WriteUInt16Raw(s_writer, (ushort)(viewId >> 16));
            WriteUInt16Raw(s_writer, (ushort)(viewId & 0xFFFF));
            WriteUInt16Raw(s_writer, hash);
            byte wireTarget = (byte)target;
            if (target == CYGRPCTarget.All)
            {
                wireTarget = 2; // Maps to server's RPC_TARGET_ALL (2) so it is multicasted to peers instead of swallowed as SERVER (0)
            }
            s_writer.WriteByte(wireTarget);
            if (packArgs != null)
            {
                packArgs(s_writer);
            }
            else
            {
                s_writer.WriteBool(true);
            }

            ArraySegment<byte> payload = s_writer.ToArraySegment();

            // WHY: send first, echo second - the payload shares the writer's internal buffer, and a
            // nested Send from the locally echoed handler would clobber it before transmission.
            if (!CYGNetwork.SendRpcPayload(payload))
            {
                return;
            }

            if (ShouldEchoLocally(view, target, targetPlayer))
            {
                if (dispatchTable.TryGetValue(view, out Dictionary<ushort, Action<object, CYGBitReader>> byHash) &&
                    byHash.TryGetValue(hash, out Action<object, CYGBitReader> invoker))
                {
                    s_reader.Attach(payload.Array, payload.Offset + HeaderLength, payload.Count - HeaderLength);
                    invoker(null, s_reader);
                }
            }
        }

        /// <summary>Alias for Invoke matching the CYGRpc.Send naming.</summary>
        public static void Send(CYGView view, string methodName, CYGRPCTarget target = CYGRPCTarget.All,
            Action<CYGBitWriter> packArgs = null, CYGPlayer targetPlayer = null)
        {
            Invoke(view, methodName, target, packArgs, targetPlayer);
        }

        static bool ShouldEchoLocally(CYGView view, CYGRPCTarget target, CYGPlayer targetPlayer)
        {
            switch (target)
            {
                case CYGRPCTarget.All:
                    return true;
                case CYGRPCTarget.Owner:
                    return view.IsMine;
                case CYGRPCTarget.MasterClient:
                    return CYGNetwork.IsMasterClient;
                case CYGRPCTarget.Target:
                    return targetPlayer != null && targetPlayer.IsLocal;
                default:
                    return false;
            }
        }

        static void HandleRpcReceived(ArraySegment<byte> payload)
        {
            if (payload.Count < HeaderLength || payload.Array == null)
            {
                return;
            }

            byte[] buffer = payload.Array;
            int offset = payload.Offset;
            int viewId = (buffer[offset] << 24) | (buffer[offset + 1] << 16) | (buffer[offset + 2] << 8) | buffer[offset + 3];
            ushort hash = (ushort)((buffer[offset + 4] << 8) | buffer[offset + 5]);
            CYGRPCTarget target = (CYGRPCTarget)buffer[offset + 6];

            if (!CYGViewTable.TryGet(viewId, out CYGView view) || view == null)
            {
                return;
            }

            if (!dispatchTable.TryGetValue(view, out Dictionary<ushort, Action<object, CYGBitReader>> byHash))
            {
                return;
            }

            if (!byHash.TryGetValue(hash, out Action<object, CYGBitReader> invoker))
            {
                return;
            }

            if (target == CYGRPCTarget.Owner && !view.IsMine)
            {
                return;
            }

            if (target == CYGRPCTarget.MasterClient && !CYGNetwork.IsMasterClient)
            {
                return;
            }

            s_reader.Attach(buffer, offset + HeaderLength, payload.Count - HeaderLength);
            invoker(null, s_reader);
        }

        static RpcBinding BuildBinding(string key, MethodInfo method)
        {
            if (method.IsStatic || method.IsGenericMethod)
            {
                Debug.LogError($"CYGRpcRouter: [CYGRPC] '{key}' must be a non-generic instance method; skipped.");
                return null;
            }

            if (method.ReturnType != typeof(void))
            {
                Debug.LogError($"CYGRpcRouter: [CYGRPC] '{key}' must return void; skipped.");
                return null;
            }

            ParameterInfo[] parameters = method.GetParameters();
            Func<CYGBitReader, object>[] unboxers = new Func<CYGBitReader, object>[parameters.Length];
            for (int i = 0; i < parameters.Length; i++)
            {
                unboxers[i] = CreateUnboxer(parameters[i].ParameterType);
                if (unboxers[i] == null)
                {
                    Debug.LogError($"CYGRpcRouter: [CYGRPC] '{key}' has unsupported parameter '{parameters[i].Name}' of type {parameters[i].ParameterType.Name}; skipped.");
                    return null;
                }
            }

            object[] boxedArgs = new object[parameters.Length];
            RpcBinding binding = new RpcBinding
            {
                Hash = Fnv1a(method.Name),
            };

            // WHY: dispatch keys on the 16-bit hash only, so two differently named RPCs sharing a hash
            // would silently overwrite each other on the same view; warn instead of failing quietly.
            if (hashOwners.TryGetValue(binding.Hash, out string ownerName) && ownerName != method.Name)
            {
                Debug.LogWarning($"CYGRpcRouter: hash collision between RPC '{ownerName}' and '{key}' (0x{binding.Hash:X4}); rename one method.");
            }
            else
            {
                hashOwners[binding.Hash] = method.Name;
            }


            binding.Invoker = (instance, reader) =>
            {
                // WHY: boxedArgs is reused per binding; RPC dispatch runs only on the main thread and
                // Unity does not re-enter the same handler, so no concurrent overwrite can occur.
                for (int i = 0; i < unboxers.Length; i++)
                {
                    boxedArgs[i] = unboxers[i](reader);
                }

                method.Invoke(instance, boxedArgs);
            };

            return binding;
        }

        static Func<CYGBitReader, object> CreateUnboxer(Type parameterType)
        {
            if (parameterType == typeof(byte))
            {
                return reader => reader.ReadByte();
            }

            if (parameterType == typeof(bool))
            {
                return reader => reader.ReadBool();
            }

            if (parameterType == typeof(int))
            {
                return reader => reader.ReadInt32();
            }

            if (parameterType == typeof(uint))
            {
                return reader => reader.ReadUInt32();
            }

            if (parameterType == typeof(float))
            {
                return reader => reader.ReadSingle();
            }

            if (parameterType == typeof(string))
            {
                return reader => reader.ReadString();
            }

            if (parameterType == typeof(Vector3))
            {
                return reader => reader.ReadVector3();
            }

            if (parameterType == typeof(Quaternion))
            {
                return reader => reader.ReadQuaternion();
            }

            if (parameterType.IsEnum)
            {
                Type enumType = parameterType;
                return reader => Enum.ToObject(enumType, reader.ReadInt32());
            }

            return null;
        }

        static void WriteUInt16Raw(CYGBitWriter writer, ushort value)
        {
            writer.WriteByte((byte)(value >> 8));
            writer.WriteByte((byte)(value & 0xFF));
        }
    }

    /// <summary>Add to prefabs with [CYGRPC] methods to bind them automatically in Awake.</summary>
    public sealed class CYGRpcAutoBind : MonoBehaviour
    {
        CYGView view;

        void Awake()
        {
            CYGRpcRouter.Bind(this);
            view = GetComponent<CYGView>();
        }

        void OnDestroy()
        {
            CYGRpcRouter.Unbind(view);
        }
    }

    /// <summary>Convenience facade over CYGRpcRouter.</summary>
    public static class CYGRpc
    {
        /// <summary>Sends an RPC, then runs it locally when the target includes this client.</summary>
        public static void Send(CYGView view, string methodName, CYGRPCTarget target = CYGRPCTarget.All,
            Action<CYGBitWriter> packArgs = null, CYGPlayer targetPlayer = null)
        {
            CYGRpcRouter.Send(view, methodName, target, packArgs, targetPlayer);
        }

        /// <summary>Sends an RPC to the server/host (Client -> Server).</summary>
        public static void ServerRpc(CYGView view, string methodName, Action<CYGBitWriter> packArgs = null)
        {
            CYGRpcRouter.Invoke(view, methodName, CYGRPCTarget.MasterClient, packArgs, null);
        }

        /// <summary>Sends an RPC to all observers (Server -> Clients / Client -> Server -> Clients).</summary>
        public static void ObserversRpc(CYGView view, string methodName, Action<CYGBitWriter> packArgs = null)
        {
            CYGRpcRouter.Invoke(view, methodName, CYGRPCTarget.Observers, packArgs, null);
        }

        /// <summary>Sends an RPC to a specific target client (Server -> Target Client).</summary>
        public static void TargetRpc(CYGView view, string methodName, CYGPlayer targetPlayer, Action<CYGBitWriter> packArgs = null)
        {
            CYGRpcRouter.Invoke(view, methodName, CYGRPCTarget.Target, packArgs, targetPlayer);
        }

        /// <summary>Alias for Send.</summary>
        public static void Invoke(CYGView view, string methodName, CYGRPCTarget target = CYGRPCTarget.All,
            Action<CYGBitWriter> packArgs = null, CYGPlayer targetPlayer = null)
        {
            CYGRpcRouter.Invoke(view, methodName, target, packArgs, targetPlayer);
        }
    }
}
