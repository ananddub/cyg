-- OpenOxide schema runtime for Lua 5.1 / LuaJIT.
--
-- Decodes the `OXSD` descriptor and applies `OXS2` full/patch sections. The
-- generated bindings (`openoxide_schema_generated.lua`) register factories:
--
--   local OpenOxideSchema = require("openoxide_schema")
--   local Generated = require("openoxide_schema_generated")
--   local registry = OpenOxideSchema.new_registry()
--   Generated.register(registry)
--   local state, err = OpenOxideSchema.state_from_descriptor(bytes, registry)
--   state:apply(section)
--
-- Field indices in callbacks are 1-based, matching `entity.values`. Use
-- `get_value`/`set_value` for name-based access.

local M = {}

M.FLAG_OPTIONAL = 1
M.FLAG_STATIC = 2
M.FLAG_TRANSIENT = 4

local KIND_NAMES = {
    [0] = nil,
    [1] = "u8",
    [2] = "u16",
    [3] = "u32",
    [4] = "i32",
    [5] = "f32",
    [6] = "f64",
    [7] = "bool",
    [8] = "i64",
    [9] = "i8",
    [10] = "i16",
    [11] = "string",
    [12] = "array",
}

local KIND_SIZES = {
    u8 = 1, i8 = 1, ["bool"] = 1,
    u16 = 2, i16 = 2,
    u32 = 4, i32 = 4, f32 = 4,
    f64 = 8, i64 = 8,
    ["string"] = 0, array = 0,
}

function M.kind_name(code)
    return KIND_NAMES[code]
end

function M.kind_size(code)
    local name = KIND_NAMES[code]
    return name and KIND_SIZES[name] or 0
end

-- ---------------------------------------------------------------------------
-- Little-endian reader
-- ---------------------------------------------------------------------------

local Reader = {}
Reader.__index = Reader

local function new_reader(data, offset, stop)
    return setmetatable({
        data = data,
        pos = offset or 1,
        stop = stop or #data,
        failed = false,
    }, Reader)
end

function Reader:need(count)
    if self.failed then return false end
    if count < 0 or self.pos + count - 1 > self.stop then
        self.failed = true
        return false
    end
    return true
end

function Reader:u8()
    if not self:need(1) then return 0 end
    local value = self.data:byte(self.pos)
    self.pos = self.pos + 1
    return value
end

function Reader:i8()
    local value = self:u8()
    if value >= 128 then return value - 256 end
    return value
end

function Reader:u16()
    local b1, b2 = self:u8(), self:u8()
    return b1 + b2 * 256
end

function Reader:i16()
    local value = self:u16()
    if value >= 32768 then return value - 65536 end
    return value
end

function Reader:u32()
    local b1, b2, b3, b4 = self:u8(), self:u8(), self:u8(), self:u8()
    return b1 + b2 * 256 + b3 * 65536 + b4 * 16777216
end

function Reader:i32()
    local value = self:u32()
    if value >= 2147483648 then return value - 4294967296 end
    return value
end

function Reader:i64()
    local low = self:u32()
    local high = self:u32()
    if self.failed then return 0 end
    -- Lua 5.1 numbers are doubles: exact up to 2^53, approximate beyond.
    local value = low + high * 4294967296
    if high >= 2147483648 then value = value - 18446744073709551616 end
    return value
end

function Reader:f32()
    local b1, b2, b3, b4 = self:u8(), self:u8(), self:u8(), self:u8()
    if self.failed then return 0 end
    local sign = 1
    if b4 >= 128 then
        sign = -1
        b4 = b4 - 128
    end
    local exponent = b4 * 2 + math.floor(b3 / 128)
    local mantissa = (b3 % 128) * 65536 + b2 * 256 + b1
    if exponent == 0 then
        if mantissa == 0 then return sign * 0 end
        return sign * mantissa * 2 ^ -149
    elseif exponent == 255 then
        if mantissa == 0 then return sign * math.huge end
        return 0 / 0
    end
    return sign * (1 + mantissa / 8388608) * 2 ^ (exponent - 127)
end

