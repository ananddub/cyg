namespace CookYourGame.Lsp;

/// <summary>Hover and completion text for the built-in language surface.</summary>
internal static class CygDocs
{
    public static readonly IReadOnlyList<string> IntegerUnderlyings = new[]
    {
        "Byte", "SByte", "Int16", "UInt16", "Int32", "UInt32", "Int64", "UInt64",
    };

    public static readonly IReadOnlyList<string> BuiltinTypes = new[]
    {
        "FP", "Byte", "SByte", "Int16", "UInt16", "Int32", "UInt32", "Int64", "UInt64",
        "Boolean", "FPVector2", "FPVector3", "FPQuaternion", "FPMatrix4x4", "FPAABB",
        "FPBounds", "entity_ref", "player_ref",
    };

    public static readonly IReadOnlyList<string> ContainerTypes = new[]
    {
        "QString", "array", "list", "dictionary", "asset_ref",
    };

    public static readonly IReadOnlyList<string> TopLevelKeywords = new[]
    {
        "namespace", "asset", "struct", "component", "enum", "flags", "union",
        "input", "global", "event", "synced_event", "signal",
    };

    private static readonly Dictionary<string, string> KeywordDocs = new(StringComparer.Ordinal)
    {
        ["namespace"] =
            "Sets the C# namespace used by the emitter. It must be the first declaration in the file. " +
            "Default: `CookYourGame.Sim.Generated`.\n\n```cyg\nnamespace MyGame.Sim;\n```",
        ["asset"] =
            "Declares an asset schema name. Use it in a field as `asset_ref<Name>`. " +
            "At runtime the field carries a stable asset id, not a managed reference.\n\n```cyg\nasset WeaponSpec;\n```",
        ["struct"] =
            "A plain value type copied by value. Fields must use deterministic, fixed-size types " +
            "(`FP`, integer types, fixed containers, or other declared structs/enums/unions).",
        ["component"] =
            "A deterministic ECS component. Give every component a stable explicit id in the user range " +
            "**1000..999999**; clients must agree on it across builds. Fields follow the same deterministic type rules.\n\n" +
            "```cyg\ncomponent Character : 1001 {\n    FP Health;\n}\n```",
        ["enum"] =
            "An integer enum. The optional underlying type defaults to `Int32`. Members auto-increment from the " +
            "previous value when no explicit value is written.",
        ["flags"] =
            "A bit-flags enum. It **requires** an explicit underlying type. `None` is 0 and the remaining members " +
            "receive successive powers of two unless a value is written explicitly.",
        ["union"] =
            "A tagged union. Declare at least two `struct` variants; exactly one variant payload is active at a time.",
        ["input"] =
            "Declares the per-player input schema. A module may have one `input` block. Use `button Name;` for " +
            "digital inputs (up to 64) and fixed-point types for analog axes.",
        ["global"] =
            "Declares the simulation-global state schema. A module may have one `global` block.",
        ["event"] =
            "A one-shot networked event. Give it an explicit id in **1000000..1999999**.",
        ["synced_event"] =
            "A networked event that is also recorded in the synced event stream. The id must be in " +
            "**1000000..1999999**.",
        ["signal"] =
            "A server-to-client signal. Declare its parameters like a function; signals carry no id and are not " +
            "part of simulation state.\n\n```cyg\nsignal OnPlayerDamaged(entity_ref victim, FP damage);\n```",
        ["button"] =
            "A digital input button (one bit) inside an `input` block. Up to 64 buttons are supported.",
        ["rpc"] =
            "Reserved for a later CYG release. Use `command`, `signal`, `event` or `synced_event` today.",
        ["command"] =
            "A player command: external input recorded per tick, never hashed into simulation state and never " +
            "rewound by rollback. Give it an explicit id in **2000000..2999999**.\n\n" +
            "```cyg\ncommand SpawnCharacter : 2000001 {\n    player_ref Owner;\n    FPVector3 Position;\n}\n```\n\n" +
            "The compiler emits a payload struct plus a `<Name>Command` adapter deriving from `CYGCommand`.",
    };

