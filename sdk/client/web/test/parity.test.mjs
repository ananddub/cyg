import assert from "node:assert/strict";
import CYG from "../cyg.js";

// Colyseus-parity: matchmaking failures carry MatchMakeError.name + .code,
// and client.reconnect() guards against a missing token.
{
    const originalFetch = globalThis.fetch;
    globalThis.fetch = async () => ({ status: 404, text: async () => "no room" });
    try {
        const client = new CYG.Client("http://127.0.0.1:1");
        try {
            await client.joinOrCreate("ghost_room", {});
            assert.fail("should have thrown");
        } catch (e) {
            assert.equal(e.name, "MatchMakeError", `name: ${e.name}`);
            assert.equal(e.code, 404, `code: ${e.code}`);
        }
        const client2 = new CYG.Client("http://127.0.0.1:1");
        try {
            await client2.reconnect(null);
            assert.fail("should have thrown");
        } catch (e) {
            assert.equal(e.name, "MatchMakeError");
            assert.equal(e.code, 400);
        }
    } finally {
        globalThis.fetch = originalFetch;
    }
}
console.log("parity.test: all assertions passed");
