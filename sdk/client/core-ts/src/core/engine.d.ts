// Typed escape hatch for the client core (see engine.js).
// The full surface is typed in ../types/engine.d.ts; this loose module keeps
// the JS bundle source out of type inference until the module split lands.
declare const engine: Record<string, any>;
export default engine;
