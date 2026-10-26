#pragma once

// =============================================================================
// CYG Game Server — Pure C Room SDK (WASM)
// Pure Procedural C — No classes, zero overhead, normal C functions!
// Target Compilers: clang --target=wasm32 / wasi-sdk / emscripten
// =============================================================================

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>
#if __has_include(<stdlib.h>)
#include <stdlib.h>
#endif

#if defined(__EMSCRIPTEN__)
#include <emscripten.h>
#define OX_EXPORT EMSCRIPTEN_KEEPALIVE
#elif defined(__wasm__) || defined(__wasm32__) || defined(__GNUC__) || defined(__clang__)
#define OX_EXPORT __attribute__((visibility("default")))
#else
#define OX_EXPORT
#endif

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Pack linear memory pointer and length into a 64-bit integer return value.
 * High 32 bits = byte pointer, Low 32 bits = byte length.
 */
static inline int64_t ox_pack_delta(const void* ptr, uint32_t len) {
    return ((int64_t)(uintptr_t)ptr << 32) | ((int64_t)len & 0xFFFFFFFFLL);
}

/**
 * Standard CYG Lifecycle Functions for Pure C
 * Implement these normal functions directly in your .c game room!
 */

/// Called once when the room instance is allocated on the host.
OX_EXPORT void on_create(int32_t max_clients);

/// Called when a new player joins with authoritative spawn coordinates.
OX_EXPORT void on_join(int64_t id, double spawn_x, double spawn_z);

/// Called when a player leaves or disconnects.
OX_EXPORT void on_leave(int64_t id);

/// Called when an authoritative client input packet arrives.
OX_EXPORT void on_input(int64_t id, int32_t ptr, int32_t len);

/// Fixed authoritative simulation tick (20 Hz - 128 Hz).
/// Return ox_pack_delta(buffer_ptr, buffer_len) for outbound state broadcast.
OX_EXPORT int64_t on_tick(double dt);

/// State persistence snapshot hook (optional).
OX_EXPORT int64_t on_save(void);

/// State restoration hook from saved linear snapshot (optional).
OX_EXPORT void on_load(int32_t ptr, int32_t len);

/// Guest memory allocator for inbound client network packets.
OX_EXPORT int32_t on_alloc(int32_t len);

/// Room fullness check hook.
OX_EXPORT bool on_is_full(int32_t count);

/**
 * Convenience Macro: Default Optional Hooks
 * If your game doesn't need custom persistence or allocators, put OX_DEFAULT_HOOKS
 * in your .c file and only write on_create, on_join, on_leave, on_input, on_tick!
 */
#define OX_DEFAULT_HOOKS \
    OX_EXPORT int32_t on_alloc(int32_t len) { \
        return (int32_t)(uintptr_t)malloc(len > 0 ? (size_t)len : 1); \
    } \
    OX_EXPORT int64_t on_save(void) { \
        return 0; \
    } \
    OX_EXPORT void on_load(int32_t ptr, int32_t len) { \
        (void)ptr; (void)len; \
    } \
    OX_EXPORT bool on_is_full(int32_t count) { \
        (void)count; return false; \
    }

/**
 * Universal Hardware KeyCodes (A-Z, 0-9, Arrows, F1-F12, Mouse Buttons)
 */
