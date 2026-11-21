// =============================================================================
// CYG Game Server — Guest Room SDK for C# (.NET 8/9 wasi-wasm)
// =============================================================================

using System;
using System.Runtime.InteropServices;
using System.Text;

namespace CYG
{
    /// <summary>
    /// Base class for an authoritative CYG game room in C#.
    /// </summary>
    public abstract class Room
    {
        public int MaxClients { get; set; } = 16;

        /// <summary>Called when the room is initialized.</summary>
        public abstract void OnCreate(int maxClients);

        /// <summary>Called when a player joins.</summary>
        public virtual string OnAuth(long id, string options, string context) { return "{}"; }
        public abstract void OnJoin(long id, string options, string auth);

        /// <summary>Called when a player disconnects.</summary>
        public abstract void OnLeave(long id, int code);

        /// <summary>Called on inbound client input packet.</summary>
        public abstract void OnInput(long id, ReadOnlySpan<byte> data);

        /// <summary>
        /// Fixed simulation tick (20 Hz - 128 Hz).
        /// Return PackDelta(ptr, len) containing the state delta buffer.
        /// </summary>
        public abstract long OnTick(double dt);

        /// <summary>State persistence snapshot hook (optional).</summary>
        public virtual long OnSave() => 0;

        /// <summary>State restoration hook (optional).</summary>
        public virtual void OnLoad(ReadOnlySpan<byte> data) {}

        /// <summary>Memory allocation hook for inbound payloads.</summary>
        public virtual int OnAlloc(int len) => (int)Marshal.AllocHGlobal(len);

        /// <summary>Room fullness check.</summary>
        public virtual bool OnIsFull(int count) => count >= MaxClients;

        /// <summary>Packs linear memory address and length into 64-bit return.</summary>
        public static long PackDelta(IntPtr ptr, int len)
        {
            return ((long)ptr.ToInt64() << 32) | ((long)len & 0xFFFFFFFFL);
        }

        /// <summary>Logs a message to the CYG server host logger.</summary>
        public static void Log(string msg)
        {
            if (string.IsNullOrEmpty(msg)) return;
            var bytes = Encoding.UTF8.GetBytes(msg);
            GCHandle pin = GCHandle.Alloc(bytes, GCHandleType.Pinned);
            try {
                CYGHost.host_log(pin.AddrOfPinnedObject(), bytes.Length);
            } finally {
                pin.Free();
            }
        }

        /// <summary>Broadcasts a typed event message to all players in the room.</summary>
        public static void Broadcast(string eventType, byte[] payload)
        {
            SendTo(0, eventType, payload);
        }

        /// <summary>Sends a typed event message to a specific player in the room.</summary>
        public static void SendTo(long target, string eventType, byte[] payload)
        {
            var typeBytes = Encoding.UTF8.GetBytes(eventType ?? "");
            payload ??= Array.Empty<byte>();

            GCHandle pinType = GCHandle.Alloc(typeBytes, GCHandleType.Pinned);
            GCHandle pinData = GCHandle.Alloc(payload, GCHandleType.Pinned);
            try {
                CYGHost.host_send(target, pinType.AddrOfPinnedObject(), typeBytes.Length, pinData.AddrOfPinnedObject(), payload.Length);
            } finally {
                pinType.Free();
                pinData.Free();
            }
        }

        /// <summary>Disconnects and kicks a player from the room.</summary>
        public static void Kick(long playerId)
        {
            CYGHost.host_kick(playerId);
        }
    }

    internal static class CYGHost
    {
        [DllImport("cyg", EntryPoint = "host_log")]
        internal static extern int host_log(IntPtr ptr, int len);

        [DllImport("cyg", EntryPoint = "host_send")]
        internal static extern int host_send(long target, IntPtr typePtr, int typeLen, IntPtr dataPtr, int dataLen);

        [DllImport("cyg", EntryPoint = "host_kick")]
        internal static extern int host_kick(long playerId);
    }

    /// <summary>
    /// Auto-generated flat C-ABI export bridge for WebAssembly runtimes.
    /// </summary>
    public static class CYGExports
    {
        public static Room ActiveRoom { get; set; }

        [UnmanagedCallersOnly(EntryPoint = "on_create")]
        public static void OnCreate(int max) => ActiveRoom?.OnCreate(max);

        [UnmanagedCallersOnly(EntryPoint = "on_auth")]
        public static long OnAuth(long id, IntPtr optPtr, int optLen, IntPtr ctxPtr, int ctxLen)
        {
            return 0; // C# guests reject nothing by default; auth data flows via OnJoin
        }

        [UnmanagedCallersOnly(EntryPoint = "on_join")]
        public static void OnJoin(long id, IntPtr optPtr, int optLen, IntPtr authPtr, int authLen)
        {
            string options = ReadString(optPtr, optLen);
            string auth = ReadString(authPtr, authLen);
            ActiveRoom?.OnJoin(id, options, auth);
        }