function Reader:f64()
    local b1, b2, b3, b4 = self:u8(), self:u8(), self:u8(), self:u8()
    local b5, b6, b7, b8 = self:u8(), self:u8(), self:u8(), self:u8()
    if self.failed then return 0 end
    local sign = 1
    if b8 >= 128 then
        sign = -1
        b8 = b8 - 128
    end
    local exponent = b8 * 16 + math.floor(b7 / 16)
    local mantissa = (b7 % 16) * 281474976710656 + b6 * 1099511627776
        + b5 * 4294967296 + b4 * 16777216 + b3 * 65536 + b2 * 256 + b1
    if exponent == 0 then
        if mantissa == 0 then return sign * 0 end
        return sign * mantissa * 2 ^ -1074
    elseif exponent == 2047 then
        if mantissa == 0 then return sign * math.huge end
        return 0 / 0
    end
    return sign * (1 + mantissa / 4503599627370496) * 2 ^ (exponent - 1023)
end

function Reader:string8()
    local length = self:u8()
    if self.failed or not self:need(length) then return "" end
    local value = self.data:sub(self.pos, self.pos + length - 1)
    self.pos = self.pos + length
    return value
end

function Reader:string16()
    local length = self:u16()
    if self.failed or not self:need(length) then return "" end
    local value = self.data:sub(self.pos, self.pos + length - 1)
    self.pos = self.pos + length
    return value
end

local function is_magic(reader)
    return reader:u8() == 79 and reader:u8() == 88 and reader:u8() == 83
        and reader:u8() == 50 and not reader.failed
end

-- ---------------------------------------------------------------------------
-- Descriptor
-- ---------------------------------------------------------------------------

local function read_field(reader, version)
    local field = { name = reader:string16() }
    local code = reader:u8()
    field.code = code
    field.kind = KIND_NAMES[code]
    if reader.failed or not field.kind then return nil, "unknown field type code" end
    field.flags = 0
    field.element = nil
    if version >= 3 then
        field.flags = reader:u8()
        local element_code = reader:u8()
        if reader.failed then return nil, "truncated field" end
        if element_code ~= 0 then
            field.element = KIND_NAMES[element_code]
            if not field.element then return nil, "unknown array element type code" end
        end
    end
    field.size = KIND_SIZES[field.kind] or 0
    return field
end

