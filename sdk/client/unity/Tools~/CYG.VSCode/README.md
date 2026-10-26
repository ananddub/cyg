# CYG Language for VS Code

Language support for the CookYourGame deterministic simulation DSL (`.cyg`):

- Syntax highlighting for declarations, fields, containers, ids and enum members
- Compiler-accurate diagnostics as you type
- Completion for declarations, types, containers, enum members, assets and ids
- Hover documentation for keywords, types and declared symbols
- Go to definition, find references, rename and workspace symbols
- Document outline, breadcrumbs and semantic tokens
- Context-aware declaration, type, container and id snippets

## Requirements

- VS Code 1.91 or newer
- .NET 10 SDK (only to build the language server from source)

## Setup

The extension starts `CYG.Lsp.dll` through the `dotnet` executable. It looks for the
server in this order:

1. `cyg.lsp.serverPath` if set
2. `server/CYG.Lsp.dll` inside the extension (bundled with `npm run build:server`)
3. `../CYG.Lsp/bin/Release/net10.0/CYG.Lsp.dll`
4. `../CYG.Lsp/bin/Debug/net10.0/CYG.Lsp.dll`

For development in this repository, build the server once:

```bash
cd sdk/unity/Tools~/CYG.Lsp
dotnet build -c Release
```

Then install the extension's dependencies and launch an Extension Development Host:

```bash
cd sdk/unity/Tools~/CYG.VSCode
npm install
code --extensionDevelopmentPath="$PWD" path/to/your/project
```

Use **CYG: Restart Language Server** from the Command Palette after rebuilding the
server.

## Commands

| Command | Description |
| --- | --- |
| `CYG: Restart Language Server` | Restart the language client and server process. |

## Settings

| Setting | Default | Description |
| --- | --- | --- |
| `cyg.lsp.serverPath` | `""` | Explicit path to `CYG.Lsp.dll`. |
| `cyg.lsp.dotnetPath` | `dotnet` | `dotnet` executable used to launch the server. |

## Packaging

```bash
npm install
npm run build:server
npx @vscode/vsce package --allow-missing-repository
```
