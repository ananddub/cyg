// =============================================================================
// OpenOxide Game Server — Guest Room SDK for C# (.NET 8 NativeAOT / wasi-wasm)
// =============================================================================

using System;
using System.Runtime.InteropServices;

namespace OpenOxide
{
    /// <summary>
    /// Base class for an authoritative OpenOxide game room in C#.
    /// </summary>
    public abstract class Room
    {
        public int MaxClients { get; set; } = 16;

        /// <summary>Called when the room is initialized.</summary>
        public abstract void OnCreate(int maxClients);

        /// <summary>Called when a player joins.</summary>
        public abstract void OnJoin(long id, double spawnX, double spawnZ);

        /// <summary>Called when a player disconnects.</summary>
        public abstract void OnLeave(long id);

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
    }

    /// <summary>
    /// Auto-generated flat C-ABI export bridge for WebAssembly runtimes.
    /// </summary>
    public static class OpenOxideExports
    {
        public static Room ActiveRoom { get; set; }

        [UnmanagedCallersOnly(EntryPoint = "on_create")]
        public static void OnCreate(int max) => ActiveRoom?.OnCreate(max);

        [UnmanagedCallersOnly(EntryPoint = "on_join")]
        public static void OnJoin(long id, double x, double z) => ActiveRoom?.OnJoin(id, x, z);

        [UnmanagedCallersOnly(EntryPoint = "on_leave")]
        public static void OnLeave(long id) => ActiveRoom?.OnLeave(id);

        [UnmanagedCallersOnly(EntryPoint = "on_input")]
        public static unsafe void OnInput(long id, int ptr, int len)
        {
            if (ActiveRoom != null && ptr != 0 && len > 0)
            {
                var span = new ReadOnlySpan<byte>((void*)ptr, len);
                ActiveRoom.OnInput(id, span);
            }
        }

        [UnmanagedCallersOnly(EntryPoint = "on_tick")]
        public static long OnTick(double dt) => ActiveRoom?.OnTick(dt) ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_save")]
        public static long OnSave() => ActiveRoom?.OnSave() ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_load")]
        public static unsafe void OnLoad(int ptr, int len)
        {
            if (ActiveRoom != null && ptr != 0 && len > 0)
            {
                var span = new ReadOnlySpan<byte>((void*)ptr, len);
                ActiveRoom.OnLoad(span);
            }
        }

        [UnmanagedCallersOnly(EntryPoint = "on_alloc")]
        public static int OnAlloc(int len) => ActiveRoom?.OnAlloc(len) ?? 0;

        [UnmanagedCallersOnly(EntryPoint = "on_is_full")]
        public static bool OnIsFull(int count) => ActiveRoom?.OnIsFull(count) ?? false;
    }
}
