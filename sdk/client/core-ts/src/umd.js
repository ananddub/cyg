// Drop-in UMD entry: module.exports is the flat legacy surface (CYG.Room,
// CYG.Client, ...) plus the typed additions, matching the hand-written
// sdk/client/web/cyg.js export shape 1:1.
const legacy = require("./core/legacy.js").default;
const { TypedRoom } = require("./room.js");
const { schemaFromTable, useSchemaTable } = require("./schema.js");

module.exports = {
    ...legacy,
    TypedRoom,
    schemaFromTable,
    useSchemaTable,
};
