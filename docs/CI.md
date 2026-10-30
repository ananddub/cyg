# CI

`just ci` and `.github/workflows/ci.yml` run the same cargo gates in the same
order: clippy (`-D warnings`), `check --workspace --all-targets`,
`test --workspace`, then a release build guarded by a 150 MiB binary budget.

Two things the pipeline does that are easy to trip over:

- **`dashboard/dist` must exist before cargo builds.** `engine-api` embeds it at
  compile time, so the `dashboard` job builds it first and the `rust` job
  downloads it as an artifact. Locally: `just dashboard`.
- **Formatting is a ratchet, not a tree-wide gate.** CI runs `rustfmt --check`
  only on the `.rs` files a change touches. The tree predates the current
  rustfmt defaults and still has a large backlog; `just fmt` fixes everything at
  once, which is a commit that touches every crate, so it is a deliberate
  decision rather than a routine one.

Toolchain: pinned to `1.99.0` in both workflows (`RUST_TOOLCHAIN`). Bump it in
its own commit so a rustfmt or clippy change is attributable.

`release.yml` builds the four archives `scripts/install.sh` looks for
(`cyg-gaming-<triple>.tar.gz` plus a Windows `.zip`), each with a `.sha256`
sidecar, and publishes them to the tag's GitHub release.

## Known blocker

`[patch.crates-io] auto-di = { path = "../../rust/testing_dep/crates/auto-di" }`
in the root `Cargo.toml` points at a directory outside this repository, so every
cargo job on a fresh runner fails to resolve dependencies before it starts. Both
workflows detect it and stop with that message. It needs one of:

- vendoring `auto-di` (and `auto-di-macros`) under `crates/auto/` like the other
  `auto_*` crates, then deleting the patch, or
- publishing `auto-di` and depending on the released version.

Until then CI documents the build instead of proving it.
