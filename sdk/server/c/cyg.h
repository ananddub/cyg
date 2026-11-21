#pragma once

// =============================================================================
// CYG Game Server — Pure C Room SDK (WASM)
// Pure Procedural C — No classes, zero overhead, normal C functions!
// Target Compilers: clang --target=wasm32 / wasi-sdk / zig cc
// =============================================================================

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#if defined(__EMSCRIPTEN__)
#include <emscripten.h>
#define CYG_EXPORT EMSCRIPTEN_KEEPALIVE
#elif defined(__wasm__) || defined(__wasm32__) || defined(__GNUC__) || defined(__clang__)
#define CYG_EXPORT __attribute__((visibility("default")))
#else
#define CYG_EXPORT
#endif

#ifndef OX_EXPORT
#define OX_EXPORT CYG_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Pack linear memory pointer and length into a 64-bit integer return value.
 * High 32 bits = byte pointer, Low 32 bits = byte length.
 */
static inline int64_t cyg_pack_delta(const void* ptr, uint32_t len) {
    return ((int64_t)(uintptr_t)ptr << 32) | ((int64_t)len & 0xFFFFFFFFLL);
}

static inline int64_t ox_pack_delta(const void* ptr, uint32_t len) {
    return cyg_pack_delta(ptr, len);
}

/**
 * Host Environment Functions (Imports provided by CYG Host)
 */
__attribute__((import_module("cyg"), import_name("host_log")))
int32_t host_log(const char* ptr, int32_t len);

__attribute__((import_module("cyg"), import_name("host_send")))
int32_t host_send(int64_t target, const char* type_ptr, int32_t type_len, const void* data_ptr, int32_t data_len);

__attribute__((import_module("cyg"), import_name("host_kick")))
int32_t host_kick(int64_t player_id);

static inline void cyg_log(const char* msg, int32_t len) {
    if (msg && len > 0) host_log(msg, len);
}

static inline void cyg_send(int64_t target, const char* event_type, int32_t event_type_len, const void* data, int32_t data_len) {
    host_send(target, event_type, event_type_len, data, data_len);
}

static inline void cyg_broadcast(const char* event_type, int32_t event_type_len, const void* data, int32_t data_len) {
    host_send(0, event_type, event_type_len, data, data_len);
}

static inline void cyg_kick(int64_t player_id) {
    host_kick(player_id);
}

/**
 * Standard CYG Lifecycle Functions for Pure C
 * Implement these normal functions directly in your .c game room!
 */

/// Called once when the room instance is allocated on the host.
CYG_EXPORT void on_create(int32_t max_clients);

/// Called when a new player joins with authoritative spawn coordinates.
CYG_EXPORT int64_t on_auth(int64_t id, int32_t opt_ptr, int32_t opt_len, int32_t ctx_ptr, int32_t ctx_len);
CYG_EXPORT void on_join(int64_t id, int32_t opt_ptr, int32_t opt_len, int32_t auth_ptr, int32_t auth_len);

/// Called when a player leaves or disconnects.
CYG_EXPORT void on_leave(int64_t id, int32_t code);

/// Called when an authoritative client input packet arrives.
CYG_EXPORT void on_input(int64_t id, int32_t ptr, int32_t len);

/// Fixed authoritative simulation tick (20 Hz - 128 Hz).
/// Return cyg_pack_delta(buffer_ptr, buffer_len) for outbound state broadcast.
CYG_EXPORT int64_t on_tick(double dt);

/// State persistence snapshot hook (optional).
CYG_EXPORT int64_t on_save(void);

/// State restoration hook from saved linear snapshot (optional).
CYG_EXPORT void on_load(int32_t ptr, int32_t len);

/// Guest memory allocator for inbound client network packets.
CYG_EXPORT int32_t on_alloc(int32_t len);

/// Room fullness check hook.
CYG_EXPORT bool on_is_full(int32_t count);

/**
 * Convenience Macro: Default Optional Hooks
 * Provides a self-contained bump allocator (no external malloc needed) and default no-op hooks.
 */
#define CYG_DEFAULT_HOOKS \
    static uint8_t __cyg_guest_heap[65536]; \
    static uint32_t __cyg_guest_heap_ptr = 0; \
    CYG_EXPORT int32_t on_alloc(int32_t len) { \
        len = (len + 7) & ~7; \
        if (__cyg_guest_heap_ptr + (uint32_t)len > sizeof(__cyg_guest_heap)) __cyg_guest_heap_ptr = 0; \
        void* p = &__cyg_guest_heap[__cyg_guest_heap_ptr]; \
        __cyg_guest_heap_ptr += (uint32_t)len; \
        return (int32_t)(uintptr_t)p; \
    } \
    CYG_EXPORT int64_t on_save(void) { \
        return 0; \
    } \
    CYG_EXPORT void on_load(int32_t ptr, int32_t len) { \
        (void)ptr; (void)len; \
    } \
    CYG_EXPORT bool on_is_full(int32_t count) { \
        (void)count; return false; \
    }

#ifndef OX_DEFAULT_HOOKS
#define OX_DEFAULT_HOOKS CYG_DEFAULT_HOOKS
#endif

/**
 * Universal Hardware KeyCodes (A-Z, 0-9, Arrows, F1-F12, Mouse Buttons)
 */