        private static string ReadString(IntPtr ptr, int len)
        {
            if (ptr == IntPtr.Zero || len <= 0) return "";
            var bytes = new byte[len];
            Marshal.Copy(ptr, bytes, 0, len);
            return Encoding.UTF8.GetString(bytes);
        }

        [UnmanagedCallersOnly(EntryPoint = "on_leave")]
        public static void OnLeave(long id, int code) => ActiveRoom?.OnLeave(id, code);

        [UnmanagedCallersOnly(EntryPoint = "on_input")]
        public static void OnInput(long id, int ptr, int len)
        {
            if (ActiveRoom != null && ptr != 0 && len > 0)
            {
                var bytes = new byte[len];
                Marshal.Copy((IntPtr)ptr, bytes, 0, len);
                ActiveRoom.OnInput(id, bytes);
            }
        }

        [UnmanagedCallersOnly(EntryPoint = "on_tick")]
        public static long OnTick(double dt) => ActiveRoom?.OnTick(dt) ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_save")]
        public static long OnSave() => ActiveRoom?.OnSave() ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_load")]
        public static void OnLoad(int ptr, int len)
        {
            if (ActiveRoom != null && ptr != 0 && len > 0)
            {
                var bytes = new byte[len];
                Marshal.Copy((IntPtr)ptr, bytes, 0, len);
                ActiveRoom.OnLoad(bytes);
            }
        }

        [UnmanagedCallersOnly(EntryPoint = "on_alloc")]
        public static int OnAlloc(int len) => ActiveRoom?.OnAlloc(len) ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_is_full")]
        public static bool OnIsFull(int count) => ActiveRoom?.OnIsFull(count) ?? false;
    }

    /// <summary>
    /// Universal Hardware KeyCodes (A-Z, 0-9, Arrows, F1-F12, Mouse Buttons).
    /// </summary>
    public enum KeyCode : ushort
    {
        None = 0,
        // Mouse
        MouseLeft = 1, MouseMiddle = 2, MouseRight = 3, MouseBack = 4, MouseForward = 5,
        // Control & Whitespace
        Backspace = 8, Tab = 9, Enter = 13, Shift = 16, Control = 17, Alt = 18,
        Pause = 19, CapsLock = 20, Escape = 27, Space = 32,
        // Navigation
        PageUp = 33, PageDown = 34, End = 35, Home = 36, Insert = 45, Delete = 46, PrintScreen = 44,
        // Arrows
        ArrowLeft = 37, ArrowUp = 38, ArrowRight = 39, ArrowDown = 40,
        Left = 37, Up = 38, Right = 39, Down = 40,
        // Digits
        Digit0 = 48, Digit1 = 49, Digit2 = 50, Digit3 = 51, Digit4 = 52,
        Digit5 = 53, Digit6 = 54, Digit7 = 55, Digit8 = 56, Digit9 = 57,
        Num0 = 48, Num1 = 49, Num2 = 50, Num3 = 51, Num4 = 52,
        Num5 = 53, Num6 = 54, Num7 = 55, Num8 = 56, Num9 = 57,
        // Alphabet A-Z
        KeyA = 65, KeyB = 66, KeyC = 67, KeyD = 68, KeyE = 69, KeyF = 70,
        KeyG = 71, KeyH = 72, KeyI = 73, KeyJ = 74, KeyK = 75, KeyL = 76,
        KeyM = 77, KeyN = 78, KeyO = 79, KeyP = 80, KeyQ = 81, KeyR = 82,
        KeyS = 83, KeyT = 84, KeyU = 85, KeyV = 86, KeyW = 87, KeyX = 88,
        KeyY = 89, KeyZ = 90,
        A = 65, B = 66, C = 67, D = 68, E = 69, F = 70,
        G = 71, H = 72, I = 73, J = 74, K = 75, L = 76,
        M = 77, N = 78, O = 79, P = 80, Q = 81, R = 82,
        S = 83, T = 84, U = 85, V = 86, W = 87, X = 88,
        Y = 89, Z = 90,
        // Meta
        MetaLeft = 91, MetaRight = 92, ContextMenu = 93,
        // Numpad
        Numpad0 = 96, Numpad1 = 97, Numpad2 = 98, Numpad3 = 99, Numpad4 = 100,
        Numpad5 = 101, Numpad6 = 102, Numpad7 = 103, Numpad8 = 104, Numpad9 = 105,
        NumpadMultiply = 106, NumpadAdd = 107, NumpadSeparator = 108,
        NumpadSubtract = 109, NumpadDecimal = 110, NumpadDivide = 111,
        // Function Keys
        F1 = 112, F2 = 113, F3 = 114, F4 = 115, F5 = 116, F6 = 117,
        F7 = 118, F8 = 119, F9 = 120, F10 = 121, F11 = 122, F12 = 123,
        // Modifiers
        ShiftLeft = 160, ShiftRight = 161, ControlLeft = 162, ControlRight = 163,
        AltLeft = 164, AltRight = 165
    }
}