    private static readonly Dictionary<string, string> TypeDocs = new(StringComparer.Ordinal)
    {
        ["FP"] =
            "Q32.32 fixed-point number. **Use `FP` instead of `float`/`double` in simulation state.** " +
            "The tick math is integer-only, which is what makes rollback bit-identical across platforms.",
        ["Byte"] = "Unsigned 8-bit integer.",
        ["SByte"] = "Signed 8-bit integer.",
        ["Int16"] = "Signed 16-bit integer.",
        ["UInt16"] = "Unsigned 16-bit integer.",
        ["Int32"] = "Signed 32-bit integer.",
        ["UInt32"] = "Unsigned 32-bit integer.",
        ["Int64"] = "Signed 64-bit integer.",
        ["UInt64"] = "Unsigned 64-bit integer.",
        ["Boolean"] = "Boolean value.",
        ["FPVector2"] = "Deterministic two-component fixed-point vector.",
        ["FPVector3"] = "Deterministic three-component fixed-point vector.",
        ["FPQuaternion"] = "Deterministic fixed-point quaternion.",
        ["FPMatrix4x4"] = "Deterministic 4x4 fixed-point matrix.",
        ["FPAABB"] = "Deterministic axis-aligned bounding box.",
        ["FPBounds"] = "Deterministic bounds (center + extents).",
        ["entity_ref"] = "Reference to a simulation entity.",
        ["player_ref"] = "Reference to a player.",
        ["QString"] =
            "Fixed-capacity UTF-8 string. Write `QString<32> Name;`. The capacity is capped at 256 bytes.",
        ["array"] =
            "Fixed-size array. Write `array<T>[N] Name;`. The capacity is capped at 1024 elements and `T` must be deterministic.",
        ["list"] =
            "Fixed-capacity list. Write `list<T>[N] Name;`. The capacity is capped at 256 elements.",
        ["dictionary"] =
            "Fixed-capacity dictionary. Write `dictionary<K, V>[N] Name;`. The capacity is capped at 256 entries.",
        ["asset_ref"] =
            "Reference to a declared asset schema. Write `asset_ref<WeaponSpec> Primary;` and declare the schema " +
            "once with `asset WeaponSpec;`.",
        ["float"] =
            "**Not allowed.** Floating-point values diverge across platforms. Use `FP` for deterministic fixed-point math.",
        ["double"] =
            "**Not allowed.** Floating-point values diverge across platforms. Use `FP` for deterministic fixed-point math.",
        ["decimal"] = "**Not allowed.** Use `FP` for deterministic fixed-point math.",
        ["string"] = "**Not allowed.** Use `QString<N>` for a fixed-capacity string.",
        ["String"] = "**Not allowed.** Use `QString<N>` for a fixed-capacity string.",
        ["Vector2"] = "**Not allowed.** Use `FPVector2`.",
        ["Vector3"] = "**Not allowed.** Use `FPVector3`.",
        ["Vector4"] = "**Not allowed.** Use `FPVector3` or separate `FP` fields.",
        ["Quaternion"] = "**Not allowed.** Use `FPQuaternion`.",
        ["Matrix4x4"] = "**Not allowed.** Use `FPMatrix4x4`.",
        ["List"] = "**Not allowed.** Use `list<T>[N]`.",
        ["Dictionary"] = "**Not allowed.** Use `dictionary<K, V>[N]`.",
        ["HashSet"] = "**Not allowed.** Use `flags` or a fixed container.",
        ["Queue"] = "**Not allowed.** Use `list<T>[N]`.",
        ["Stack"] = "**Not allowed.** Use `array<T>[N]` or `list<T>[N]`.",
    };

    public static bool IsKnownKeyword(string name) => KeywordDocs.ContainsKey(name);

    public static bool IsKnownType(string name) => TypeDocs.ContainsKey(name);

    public static string? Keyword(string name) => KeywordDocs.GetValueOrDefault(name);

    public static string? Type(string name) => TypeDocs.GetValueOrDefault(name);

    public static string? DescribeToken(string name)
    {
        if (KeywordDocs.TryGetValue(name, out string? keyword))
        {
            return keyword;
        }

        return TypeDocs.GetValueOrDefault(name);
    }
}