typedef enum cyg_key_code {
    CYG_KEY_NONE             = 0,
    // Mouse
    CYG_KEY_MOUSE_LEFT       = 1,
    CYG_KEY_MOUSE_MIDDLE     = 2,
    CYG_KEY_MOUSE_RIGHT      = 3,
    CYG_KEY_MOUSE_BACK       = 4,
    CYG_KEY_MOUSE_FORWARD    = 5,
    // Control & Whitespace
    CYG_KEY_BACKSPACE        = 8,
    CYG_KEY_TAB              = 9,
    CYG_KEY_ENTER            = 13,
    CYG_KEY_SHIFT            = 16,
    CYG_KEY_CONTROL          = 17,
    CYG_KEY_ALT              = 18,
    CYG_KEY_PAUSE            = 19,
    CYG_KEY_CAPS_LOCK        = 20,
    CYG_KEY_ESCAPE           = 27,
    CYG_KEY_SPACE            = 32,
    // Navigation
    CYG_KEY_PAGE_UP          = 33,
    CYG_KEY_PAGE_DOWN        = 34,
    CYG_KEY_END              = 35,
    CYG_KEY_HOME             = 36,
    CYG_KEY_INSERT           = 45,
    CYG_KEY_DELETE           = 46,
    CYG_KEY_PRINT_SCREEN     = 44,
    // Arrows
    CYG_KEY_ARROW_LEFT       = 37,
    CYG_KEY_ARROW_UP         = 38,
    CYG_KEY_ARROW_RIGHT      = 39,
    CYG_KEY_ARROW_DOWN       = 40,
    // Digits
    CYG_KEY_0                = 48,
    CYG_KEY_1                = 49,
    CYG_KEY_2                = 50,
    CYG_KEY_3                = 51,
    CYG_KEY_4                = 52,
    CYG_KEY_5                = 53,
    CYG_KEY_6                = 54,
    CYG_KEY_7                = 55,
    CYG_KEY_8                = 56,
    CYG_KEY_9                = 57,
    // Alphabet A-Z
    CYG_KEY_A                = 65,
    CYG_KEY_B                = 66,
    CYG_KEY_C                = 67,
    CYG_KEY_D                = 68,
    CYG_KEY_E                = 69,
    CYG_KEY_F                = 70,
    CYG_KEY_G                = 71,
    CYG_KEY_H                = 72,
    CYG_KEY_I                = 73,
    CYG_KEY_J                = 74,
    CYG_KEY_K                = 75,
    CYG_KEY_L                = 76,
    CYG_KEY_M                = 77,
    CYG_KEY_N                = 78,
    CYG_KEY_O                = 79,
    CYG_KEY_P                = 80,
    CYG_KEY_Q                = 81,
    CYG_KEY_R                = 82,
    CYG_KEY_S                = 83,
    CYG_KEY_T                = 84,
    CYG_KEY_U                = 85,
    CYG_KEY_V                = 86,
    CYG_KEY_W                = 87,
    CYG_KEY_X                = 88,
    CYG_KEY_Y                = 89,
    CYG_KEY_Z                = 90,
    // Meta
    CYG_KEY_META_LEFT        = 91,
    CYG_KEY_META_RIGHT       = 92,
    CYG_KEY_CONTEXT_MENU     = 93,
    // Numpad
    CYG_KEY_NUMPAD_0         = 96,
    CYG_KEY_NUMPAD_1         = 97,
    CYG_KEY_NUMPAD_2         = 98,
    CYG_KEY_NUMPAD_3         = 99,
    CYG_KEY_NUMPAD_4         = 100,
    CYG_KEY_NUMPAD_5         = 101,
    CYG_KEY_NUMPAD_6         = 102,
    CYG_KEY_NUMPAD_7         = 103,
    CYG_KEY_NUMPAD_8         = 104,
    CYG_KEY_NUMPAD_9         = 105,
    CYG_KEY_NUMPAD_MULTIPLY  = 106,
    CYG_KEY_NUMPAD_ADD       = 107,
    CYG_KEY_NUMPAD_SEPARATOR = 108,
    CYG_KEY_NUMPAD_SUBTRACT  = 109,
    CYG_KEY_NUMPAD_DECIMAL   = 110,
    CYG_KEY_NUMPAD_DIVIDE    = 111,
    // Function Keys
    CYG_KEY_F1               = 112,
    CYG_KEY_F2               = 113,
    CYG_KEY_F3               = 114,
    CYG_KEY_F4               = 115,
    CYG_KEY_F5               = 116,
    CYG_KEY_F6               = 117,
    CYG_KEY_F7               = 118,
    CYG_KEY_F8               = 119,
    CYG_KEY_F9               = 120,
    CYG_KEY_F10              = 121,
    CYG_KEY_F11              = 122,
    CYG_KEY_F12              = 123,
    // Modifiers Specific
    CYG_KEY_SHIFT_LEFT       = 160,
    CYG_KEY_SHIFT_RIGHT      = 161,
    CYG_KEY_CONTROL_LEFT     = 162,
    CYG_KEY_CONTROL_RIGHT    = 163,
    CYG_KEY_ALT_LEFT         = 164,
    CYG_KEY_ALT_RIGHT        = 165
} cyg_key_code;

typedef cyg_key_code ox_key_code;

#ifdef __cplusplus
}
#endif