typedef enum ox_key_code {
    OX_KEY_NONE             = 0,
    // Mouse
    OX_KEY_MOUSE_LEFT       = 1,
    OX_KEY_MOUSE_MIDDLE     = 2,
    OX_KEY_MOUSE_RIGHT      = 3,
    OX_KEY_MOUSE_BACK       = 4,
    OX_KEY_MOUSE_FORWARD    = 5,
    // Control & Whitespace
    OX_KEY_BACKSPACE        = 8,
    OX_KEY_TAB              = 9,
    OX_KEY_ENTER            = 13,
    OX_KEY_SHIFT            = 16,
    OX_KEY_CONTROL          = 17,
    OX_KEY_ALT              = 18,
    OX_KEY_PAUSE            = 19,
    OX_KEY_CAPS_LOCK        = 20,
    OX_KEY_ESCAPE           = 27,
    OX_KEY_SPACE            = 32,
    // Navigation
    OX_KEY_PAGE_UP          = 33,
    OX_KEY_PAGE_DOWN        = 34,
    OX_KEY_END              = 35,
    OX_KEY_HOME             = 36,
    OX_KEY_INSERT           = 45,
    OX_KEY_DELETE           = 46,
    OX_KEY_PRINT_SCREEN     = 44,
    // Arrows
    OX_KEY_ARROW_LEFT       = 37,
    OX_KEY_ARROW_UP         = 38,
    OX_KEY_ARROW_RIGHT      = 39,
    OX_KEY_ARROW_DOWN       = 40,
    // Digits
    OX_KEY_0                = 48,
    OX_KEY_1                = 49,
    OX_KEY_2                = 50,
    OX_KEY_3                = 51,
    OX_KEY_4                = 52,
    OX_KEY_5                = 53,
    OX_KEY_6                = 54,
    OX_KEY_7                = 55,
    OX_KEY_8                = 56,
    OX_KEY_9                = 57,
    // Alphabet A-Z
    OX_KEY_A                = 65,
    OX_KEY_B                = 66,
    OX_KEY_C                = 67,
    OX_KEY_D                = 68,
    OX_KEY_E                = 69,
    OX_KEY_F                = 70,
    OX_KEY_G                = 71,
    OX_KEY_H                = 72,
    OX_KEY_I                = 73,
    OX_KEY_J                = 74,
    OX_KEY_K                = 75,
    OX_KEY_L                = 76,
    OX_KEY_M                = 77,
    OX_KEY_N                = 78,
    OX_KEY_O                = 79,
    OX_KEY_P                = 80,
    OX_KEY_Q                = 81,
    OX_KEY_R                = 82,
    OX_KEY_S                = 83,
    OX_KEY_T                = 84,
    OX_KEY_U                = 85,
    OX_KEY_V                = 86,
    OX_KEY_W                = 87,
    OX_KEY_X                = 88,
    OX_KEY_Y                = 89,
    OX_KEY_Z                = 90,
    // Meta
    OX_KEY_META_LEFT        = 91,
    OX_KEY_META_RIGHT       = 92,
    OX_KEY_CONTEXT_MENU     = 93,
    // Numpad
    OX_KEY_NUMPAD_0         = 96,
    OX_KEY_NUMPAD_1         = 97,
    OX_KEY_NUMPAD_2         = 98,
    OX_KEY_NUMPAD_3         = 99,
    OX_KEY_NUMPAD_4         = 100,
    OX_KEY_NUMPAD_5         = 101,
    OX_KEY_NUMPAD_6         = 102,
    OX_KEY_NUMPAD_7         = 103,
    OX_KEY_NUMPAD_8         = 104,
    OX_KEY_NUMPAD_9         = 105,
    OX_KEY_NUMPAD_MULTIPLY  = 106,
    OX_KEY_NUMPAD_ADD       = 107,
    OX_KEY_NUMPAD_SEPARATOR = 108,
    OX_KEY_NUMPAD_SUBTRACT  = 109,
    OX_KEY_NUMPAD_DECIMAL   = 110,
    OX_KEY_NUMPAD_DIVIDE    = 111,
    // Function Keys
    OX_KEY_F1               = 112,
    OX_KEY_F2               = 113,
    OX_KEY_F3               = 114,
    OX_KEY_F4               = 115,
    OX_KEY_F5               = 116,
    OX_KEY_F6               = 117,
    OX_KEY_F7               = 118,
    OX_KEY_F8               = 119,
    OX_KEY_F9               = 120,
    OX_KEY_F10              = 121,
    OX_KEY_F11              = 122,
    OX_KEY_F12              = 123,
    // Modifiers Specific
    OX_KEY_SHIFT_LEFT       = 160,
    OX_KEY_SHIFT_RIGHT      = 161,
    OX_KEY_CONTROL_LEFT     = 162,
    OX_KEY_CONTROL_RIGHT    = 163,
    OX_KEY_ALT_LEFT         = 164,
    OX_KEY_ALT_RIGHT        = 165
} ox_key_code;

#ifdef __cplusplus
}
#endif

