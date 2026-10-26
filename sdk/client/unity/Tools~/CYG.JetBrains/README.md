# CYG for JetBrains IDEs

Adds `.cyg` support to IntelliJ-based IDEs (RustRover 2026.2+ and IntelliJ IDEA 2026.2+):

- file type and lexer with syntax highlighting
- line/block comments and brace matching
- the CYG language server (completion, hover, diagnostics, go to definition,
  references, rename, document/workspace symbols)

## Build

Requires a .NET 10 SDK/runtime and a 2026.2+ IDE installation for its platform jars.

```bash
./build.sh
```

Set `IDEA_HOME` if the IDE is not at `~/Documents/RustRover-2026.2.3`.
The plugin ZIP is written to `build/CYG-0.1.0.zip`; install it via
Settings | Plugins | Install Plugin from Disk.

The CYG language server is bundled inside the ZIP. At runtime the plugin uses
`dotnet` from `PATH`; install the .NET 10 runtime on the machine.
As a fallback, a `cyg-lsp` executable on `PATH` is used, and when the IDE is
started inside this repository the locally built
`sdk/unity/Tools~/CYG.Lsp/bin/{Release,Debug}/net10.0/CYG.Lsp.dll` is picked up.
