-- =============================================================================
-- OpenOxide Game Server — Guest Room SDK for Lua
-- Authoritative Multiplayer Room Lifecycle Contract
-- =============================================================================

local OpenOxide = {}
OpenOxide.__index = OpenOxide

-- --- 1. ROOM BASE CLASS (COLYSEUS / ROBLOX STYLE) ---

OpenOxide.Room = {}
OpenOxide.Room.__index = OpenOxide.Room

function OpenOxide.Room:extend()
    local cls = {}
    for k, v in pairs(self) do
        cls[k] = v
    end
    cls.__index = cls
    cls.super = self
    return cls
end

function OpenOxide.Room:new()
    local inst = setmetatable({}, self)
    inst.max_clients = 16
    inst.players = {}
    inst.current_tick = 0
    return inst
end

--- Called once when the room instance is allocated on the host.
function OpenOxide.Room:onCreate(max_clients)
    self.max_clients = max_clients or 16
end

--- Called when a new player joins with authoritative spawn coordinates.
function OpenOxide.Room:onJoin(id, spawn_x, spawn_z)
    self.players[id] = {
        id = id,
        x = spawn_x,
        y = 0.0,
        z = spawn_z,
        health = 100.0,
        score = 0.0,
    }
end

--- Called when a player leaves or disconnects.
function OpenOxide.Room:onLeave(id)
    self.players[id] = nil
end

--- Called when an authoritative client input packet arrives.
-- @param id: int64 player ID
-- @param data: raw byte string of client input
function OpenOxide.Room:onInput(id, data)
    -- Override to process movement, aim angles, or abilities
end

--- Fixed authoritative simulation tick (20 Hz - 128 Hz).
-- @param dt: delta time in seconds (e.g. 0.0078125 for 128 Hz)
-- @return raw byte string of serialized state delta to broadcast to clients
function OpenOxide.Room:onTick(dt)
    self.current_tick = self.current_tick + 1
    return ""
end

--- State persistence snapshot hook (optional).
-- @return raw byte string of full room state snapshot
function OpenOxide.Room:onSave()
    return ""
end

--- State restoration hook from saved linear snapshot (optional).
-- @param data: raw byte string of previously saved state
function OpenOxide.Room:onLoad(data)
end

--- Memory allocator hook for inbound network packets.
function OpenOxide.Room:onAlloc(len)
    return 0
end

--- Room capacity check hook.
-- @param count: current connected player count
-- @return boolean true if room is full
function OpenOxide.Room:onIsFull(count)
    return count >= self.max_clients
end

-- --- 2. BINARY DELTA PACKING HELPERS ---

--- Packs a linear memory pointer and byte length into a 64-bit integer return value.
function OpenOxide.pack_delta(ptr, len)
    -- High 32 bits = pointer, Low 32 bits = length
    return (ptr * 4294967296) + len
end

--- Creates a binary buffer writer for serializing state deltas.
function OpenOxide.createBuffer(initial_capacity)
    local buf = {}
    buf.chunks = {}
    buf.length = 0

    function buf:writeUInt8(val)
        table.insert(self.chunks, string.char(val % 256))
        self.length = self.length + 1
    end

    function buf:writeUInt32LE(val)
        local b1 = val % 256
        local b2 = math.floor(val / 256) % 256
        local b3 = math.floor(val / 65536) % 256
        local b4 = math.floor(val / 16777216) % 256
        table.insert(self.chunks, string.char(b1, b2, b3, b4))
        self.length = self.length + 4
    end

    function buf:writeFloat64LE(val)
        -- Uses string.pack in Lua 5.3+, or fallback
        if string.pack then
            table.insert(self.chunks, string.pack("<d", val))
            self.length = self.length + 8
        end
    end

    function buf:writeBytes(bytes)
        table.insert(self.chunks, bytes)
        self.length = self.length + #bytes
    end

    function buf:toString()
        return table.concat(self.chunks)
    end

    return buf
end

return OpenOxide
