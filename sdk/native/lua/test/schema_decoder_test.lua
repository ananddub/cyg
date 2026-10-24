-- Schema decoder smoke test (Lua 5.1 / LuaJIT).
--
--   luajit sdk/native/lua/test/schema_decoder_test.lua
local source = debug.getinfo(1, "S").source
local here = source:sub(2):match("(.*/)") or "./"
package.path = here .. "../?.lua;" .. package.path

local S = require("openoxide_schema")

local checks, failures = 0, 0
local function check(name, ok)
    checks = checks + 1
    if not ok then failures = failures + 1 end
    print((ok and "CHECK " or "FAIL  ") .. name)
end

local kPlayerHash = 0x11111111
local kRootHash = 0x22222222

local Buf = {}
Buf.__index = Buf
function Buf.new() return setmetatable({ parts = {} }, Buf) end
function Buf:u8(v) self.parts[#self.parts + 1] = string.char(v % 256) end
function Buf:u16(v)
    self:u8(v % 256)
    self:u8(math.floor(v / 256) % 256)
end
function Buf:u32(v)
    self:u16(v % 65536)
    self:u16(math.floor(v / 65536) % 65536)
end
function Buf:raw(s) self.parts[#self.parts + 1] = s end
function Buf:s16(s)
    self:u16(#s)
    self:raw(s)
end
function Buf:magic() self:raw("OXS2") end
function Buf:bytes() return table.concat(self.parts) end

-- Portable little-endian IEEE-754 encoder for the test's simple values.
local function f64(x)
    local sign = 0
    if x < 0 then
        sign = 1
        x = -x
    end
    local b = {}
    if x == 0 then
        for i = 1, 7 do b[i] = 0 end
        b[8] = sign * 128
        return string.char(b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8])
    end
    local exponent = math.floor(math.log(x) / math.log(2))
    local mantissa = x / 2 ^ exponent
    if mantissa < 1 then
        exponent = exponent - 1
    elseif mantissa >= 2 then
        exponent = exponent + 1
    end
    mantissa = (x / 2 ^ exponent - 1) * 4503599627370496
    mantissa = math.floor(mantissa + 0.5)
    if mantissa >= 4503599627370496 then
        mantissa = 0
        exponent = exponent + 1
    end
    local biased = exponent + 1023
    for i = 1, 7 do
        b[i] = mantissa % 256
        mantissa = math.floor(mantissa / 256)
    end
    b[7] = b[7] + (biased % 16) * 16
    b[8] = sign * 128 + math.floor(biased / 16)
    return string.char(b[1], b[2], b[3], b[4], b[5], b[6], b[7], b[8])
end

local function field(b, name, code, flags, element)
    b:s16(name)
    b:u8(code)
    b:u8(flags)
    b:u8(element)
end

local function descriptor()
    local b = Buf.new()
    b:raw("OXSD")
    b:u8(3)
    b:u8(0)
    b:u16(1)
    b:s16("Player")
    b:u32(1)
    b:u32(kPlayerHash)
    b:u16(4)
    field(b, "x", 6, 0, 0)
    field(b, "hp", 1, 0, 0)
    field(b, "tags", 12, 0, 3)
    field(b, "target", 3, 1, 0)
    b:s16("State")
    b:u32(1)
    b:u32(kRootHash)
    b:u16(1)
    field(b, "round", 2, 0, 0)
    b:u16(1)
    b:s16("players")
    b:u16(0)
    return b:bytes()
end

local function row(s, id, x, hp, tags, target)
    s:u32(id)
    s:raw(f64(x))
    s:u8(hp)
    s:u8(#tags)
    for _, tag in ipairs(tags) do s:u32(tag) end
    if target then
        s:u8(1)
        s:u32(target)
    else
        s:u8(0)
    end
end

local function section_full()
    local s = Buf.new()
    s:magic()
    s:u8(1)
    s:u32(kPlayerHash)
    s:u32(2)
    row(s, 1, 1.5, 10, { 7, 9 }, 42)
    row(s, 2, 3.5, 20, {}, nil)
    return s:bytes()
end

local function world_collection(f, id, hash, section)
    f:u16(id)
    f:u32(hash)
    f:u32(#section)
    f:raw(section)
end

local function world_full()
    local f = Buf.new()
    f:magic()
    f:u8(3)
    f:u32(kRootHash)
    f:u16(1)
    f:u16(7)
    f:u16(1)
    world_collection(f, 0, kPlayerHash, section_full())
    return f:bytes()
end

local function world_patch()
    local f = Buf.new()
    f:magic()
    f:u8(4)
    f:u32(kRootHash)
    f:u16(1)
    f:u32(1)
    f:u16(8)

    local p = Buf.new()
    p:magic()
    p:u8(2)
    p:u32(kPlayerHash)
    p:u32(2)
    p:u8(3)
    p:u32(1)
    p:u32(2 ^ 0 + 2 ^ 3)
    p:raw(f64(9.5))
    p:u8(0)
    p:u8(2)
    p:u32(2)
    f:u16(1)
    world_collection(f, 0, kPlayerHash, p:bytes())
    return f:bytes()
end

local state, err = S.state_from_descriptor(descriptor())
check("descriptor parses", state ~= nil)
if not state then
    print("error: " .. tostring(err))
    os.exit(1)
end
check("bad descriptor rejected", S.parse_descriptor("OXSD") == nil)

local counters = { adds = 0, removes = 0, changes = 0, updates = 0, root_changes = 0 }
state.on_entity_added = function() counters.adds = counters.adds + 1 end
state.on_entity_removed = function() counters.removes = counters.removes + 1 end
state.on_field_changed = function() counters.changes = counters.changes + 1 end
state.on_entity_updated = function() counters.updates = counters.updates + 1 end
state.on_root_field_changed = function() counters.root_changes = counters.root_changes + 1 end

check("world full applies", state:apply(world_full()))
check("ready", state.ready)
check("added two", counters.adds == 2 and #state:collection("players").order == 2)

local p1 = state:find("players", 1)
local p2 = state:find("players", 2)
check("typed row", p1.values[1] == 1.5 and p1.values[2] == 10)
check("array field", #p1.values[3] == 2 and p1.values[3][2] == 9)
check("optional present", p1.values[4] == 42)
check("optional absent", p2.values[4] == nil and #p2.values[3] == 0)
check("root scalar", state.root_entity.values[1] == 7)

check("world patch applies", state:apply(world_patch()))
check("root patched", state.root_entity.values[1] == 8)
check("field updated", p1.values[1] == 9.5 and p1.values[4] == nil)
check("entity removed", state:find("players", 2) == nil)
check("events", counters.removes == 1 and counters.changes == 2
    and counters.updates == 1 and counters.root_changes == 2)

local bare = S.state_from_descriptor(descriptor())
check("patch before full dropped", bare ~= nil and not bare:apply(world_patch()))
check("truncated rejected", not state:apply("OXS2\1\0"))
check("foreign rejected", not state:apply(string.char(1, 2, 3, 4, 5)))

print(string.format("checks=%d failures=%d", checks, failures))
os.exit(failures > 0 and 1 or 0)
