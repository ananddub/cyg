# CYG for Zed

Zed extension for `.cyg` schemas.

- Tree-sitter grammar with syntax highlighting, code outline, indentation,
  bracket matching, and comment toggling.
- `cyg-lsp` language server: diagnostics, completion, hover, definition,
  references, rename, document symbols, workspace symbols, and semantic tokens.

## Install

```bash
sdk/unity/Tools~/CYG.Zed/install.sh
```

The script publishes `CYG.Lsp` to `~/.local/share/cyg-lsp`, writes a
`cyg-lsp` launcher to `~/.local/bin`, and syncs the grammar into
`~/.local/share/cyg/tree-sitter-cyg` (a local git repository that Zed can
fetch), pinning its current revision in `extension.toml`.

Then in Zed open the extensions view and run **Install Dev Extension**,
selecting this directory (`sdk/unity/Tools~/CYG.Zed`). Zed compiles the Rust
extension and Tree-sitter grammar on install.

The server is resolved in this order:

1. `cyg-lsp` on `PATH` (written by `install.sh`).
2. `dotnet sdk/unity/Tools~/CYG.Lsp/bin/Release/net10.0/CYG.Lsp.dll` or the
   `Debug` build inside the open worktree.
3. A clear error if neither exists.

## Layout

| Path | Purpose |
| --- | --- |
| `extension.toml` | Manifest, grammar repository, language server registration |
| `src/lib.rs` | Resolves the `cyg-lsp` command for Zed |
| `languages/cyg/config.toml` | File suffix, comments, brackets, tab size |
| `languages/cyg/*.scm` | Highlight, outline, indent and bracket queries |
| `install.sh` | Local setup helper |

Grammar sources live in `sdk/unity/Tools~/tree-sitter-cyg`.

## Publishing

The grammar `repository` in `extension.toml` currently points at the local
checkout created by `install.sh`. To distribute the extension, host the
grammar repository (or the `tree-sitter-cyg` directory of this repository)
publicly, point `repository`/`rev` at that remote, and include the extension
in the `zed-industries/extensions` registry.
