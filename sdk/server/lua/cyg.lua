-- =============================================================================
-- CYG Game Server — Guest Room SDK for Lua
-- Authoritative Multiplayer Room Lifecycle Contract
-- =============================================================================

local CYG = {}
CYG.__index = CYG

-- --- 1. ROOM BASE CLASS (COLYSEUS / ROBLOX STYLE) ---

CYG.Room = {}
CYG.Room.__index = CYG.Room

function CYG.Room:extend()
    local cls = {}
    for k, v in pairs(self) do
        cls[k] = v
    end
    cls.__index = cls
    cls.super = self
    return cls
end

function CYG.Room:new()
    local inst = setmetatable({}, self)
    inst.max_clients = 16
    inst.players = {}
    inst.current_tick = 0
    return inst
end

--- Called once when the room instance is allocated on the host.
function CYG.Room:onCreate(max_clients)
    self.max_clients = max_clients or 16
end

--- Called when a new player joins with authoritative spawn coordinates.
function CYG.Room:onJoin(id, spawn_x, spawn_z)
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
function CYG.Room:onLeave(id)
    self.players[id] = nil
end

--- Called when an authoritative client input packet arrives.
-- @param id: int64 player ID
-- @param data: raw byte string of client input
function CYG.Room:onInput(id, data)
    -- Override to process movement, aim angles, or abilities
end

--- Fixed authoritative simulation tick (20 Hz - 128 Hz).
-- @param dt: delta time in seconds (e.g. 0.0078125 for 128 Hz)
-- @return raw byte string of serialized state delta to broadcast to clients
function CYG.Room:onTick(dt)
    self.current_tick = self.current_tick + 1
    return ""
end

--- State persistence snapshot hook (optional).
-- @return raw byte string of full room state snapshot
function CYG.Room:onSave()
    return ""
end

--- State restoration hook from saved linear snapshot (optional).
-- @param data: raw byte string of previously saved state
function CYG.Room:onLoad(data)
end

--- Memory allocator hook for inbound network packets.
function CYG.Room:onAlloc(len)
    return 0
end

--- Room capacity check hook.
-- @param count: current connected player count
-- @return boolean true if room is full
function CYG.Room:onIsFull(count)
    return count >= self.max_clients
end

-- --- 2. BINARY DELTA PACKING HELPERS ---

--- Packs a linear memory pointer and byte length into a 64-bit integer return value.
function CYG.pack_delta(ptr, len)
    -- High 32 bits = pointer, Low 32 bits = length
    return (ptr * 4294967296) + len
end

--- Creates a binary buffer writer for serializing state deltas.
function CYG.createBuffer(initial_capacity)
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

-- --- 4. UNIVERSAL HARDWARE KEYCODES (A-Z, 0-9, ARROWS, F1-F12, MOUSE) ---

CYG.KeyCode = {
    None = 0,
    -- Mouse
    MouseLeft = 1, MouseMiddle = 2, MouseRight = 3, MouseBack = 4, MouseForward = 5,
    -- Control & Whitespace
    Backspace = 8, Tab = 9, Enter = 13, Shift = 16, Control = 17, Alt = 18,
    Pause = 19, CapsLock = 20, Escape = 27, Space = 32,
    -- Navigation
    PageUp = 33, PageDown = 34, End = 35, Home = 36, Insert = 45, Delete = 46, PrintScreen = 44,
    -- Arrows
    ArrowLeft = 37, ArrowUp = 38, ArrowRight = 39, ArrowDown = 40,
    Left = 37, Up = 38, Right = 39, Down = 40,
    -- Digits
    Digit0 = 48, Digit1 = 49, Digit2 = 50, Digit3 = 51, Digit4 = 52,
    Digit5 = 53, Digit6 = 54, Digit7 = 55, Digit8 = 56, Digit9 = 57,
    Num0 = 48, Num1 = 49, Num2 = 50, Num3 = 51, Num4 = 52,
    Num5 = 53, Num6 = 54, Num7 = 55, Num8 = 56, Num9 = 57,
    -- Alphabet A-Z
    KeyA = 65, KeyB = 66, KeyC = 67, KeyD = 68, KeyE = 69, KeyF = 70,
    KeyG = 71, KeyH = 72, KeyI = 73, KeyJ = 74, KeyK = 75, KeyL = 76,
    KeyM = 77, KeyN = 78, KeyO = 79, KeyP = 80, KeyQ = 81, KeyR = 82,
    KeyS = 83, KeyT = 84, KeyU = 85, KeyV = 86, KeyW = 87, KeyX = 88,
    KeyY = 89, KeyZ = 90,
    A = 65, B = 66, C = 67, D = 68, E = 69, F = 70,
    G = 71, H = 72, I = 73, J = 74, K = 75, L = 76,
    M = 77, N = 78, O = 79, P = 80, Q = 81, R = 82,
    S = 83, T = 84, U = 85, V = 86, W = 87, X = 88,
    Y = 89, Z = 90,
    -- Meta
    MetaLeft = 91, MetaRight = 92, ContextMenu = 93,
    -- Numpad
    Numpad0 = 96, Numpad1 = 97, Numpad2 = 98, Numpad3 = 99, Numpad4 = 100,
    Numpad5 = 101, Numpad6 = 102, Numpad7 = 103, Numpad8 = 104, Numpad9 = 105,
    NumpadMultiply = 106, NumpadAdd = 107, NumpadSeparator = 108,
    NumpadSubtract = 109, NumpadDecimal = 110, NumpadDivide = 111,
    -- Function Keys
    F1 = 112, F2 = 113, F3 = 114, F4 = 115, F5 = 116, F6 = 117,
    F7 = 118, F8 = 119, F9 = 120, F10 = 121, F11 = 122, F12 = 123,
    -- Modifiers
    ShiftLeft = 160, ShiftRight = 161, ControlLeft = 162, ControlRight = 163,
    AltLeft = 164, AltRight = 165
}

return CYG

