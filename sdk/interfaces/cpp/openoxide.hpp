#pragma once

// =============================================================================
// OpenOxide Game Server — Guest Room SDK for C / C++ (WASM)
// WebAssembly Target: wasm32-wasi / wasm32-unknown-unknown (clang / emscripten)
// =============================================================================

#if __has_include(<cstdint>)
#include <cstdint>
#else
#include <stdint.h>
#endif
#if __has_include(<cstddef>)
#include <cstddef>
#else
#include <stddef.h>
#endif
#if __has_include(<cstdlib>)
#include <cstdlib>
#endif

namespace openoxide {

/**
 * Authoritative Game Room Base Interface
 * Inherit from this class and override lifecycle methods to implement your room.
 */
class Room {
public:
    virtual ~Room() = default;

    /// Called once when the room instance is allocated on the host.
    virtual void onCreate(int32_t maxClients) = 0;

    /// Called when a new player connects to the room.
    virtual void onJoin(int64_t id, double spawnX, double spawnZ) = 0;

    /// Called when a player disconnects or leaves.
    virtual void onLeave(int64_t id) = 0;

    /// Called when a client sends an input packet (WebSocket / UDP / WebRTC DataChannel).
    virtual void onInput(int64_t id, const uint8_t* data, size_t len) = 0;

    /// Fixed simulation tick (20 Hz - 128 Hz).
    /// Must return packed (ptr << 32) | len of the serialized delta buffer.
    virtual uint64_t onTick(double dt) = 0;

    /// State persistence snapshot hook (optional).
    virtual uint64_t onSave() {
        return 0;
    }

    /// State restoration hook from saved linear snapshot (optional).
    virtual void onLoad(const uint8_t* data, size_t len) {
        (void)data;
        (void)len;
    }

    /// Guest memory allocator for inbound client packets.
    virtual void* onAlloc(int32_t len) {
        (void)len;
        return nullptr;
    }

    /// Room capacity check hook.
    virtual bool onIsFull(int32_t count) {
        (void)count;
        return false;
    }
};

/// Helper to pack linear memory pointer and length into i64 ABI result.
inline uint64_t pack_delta(const void* ptr, uint32_t len) {
    return (static_cast<uint64_t>(reinterpret_cast<uintptr_t>(ptr)) << 32) | static_cast<uint64_t>(len);
}

} // namespace openoxide

/**
 * OPENOXIDE_ROOM Macro
 * Instantiates the room singleton and automatically exports the WebAssembly C-ABI bridge.
 * Developers don't need to write any extern "C" functions manually!
 */
#define OPENOXIDE_ROOM(RoomClass) \
    static RoomClass* g_openoxide_room = nullptr; \
    extern "C" { \
        __attribute__((visibility("default"))) void on_create(int32_t max) { \
            static RoomClass s_room_instance; \
            g_openoxide_room = &s_room_instance; \
            g_openoxide_room->onCreate(max); \
        } \
        __attribute__((visibility("default"))) void on_join(int64_t id, double x, double z) { \
            if (g_openoxide_room) g_openoxide_room->onJoin(id, x, z); \
        } \
        __attribute__((visibility("default"))) void on_leave(int64_t id) { \
            if (g_openoxide_room) g_openoxide_room->onLeave(id); \
        } \
        __attribute__((visibility("default"))) void on_input(int64_t id, int32_t ptr, int32_t len) { \
            if (g_openoxide_room) { \
                g_openoxide_room->onInput(id, reinterpret_cast<const uint8_t*>(ptr), static_cast<size_t>(len)); \
            } \
        } \
        __attribute__((visibility("default"))) int64_t on_tick(double dt) { \
            return g_openoxide_room ? static_cast<int64_t>(g_openoxide_room->onTick(dt)) : 0; \
        } \
        __attribute__((visibility("default"))) int64_t on_save() { \
            return g_openoxide_room ? static_cast<int64_t>(g_openoxide_room->onSave()) : 0; \
        } \
        __attribute__((visibility("default"))) void on_load(int32_t ptr, int32_t len) { \
            if (g_openoxide_room) { \
                g_openoxide_room->onLoad(reinterpret_cast<const uint8_t*>(ptr), static_cast<size_t>(len)); \
            } \
        } \
        __attribute__((visibility("default"))) int32_t on_alloc(int32_t len) { \
            return g_openoxide_room ? static_cast<int32_t>(reinterpret_cast<uintptr_t>(g_openoxide_room->onAlloc(len))) : 0; \
        } \
        __attribute__((visibility("default"))) bool on_is_full(int32_t count) { \
            return g_openoxide_room ? g_openoxide_room->onIsFull(count) : false; \
        } \
    }
