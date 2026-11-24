// Drop-in UMD entry: module.exports is the flat core surface (CYG.Room,
// CYG.Client, ...) plus the typed additions, matching the hand-written
// sdk/client/web/cyg.js export shape 1:1.
const engine = require("./core/engine.js").default;
const { TypedRoom } = require("./room.js");
const { schemaFromTable, useSchemaTable } = require("./schema.js");

module.exports = {
    ...engine,
    TypedRoom,
    schemaFromTable,
    useSchemaTable,
};
