# CYG.CodeGen

Roslyn source generator for deterministic Unity simulation components.

Write only the state layout:

```csharp
using CookYourGame.Sim;

[CYGComponent(Id = 1001)]
public partial struct CharacterState
{
    public FP Health;
    public FPVector3 Velocity;
    public bool Grounded;
}
```

The generator emits:

- `SizeInBytes` derived from field codecs, never hard-coded by hand.
- `TypeId` and a field-layout `SchemaHash`.
- Explicit little-endian `WriteTo` / `ReadFrom`.
- `ComputeChecksum` using the same wire bytes.
- `CopyTo(ref T)` as a plain struct assignment.
- `ICYGComponent` implementation.

Wire order is source-field order. `bool` is one byte (`0` / `1`). Nested
values must expose `public const int SizeInBytes` plus
`WriteTo`/`ReadFrom`/`ComputeChecksum`; the built-in `FP`, `FPVector2`,
`FPVector3`, and `FPQuaternion` do.

## Build

The project targets `netstandard2.0` and Roslyn 3.8 for Unity 2021.3
compatibility. It is outside Unity's import path because of the `~` suffix.

```bash
cd sdk/unity/Tools~/CYG.CodeGen
dotnet build -c Release
cp bin/Release/netstandard2.0/CYG.CodeGen.dll ../../Runtime/CodeGen/CYG.CodeGen.dll
```

`Runtime/CodeGen/CYG.CodeGen.dll.meta` is labelled `RoslynAnalyzer`. Unity must
reimport the DLL after it changes.

## Test

```bash
cd sdk/unity/Tools~/CYG.CodeGen.Tests
dotnet run -c Release
```

The test compiles a `CharacterState` with the generator, then checks the wire
size, round-trip, checksum stability, `CopyTo`, and short-buffer rejection.

## Diagnostics

| Id | Meaning |
| --- | --- |
| `CYG001` | `[CYGComponent]` struct is not `partial` |
| `CYG002` | Field type is a reference type, array, pointer, or otherwise unsupported |
| `CYG003` | Nested struct has no `SizeInBytes`/codec |
| `CYG004` | Duplicate explicit component id |
| `CYG005` | Readonly field cannot be assigned by `ReadFrom` |
| `CYG006` | Nested type declarations are not supported yet |
| `CYG007` | Generic components are not supported |
| `CYG008` | A field collides with a generated member name |
| `CYG009` | `UnityEngine.*` field would make simulation state non-deterministic |
| `CYG010` | `float`/`double` warning; IEEE bits are serialized, but prefer `FP` |
| `CYG011` | `readonly struct` cannot implement mutable `ReadFrom` |
| `CYG012` | Component has no serializable fields |
| `CYG013` | `LayoutKind.Explicit` is rejected; wire order is field order |
| `CYG014` | `ref struct` cannot implement the component interface |

Set `Id` explicitly before shipping. If it is omitted, the generator derives an
id from the fully qualified type name, so renaming or moving the type changes
its wire identity.

## Safety notes

- The generator is compile-time only; it never touches `CYGRollbackRunner`.
- It does not prove cross-platform determinism. Fixed-point math, same build,
  and identical code paths still matter.
- It does not make lockstep authoritative. Competitive games still need server
  collision, line-of-sight, and anti-cheat validation.
- `CopyTo` is a struct assignment, not a deep clone. Do not put reference types
  in a component.
