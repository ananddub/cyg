#pragma once

// =============================================================================
// CYG Game Server — Guest Room SDK for C / C++ (WASM)
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

namespace cyg {

/**
 * Universal Hardware KeyCodes (A-Z, 0-9, Arrows, F1-F12, Mouse Buttons)
 */
enum class KeyCode : uint16_t {
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
    // Modifiers Specific
    ShiftLeft = 160, ShiftRight = 161, ControlLeft = 162, ControlRight = 163,
    AltLeft = 164, AltRight = 165
};

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

} // namespace cyg

/**
 * CYG_ROOM Macro
 * Instantiates the room singleton and automatically exports the WebAssembly C-ABI bridge.
 * Developers don't need to write any extern "C" functions manually!
 */
#define CYG_ROOM(RoomClass) \
    static RoomClass* g_cyg_room = nullptr; \
    extern "C" { \
        __attribute__((visibility("default"))) void on_create(int32_t max) { \
            static RoomClass s_room_instance; \
            g_cyg_room = &s_room_instance; \
            g_cyg_room->onCreate(max); \
        } \
        __attribute__((visibility("default"))) void on_join(int64_t id, double x, double z) { \
            if (g_cyg_room) g_cyg_room->onJoin(id, x, z); \
        } \
        __attribute__((visibility("default"))) void on_leave(int64_t id) { \
            if (g_cyg_room) g_cyg_room->onLeave(id); \
        } \
        __attribute__((visibility("default"))) void on_input(int64_t id, int32_t ptr, int32_t len) { \
            if (g_cyg_room) { \
                g_cyg_room->onInput(id, reinterpret_cast<const uint8_t*>(ptr), static_cast<size_t>(len)); \
            } \
        } \
        __attribute__((visibility("default"))) int64_t on_tick(double dt) { \
            return g_cyg_room ? static_cast<int64_t>(g_cyg_room->onTick(dt)) : 0; \
        } \
        __attribute__((visibility("default"))) int64_t on_save() { \
            return g_cyg_room ? static_cast<int64_t>(g_cyg_room->onSave()) : 0; \
        } \
        __attribute__((visibility("default"))) void on_load(int32_t ptr, int32_t len) { \
            if (g_cyg_room) { \
                g_cyg_room->onLoad(reinterpret_cast<const uint8_t*>(ptr), static_cast<size_t>(len)); \
            } \
        } \
        __attribute__((visibility("default"))) int32_t on_alloc(int32_t len) { \
            return g_cyg_room ? static_cast<int32_t>(reinterpret_cast<uintptr_t>(g_cyg_room->onAlloc(len))) : 0; \
        } \
        __attribute__((visibility("default"))) bool on_is_full(int32_t count) { \
            return g_cyg_room ? g_cyg_room->onIsFull(count) : false; \
        } \
    }
