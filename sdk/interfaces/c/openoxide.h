#pragma once

// =============================================================================
// OpenOxide Game Server — Pure C Room SDK (WASM)
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
 * Standard OpenOxide Lifecycle Functions for Pure C
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

#ifdef __cplusplus
}
#endif
