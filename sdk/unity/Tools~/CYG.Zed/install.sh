#!/usr/bin/env bash
# Installs the CYG language server for Zed and prepares the local grammar repo.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"
grammar_src="$repo_root/sdk/unity/Tools~/tree-sitter-cyg"
lsp_project="$repo_root/sdk/unity/Tools~/CYG.Lsp/CYG.Lsp.csproj"

install_dir="${CYG_LSP_HOME:-$HOME/.local/share/cyg-lsp}"
bin_dir="$HOME/.local/bin"
grammar_dir="$HOME/.local/share/cyg/tree-sitter-cyg"

if ! command -v dotnet >/dev/null 2>&1; then
  echo "error: dotnet was not found on PATH; install the .NET 10 SDK first." >&2
  exit 1
fi

echo "==> Publishing CYG.Lsp to $install_dir"
mkdir -p "$install_dir"
dotnet publish "$lsp_project" -c Release -o "$install_dir" >/dev/null

echo "==> Writing $bin_dir/cyg-lsp"
mkdir -p "$bin_dir"
cat > "$bin_dir/cyg-lsp" <<'EOF'
#!/bin/sh
exec dotnet "${CYG_LSP_HOME:-$HOME/.local/share/cyg-lsp}/CYG.Lsp.dll" "$@"
EOF
chmod +x "$bin_dir/cyg-lsp"

echo "==> Syncing grammar to $grammar_dir"
mkdir -p "$grammar_dir"
cp -R "$grammar_src/." "$grammar_dir/"
cd "$grammar_dir"
if [ ! -d .git ]; then
  git init -q -b main
fi
git add -A
if ! git diff --cached --quiet; then
  git -c user.name="CYG Dev" -c user.email="dev@cookyourgame.local" \
    commit -q -m "Sync CYG Tree-sitter grammar"
fi
grammar_rev="$(git rev-parse HEAD)"

echo "==> Pinning grammar revision $grammar_rev"
sed -i "s|^rev = .*|rev = \"$grammar_rev\"|" "$script_dir/extension.toml"

echo
echo "Done. Add $bin_dir to PATH if it is missing, then in Zed run:"
echo "  zed: extensions -> Install Dev Extension -> $script_dir"
