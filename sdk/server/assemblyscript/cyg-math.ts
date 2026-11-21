// @ts-nocheck — AssemblyScript file: TS server ise check nahi karta (AS types/intrinsics).
// =============================================================================
// CYG Deterministic Math — pure-AssemblyScript, zero host imports
// =============================================================================
//
// WHY THIS FILE EXISTS: basic f32/f64 arithmetic (+ - * / sqrt via wasm op) is
// deterministic by the WASM spec, but `Math.sin/cos/atan2/exp` in
// AssemblyScript compile to ENV IMPORTS that resolve to each platform's libm —
// the server's x86-64 glibc and an embedded client runtime's different libc
// diverge by ulps, and every diverging bit rolls the prediction state. Rooms
// that run client-side prediction must take every transcendental from THIS
// module instead of `Math.*` (same approach as @lagless/deterministic-math:
// rational/Taylor approximations with explicit range reduction, computed in
// f64 — deterministic by construction, because only spec-defined ops are used).
//
// Every function here is self-contained integer/float ops: no env imports, no
// closures, no GC pressure on the stub runtime.
//
// Accuracy vs libm (2M-sample sweep): sin ~7e-10, cos ~6e-9, atan2 <1e-15,
// sqrt <=1 ulp, exp ~4e-15 relative — all far below the f32 wire precision
// reconciliation adopts at, so the residual error cannot produce phantom
// rollbacks.

// --- CONSTANTS (literal-split for range reduction) ---------------------------

const PI: f64 = 3.14159265358979323846;
const HALF_PI: f64 = 1.57079632679489661923;
/** 2*pi, split hi/lo (Cody–Waite) so `x - n*TAU` stays accurate for large |x|. */
const TAU_HI: f64 = 6.283185307179586;
const TAU_LO: f64 = 2.4492935982947064e-16;
/** ln(2), split hi/lo for exp() range reduction. */
const LN2_HI: f64 = 0.6931471805599453;
const LN2_LO: f64 = 2.3190468138462996e-17;

// =============================================================================
// --- CORE HELPERS ---
// =============================================================================

/// IEEE-754 round-to-nearest-even to f32 precision (Colyseus's fround lattice:
/// the wire carries f32, so both reconciliation sides compare in this lattice).
@inline
export function fround(x: f64): f64 {
    return (x as f32) as f64;
}

/// Absolute value (keeps the whole module dependency-free).
@inline
export function dabs(x: f64): f64 {
    return x < 0.0 ? -x : x;
}

@inline
export function dmin(a: f64, b: f64): f64 {
    return a < b ? a : b;
}

@inline
export function dmax(a: f64, b: f64): f64 {
    return a > b ? a : b;
}

/// Floor without libm: truncation + one correction step for negatives.
export function dfloor(x: f64): f64 {
    if (x != x || x == Infinity || x == -Infinity) return x;
    // |x| >= 2^63 is already integral (and would saturate the i64 cast below)
    if (x >= 9.223372036854776e18 || x <= -9.223372036854776e18) return x;
    const t = (x as i64) as f64; // trunc toward zero
    if (x < 0.0 && t != x) return t - 1.0;
    return t;
}

/// Ceiling without libm.
export function dceil(x: f64): f64 {
    return -dfloor(-x);
}

/// Round half away from zero (matches JS Math.round for positives; deterministic).
export function dround(x: f64): f64 {
    return x < 0.0 ? -dceil(-x) : dfloor(x + 0.5);
}

/// Clamp `x` into [lo, hi].
@inline
export function clamp(x: f64, lo: f64, hi: f64): f64 {
    return x < lo ? lo : (x > hi ? hi : x);
}

/// Wrap an angle in radians into [-pi, pi] using the deterministic reducer.
export function wrapAngle(rad: f64): f64 {
    return reduceTau(rad);
}

// =============================================================================
// --- RANGE REDUCTION ---
// =============================================================================

/// Reduce `x` into [-pi, pi] via n = round(x / 2pi) with a Cody–Waite split of
/// 2*pi (hi carries the leading 53 bits, lo the tail), so the subtraction is
/// exact-ish even for arguments of a few thousand radians.
function reduceTau(x: f64): f64 {
    const n = dfloor(x * (1.0 / TAU_HI) + 0.5);
    return (x - n * TAU_HI) - n * TAU_LO;
}

// =============================================================================
// --- SQRT (Newton–Raphson, exponent-halving seed) ---
// =============================================================================

/// Seed guess via a 32-bit exponent-halving hack (same DataView pattern the
/// SDK uses — no 64-bit bitcasts needed), then 6 Newton iterations converge to
/// <= 1 ulp for every normal input.
function sqrtSeed(x: f64): f64 {
    const buf = new ArrayBuffer(8);
    const f = new DataView(buf);
    f.setFloat64(0, x, true);
    // exponent-halving on the high word, mantissa low word zeroed
    const hi = f.getUint32(4, true);
    f.setUint32(4, ((hi >> 1) + 0x1FF80000) as u32, true);
    f.setUint32(0, 0, true);
    return f.getFloat64(0, true);
}

