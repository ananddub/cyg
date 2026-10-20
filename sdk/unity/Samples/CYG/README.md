# CYG `.cyg` samples

| File | Covers |
| --- | --- |
| `arena.cyg` | A small shooter schema: `Character`, `Transform`, `Projectile`, `Loadout`, `Inventory`, `PlayerInfo`, enums/flags/union, input/global, events and signals. |
| `types.cyg` | Every supported field type, nested value structs, all fixed containers, enums/flags, a tagged union, 9 input buttons, globals, events, signals and zero-payload events/signals. |
| `bad_*.cyg` | Intentionally invalid inputs used to exercise diagnostics. |

Validate:

```bash
dotnet build sdk/unity/Tools~/CYG.Dsl.Cli/CYG.Dsl.Cli.csproj -c Release
dotnet sdk/unity/Tools~/CYG.Dsl.Cli/bin/Release/net10.0/CYG.Dsl.Cli.dll \
  sdk/unity/Samples/CYG/arena.cyg --check
```

Emit C#:

```bash
dotnet sdk/unity/Tools~/CYG.Dsl.Cli/bin/Release/net10.0/CYG.Dsl.Cli.dll \
  sdk/unity/Samples/CYG --out Assets/CYGGenerated
```

The emitted file contains partial struct declarations only. The
`CYG.CodeGen` Roslyn generator (bundled at `Runtime/CodeGen/CYG.CodeGen.dll`)
adds `SizeInBytes`, `TypeId`, `SchemaHash`, `WriteTo`, `ReadFrom`,
`ComputeChecksum` and `CopyTo` when Unity compiles the generated source.

The bad samples are expected to exit with code 1:

```bash
for f in sdk/unity/Samples/CYG/bad_*.cyg; do
  dotnet sdk/unity/Tools~/CYG.Dsl.Cli/bin/Release/net10.0/CYG.Dsl.Cli.dll "$f" --check
done
```

`arena.cyg` is also loaded by `Tools~/CYG.Dsl.Tests`, which compiles the
emitted C# together with the runtime, runs the source generator, and verifies
wire sizes, ids, schema hashes, round-trips, checksums, and the generated
components inside `CYGWorld` snapshots/restore.