function M.parse_descriptor(data)
    if type(data) ~= "string" or #data < 6 then
        return nil, "descriptor too short"
    end
    if data:sub(1, 4) ~= "OXSD" then
        return nil, "bad descriptor magic"
    end
    local reader = new_reader(data, 5)
    local version = reader:u8()
    if reader.failed or version < 1 or version > 3 then
        return nil, "unsupported descriptor version"
    end
    reader:u8() -- flags

    local root = {
        format_version = version,
        name = "",
        version = 0,
        hash = 0,
        fields = {},
        collections = {},
        schemas = {},
        schema_by_name = {},
    }

    local schema_count = reader:u16()
    if reader.failed then return nil, "truncated descriptor" end
    for _ = 1, schema_count do
        local schema = { fields = {} }
        schema.name = reader:string16()
        schema.version = reader:u32()
        schema.hash = reader:u32()
        local field_count = reader:u16()
        if reader.failed then return nil, "truncated schema header" end
        for _ = 1, field_count do
            local field, err = read_field(reader, version)
            if not field then return nil, err end
            schema.fields[#schema.fields + 1] = field
        end
        root.schemas[#root.schemas + 1] = schema
        root.schema_by_name[schema.name] = schema
    end

    local root_name = reader:string16()
    if reader.failed then return nil, "truncated root name" end
    if version == 1 then
        local index = reader:u16()
        if reader.failed then return nil, "truncated root reference" end
        root.name = root_name
        if root_name ~= "" and index < #root.schemas then
            root.collections[1] = {
                name = root_name,
                schema = root.schemas[index + 1].name,
            }
        end
    else
        root.version = reader:u32()
        root.hash = reader:u32()
        local scalar_count = reader:u16()
        if reader.failed then return nil, "truncated root header" end
        for _ = 1, scalar_count do
            local field, err = read_field(reader, version)
            if not field then return nil, err end
            root.fields[#root.fields + 1] = field
        end
        local collection_count = reader:u16()
        if reader.failed then return nil, "truncated root collections" end
        for _ = 1, collection_count do
            local collection_name = reader:string16()
            local index = reader:u16()
            if reader.failed then return nil, "truncated collection reference" end
            if index >= #root.schemas then return nil, "collection schema out of range" end
            root.collections[#root.collections + 1] = {
                name = collection_name,
                schema = root.schemas[index + 1].name,
            }
        end
        root.name = root_name
    end

    if reader.pos ~= #data + 1 then
        return nil, "trailing bytes in descriptor"
    end
    return root
end

-- ---------------------------------------------------------------------------
-- Entities and registry
-- ---------------------------------------------------------------------------

function M.default_value(field)
    if field.flags and field.flags % 2 == 1 then return nil end
    local kind = field.kind
    if kind == "bool" then return false end
    if kind == "string" then return "" end
    if kind == "array" then return {} end
    return 0
end

function M.new_entity(definition)
    local values = {}
    for i, field in ipairs(definition.fields) do
        values[i] = M.default_value(field)
    end
    return { id = 0, definition = definition, values = values }
end

function M.clear_fields(entity)
    local values = entity.values or {}
    local fields = entity.definition.fields
    for i, field in ipairs(fields) do
        values[i] = M.default_value(field)
    end
    for i = #fields + 1, #values do
        values[i] = nil
    end
    entity.values = values
end

function M.set_field(entity, index, value)
    if index >= 1 and index <= #entity.values then
        entity.values[index] = value
    end
end

function M.get_field(entity, index)
    if index >= 1 and index <= #entity.values then
        return entity.values[index]
    end
    return nil
end

function M.field_index(entity, name)
    for i, field in ipairs(entity.definition.fields) do
        if field.name == name then return i end
    end
    return nil
end

function M.has_field(entity, name)
    return M.field_index(entity, name) ~= nil
end

function M.get_value(entity, name)
    local index = M.field_index(entity, name)
    return index and entity.values[index] or nil
end

function M.set_value(entity, name, value)
    local index = M.field_index(entity, name)
    if index then entity.values[index] = value end
end

function M.new_registry()
    return { factories = {} }
end

function M.register(registry, schema_name, factory)
    if not registry or type(schema_name) ~= "string" or schema_name == ""
        or type(factory) ~= "function" then
        return
    end
    registry.factories[schema_name] = factory
end

function M.create(registry, definition)
    local entity
    local factory = registry and registry.factories[definition.name]
    if factory then entity = factory() end
    if type(entity) ~= "table" then entity = M.new_entity(definition) end
    entity.definition = definition
    entity.id = 0
    M.clear_fields(entity)
    return entity
end

-- ---------------------------------------------------------------------------
-- State
-- ---------------------------------------------------------------------------

local State = {}
State.__index = State

local function values_equal(a, b)
    if a == b then return true end
    if type(a) ~= "table" or type(b) ~= "table" then return false end
    if #a ~= #b then return false end
    for i = 1, #a do
        if not values_equal(a[i], b[i]) then return false end
    end
    return true
end

local function kind_scalar(reader, kind)
    if kind == "u8" then return reader:u8() end
    if kind == "i8" then return reader:i8() end
    if kind == "u16" then return reader:u16() end
    if kind == "i16" then return reader:i16() end
    if kind == "u32" then return reader:u32() end
    if kind == "i32" then return reader:i32() end
    if kind == "f32" then return reader:f32() end
    if kind == "f64" then return reader:f64() end
    if kind == "bool" then return reader:u8() ~= 0 end
    if kind == "i64" then return reader:i64() end
    if kind == "string" then return reader:string8() end
    reader.failed = true
    return nil
end

local function read_value(reader, field)
    if field.flags and field.flags % 2 == 1 then
        local present = reader:u8()
        if reader.failed or present == 0 then return nil end
    end
    if field.kind ~= "array" then
        return kind_scalar(reader, field.kind)
    end
    local element = field.element or "f64"
    local count = reader:u8()
    if reader.failed then return nil end
    local values = {}
    for i = 1, count do
        values[i] = kind_scalar(reader, element)
        if reader.failed then return nil end
    end
    return values
end

local function fire(state, name, ...)
    local callback = state[name]
    if callback then callback(...) end
end

function M.new_state(root, registry)
    if type(root) ~= "table" then return nil, "missing schema root" end
    local state = setmetatable({
        root = root,
        registry = registry,
        collections = {},
        collection_by_name = {},
        ready = false,
        on_entity_added = nil,
        on_entity_removed = nil,
        on_entity_updated = nil,
        on_field_changed = nil,
        on_root_field_changed = nil,
    }, State)

    for _, collection_def in ipairs(root.collections or {}) do
        local schema = root.schema_by_name and root.schema_by_name[collection_def.schema]
        if not schema then
            return nil, "unknown schema '" .. tostring(collection_def.schema) .. "'"
        end
        local collection = {
            name = collection_def.name,
            definition = schema,
            entities = {},
            order = {},
            ready = false,
        }
        state.collections[#state.collections + 1] = collection
        state.collection_by_name[collection.name] = collection
    end

    state.root_definition = {
        name = root.name,
        version = root.version,
        hash = root.hash,
        fields = root.fields,
    }
    state.root_entity = M.create(registry, state.root_definition)
    return state
end

function M.state_from_descriptor(data, registry)
    local root, err = M.parse_descriptor(data)
    if not root then return nil, err end
    return M.new_state(root, registry)
end

function State:collection(name)
    return self.collection_by_name[name]
end

function State:find(collection_name, id)
    local collection = self.collection_by_name[collection_name]
    return collection and collection.entities[id] or nil
end

function State:_apply_body(kind, collection, reader)
    if kind == 1 then return self:_apply_full(collection, reader) end
    return self:_apply_patch(collection, reader)
end

function State:_apply_section(collection, reader, expected)
    if not is_magic(reader) then return false end
    local kind = reader:u8()
    if reader.failed or kind ~= expected then return false end
    return self:_apply_body(kind, collection, reader)
end

function State:_apply_full(collection, reader)
    local hash = reader:u32()
    if reader.failed or hash ~= collection.definition.hash then return false end
    local count = reader:u32()
    if reader.failed then return false end

    local fields = collection.definition.fields
    local incoming = {}
    local ids = {}
    for i = 1, count do
        local id = reader:u32()
        if reader.failed then return false end
        if ids[id] then return false end
        ids[id] = true
        local values = {}
        for f = 1, #fields do
            values[f] = read_value(reader, fields[f])
            if reader.failed then return false end
        end
        incoming[i] = { id, values }
    end

    for i = #collection.order, 1, -1 do
        local id = collection.order[i]
        if not ids[id] then
            local entity = collection.entities[id]
            if entity then
                fire(self, "on_entity_removed", collection.name, id, entity)
                collection.entities[id] = nil
                table.remove(collection.order, i)
            end
        end
    end

    for _, entry in ipairs(incoming) do
        local id, values = entry[1], entry[2]
        local entity = collection.entities[id]
        if entity then
            self:_apply_values(collection, id, entity, values, true)
        else
            entity = M.create(self.registry, collection.definition)
            entity.id = id
            self:_apply_values(collection, id, entity, values, false)
            collection.entities[id] = entity
            collection.order[#collection.order + 1] = id
            fire(self, "on_entity_added", collection.name, id, entity)
        end
    end
    collection.ready = true
    return true
end

function State:_apply_patch(collection, reader)
    local hash = reader:u32()
    if reader.failed or hash ~= collection.definition.hash then return false end
    -- A patch before the full row would be lost; the host keeps sending
    -- full snapshots until the client acknowledges the baseline.
    if not collection.ready then return false end
    local operations = reader:u32()
    if reader.failed then return false end

    local fields = collection.definition.fields
    for _ = 1, operations do
        local op = reader:u8()
        if reader.failed then return false end
        if op == 1 then
            local id = reader:u32()
            if reader.failed then return false end
            local values = {}
            for f = 1, #fields do
                values[f] = read_value(reader, fields[f])
                if reader.failed then return false end
            end
            local entity = collection.entities[id]
            if entity then
                self:_apply_values(collection, id, entity, values, true)
            else
                entity = M.create(self.registry, collection.definition)
                entity.id = id
                self:_apply_values(collection, id, entity, values, false)
                collection.entities[id] = entity
                collection.order[#collection.order + 1] = id
                fire(self, "on_entity_added", collection.name, id, entity)
            end
        elseif op == 2 then
            local id = reader:u32()
            if reader.failed then return false end
            local entity = collection.entities[id]
            if entity then
                fire(self, "on_entity_removed", collection.name, id, entity)
                collection.entities[id] = nil
                for i = 1, #collection.order do
                    if collection.order[i] == id then
                        table.remove(collection.order, i)
                        break
                    end
                end
            end
        elseif op == 3 then
            local id = reader:u32()
            local mask = reader:u32()
            if reader.failed then return false end
            local entity = collection.entities[id]
            local changed = false
            for f = 1, #fields do
                if f <= 32 and math.floor(mask / 2 ^ (f - 1)) % 2 == 1 then
                    local value = read_value(reader, fields[f])
                    if reader.failed then return false end
                    if entity then
                        local previous = entity.values[f]
                        entity.values[f] = value
                        if not values_equal(previous, value) then
                            changed = true
                            fire(self, "on_field_changed", collection.name, id, f,
                                fields[f].name, value, previous)
                        end
                    end
                end
            end
            if changed and entity then
                fire(self, "on_entity_updated", collection.name, id, entity)
            end
        else
            return false
        end
    end
    return true
end

function State:_apply_world(kind, reader)
    -- A world patch before the baseline would be lost; the host repeats full
    -- frames until the client acknowledges.
    if kind == 4 and not self.ready then return false end

    local root_hash = reader:u32()
    local root_count = reader:u16()
    if reader.failed then return false end
    if root_count ~= #self.root.fields or root_hash ~= self.root_definition.hash then
        return false
    end

    local mask = 0
    if kind == 4 then
        mask = reader:u32()
        if reader.failed then return false end
    end
    for f = 1, root_count do
        if kind == 3 or (f <= 32 and math.floor(mask / 2 ^ (f - 1)) % 2 == 1) then
            local value = read_value(reader, self.root.fields[f])
            if reader.failed then return false end
            local previous = self.root_entity.values[f]
            self.root_entity.values[f] = value
            if not values_equal(previous, value) then
                fire(self, "on_root_field_changed", f, self.root.fields[f].name,
                    value, previous)
            end
        end
    end

    local count = reader:u16()
    if reader.failed then return false end
    for _ = 1, count do
        local collection_id = reader:u16()
        local hash = reader:u32()
        local length = reader:u32()
        if reader.failed then return false end
        local collection = self.collections[collection_id + 1]
        if not collection or collection.definition.hash ~= hash then return false end
        if not reader:need(length) then return false end
        local section_data = reader.data:sub(reader.pos, reader.pos + length - 1)
        local section = new_reader(section_data)
        local expected = kind == 3 and 1 or 2
        if not self:_apply_section(collection, section, expected) then return false end
        if section.pos ~= length + 1 then return false end
        reader.pos = reader.pos + length
    end
    if reader.pos ~= reader.stop + 1 then return false end
    if kind == 3 then self.ready = true end
    return true
end

function State:apply(data)
    local reader = new_reader(data)
    if not is_magic(reader) then return false end
    local kind = reader:u8()
    if reader.failed then return false end
    if kind == 1 or kind == 2 then
        if #self.collections == 0 then return false end
        return self:_apply_body(kind, self.collections[1], reader)
    end
    if kind == 3 or kind == 4 then
        return self:_apply_world(kind, reader)
    end
    return false
end

function State:_apply_values(collection, id, entity, values, should_fire)
    local changed = false
    local fields = collection.definition.fields
    for f = 1, #values do
        local previous = entity.values[f]
        local value = values[f]
        entity.values[f] = value
        if not values_equal(previous, value) then
            changed = true
            if should_fire then
                fire(self, "on_field_changed", collection.name, id, f,
                    fields[f].name, value, previous)
            end
        end
    end
    if changed and should_fire then
        fire(self, "on_entity_updated", collection.name, id, entity)
    end
end

return M
