// Typed escape hatch for the verbatim legacy core (see legacy.js).
// The full surface is typed in ../types/legacy.d.ts; this loose module keeps
// the JS bundle source out of type inference until the module split lands.
declare const legacy: Record<string, any>;
export default legacy;