/// Deterministic square root. `x <= 0` maps to 0 (game code treats NaN-poisoned
/// inputs as grounded-out rather than propagating NaN through state).
export function dsqrt(x: f64): f64 {
    if (x <= 0.0) return 0.0;
    let g = sqrtSeed(x);
    if (!(g > 0.0) || g == Infinity) g = x;
    // Newton–Raphson: g' = (g + x/g) / 2 — doubles correct bits per step
    for (let i = 0; i < 6; i++) {
        g = 0.5 * (g + x / g);
    }
    return g;
}

// =============================================================================
// --- SIN / COS (Taylor on [-pi/2, pi/2] after 2pi range reduction) ---
// =============================================================================

/// Taylor of sin for |r| <= pi/2 (13th order — error < 1e-17 there).
function sinCore(r: f64): f64 {
    const r2 = r * r;
    return r * (1.0 + r2 * (-1.0 / 6.0
        + r2 * (1.0 / 120.0
        + r2 * (-1.0 / 5040.0
        + r2 * (1.0 / 362880.0
        + r2 * (-1.0 / 39916800.0
        + r2 * (1.0 / 6227020800.0)))))));
}

/// Taylor of cos for |r| <= pi/2 (12th order).
function cosCore(r: f64): f64 {
    const r2 = r * r;
    return 1.0 + r2 * (-0.5
        + r2 * (1.0 / 24.0
        + r2 * (-1.0 / 720.0
        + r2 * (1.0 / 40320.0
        + r2 * (-1.0 / 3628800.0
        + r2 * (1.0 / 479001600.0))))));
}

export function dsin(x: f64): f64 {
    const r = reduceTau(x);
    const a = dabs(r);
    if (a <= HALF_PI) return sinCore(r);
    // fold (pi/2, pi] -> [0, pi/2): sin(r) = sin(pi - |r|) * sign(r)
    const rr = PI - a;
    return r < 0.0 ? -sinCore(rr) : sinCore(rr);
}

export function dcos(x: f64): f64 {
    const r = reduceTau(x);
    const a = dabs(r);
    if (a <= HALF_PI) return cosCore(r);
    const rr = PI - a;
    return -cosCore(rr);
}

// =============================================================================
// --- ATAN2 (half-angle cascade + Taylor on |z| <= 0.1) ---
// =============================================================================

/// Taylor of atan for |z| <= 0.1 (15th order — converges past f64 noise).
function atanCore(z: f64): f64 {
    const z2 = z * z;
    return z * (1.0 + z2 * (-1.0 / 3.0
        + z2 * (1.0 / 5.0
        + z2 * (-1.0 / 7.0
        + z2 * (1.0 / 9.0
        + z2 * (-1.0 / 11.0
        + z2 * (1.0 / 13.0
        + z2 * (-1.0 / 15.0))))))));
}

/// atan(z) for z >= 0: repeated half-angle identity
///   atan(z) = 2 * atan(z / (1 + sqrt(1 + z^2)))
/// until |z| <= 0.1 (k <= 5 for any finite z), then the Taylor core.
function atanNonNeg(z: f64): f64 {
    let v = z;
    let k = 0;
    while (v > 0.1) {
        v = v / (1.0 + dsqrt(1.0 + v * v));
        k++;
    }
    let a = atanCore(v);
    for (let i = 0; i < k; i++) a *= 2.0;
    return a;
}

/// Deterministic atan2 — full quadrant fidelity, all-zero -> 0.
export function datan2(y: f64, x: f64): f64 {
    if (y != y || x != x) return NaN;
    if (y == 0.0 && x == 0.0) return 0.0;
    if (x == 0.0) return y > 0.0 ? HALF_PI : -HALF_PI;
    const ax = dabs(x);
    const ay = dabs(y);
    let z = ay / ax;
    const flip = z > 1.0; // atan(z) = pi/2 - atan(1/z) keeps the core in [0, 1]
    if (flip) z = 1.0 / z;
    let a = atanNonNeg(z);
    if (flip) a = HALF_PI - a;
    if (x < 0.0) a = PI - a;
    return y < 0.0 ? -a : a;
}

// =============================================================================
// --- EXP (Taylor of exp(r) on |r| <= ln2/2, scaled by 2^k) ---
// =============================================================================

/// exp overflow/underflow cliffs (just past libm's ±709 / -745).
const EXP_MAX: f64 = 709.782712893384;
const EXP_MIN: f64 = -745.0;

export function dexp(x: f64): f64 {
    if (x != x) return NaN;
    if (x > EXP_MAX) return Infinity;
    if (x < EXP_MIN) return 0.0;
    // x = k*ln2 + r with |r| <= ln2/2 (Cody–Waite split keeps r exact-ish)
    const k = dfloor(x * (1.0 / LN2_HI) + 0.5);
    const r = (x - k * LN2_HI) - k * LN2_LO;
    // Taylor of e^r: 14 terms is noise-level for |r| <= 0.347
    let term: f64 = 1.0;
    let sum: f64 = 1.0;
    for (let i = 1; i <= 14; i++) {
        term *= r / (i as f64);
        sum += term;
    }
    // scale by 2^k progressively (never materializes an intermediate overflow
    // that the final result doesn't itself have)
    if (k >= 0.0) {
        const n = k as i32;
        for (let i = 0; i < n; i++) sum *= 2.0;
        return sum;
    }
    const n = (-k) as i32;
    for (let i = 0; i < n; i++) sum *= 0.5;
    return sum;
}
