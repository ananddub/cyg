# tree-sitter-cyg

Tree-sitter grammar for the CYG deterministic simulation schema language. It
mirrors the compiler grammar in `Tools~/CYG.Dsl.Core`.

## Generate and check

```bash
tree-sitter generate
tree-sitter parse sdk/unity/Samples/CYG/arena.cyg
tree-sitter query queries/highlights.scm sdk/unity/Samples/CYG/arena.cyg
```

The grammar accepts the full surface language: `namespace`, `asset`, `struct`,
`component`, `enum`, `flags`, `union`, `input`, `global`, `event`,
`synced_event`, and `signal`, including every builtin, `QString<N>`,
`asset_ref<T>`, and the fixed `array`/`list`/`dictionary` containers.

Queries:

| File | Consumer |
| --- | --- |
| `queries/highlights.scm` | Syntax highlighting |
| `queries/outline.scm` | Code outline / breadcrumbs |
| `queries/indents.scm` | Auto-indentation |
| `queries/brackets.scm` | Rainbow and matching brackets |

The Zed extension keeps copies of these queries in
`Tools~/CYG.Zed/languages/cyg/`.
