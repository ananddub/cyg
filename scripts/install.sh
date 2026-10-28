#!/bin/sh
# CYG installer — downloads the latest release binary for your platform.
#
#   curl -fsSL https://raw.githubusercontent.com/ananddub/cyg/main/scripts/install.sh | sh
#
# Override the repo (forks):  CYG_REPO=you/cyg sh install.sh
# Override the install dir:   CYG_INSTALL_DIR=/usr/local/bin sh install.sh
set -eu

REPO="${CYG_REPO:-ananddub/cyg}"
DEST="${CYG_INSTALL_DIR:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux) ;;
    Darwin) ;;
    CYG_NT*|MINGW*)
        echo "Windows pe ye one-liner nahi chalega. PowerShell me chalao:"
        echo ""
        echo "  irm https://github.com/$REPO/releases/latest/download/cyg-gaming-x86_64-pc-windows-msvc.zip -OutFile cyg.zip"
        echo "  Expand-Archive cyg.zip -DestinationPath ."
        echo ""
        echo "  phir cyg-gaming.exe ko PATH me daal do."
        exit 1
        ;;
    *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH_PART="x86_64" ;;
    arm64|aarch64) ARCH_PART="aarch64" ;;
    *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac

if [ "$OS" = "Linux" ]; then
    if [ "$ARCH_PART" != "x86_64" ]; then
        echo "Linux $ARCH binary abhi release me nahi hai (x86_64 available). Issue kholo: github.com/$REPO/issues" >&2
        exit 1
    fi
    TRIPLE="x86_64-unknown-linux-gnu"
else
    TRIPLE="aarch64-apple-darwin"
    [ "$ARCH_PART" = "x86_64" ] && TRIPLE="x86_64-apple-darwin"
fi

ASSET="cyg-gaming-$TRIPLE.tar.gz"
URL="https://github.com/$REPO/releases/latest/download/$ASSET"

echo "→ Downloading $ASSET …"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL "$URL" -o "$TMP/$ASSET"

tar xzf "$TMP/$ASSET" -C "$TMP"
mkdir -p "$DEST"
mv "$TMP/cyg-gaming-$TRIPLE/cyg-gaming" "$DEST/cyg-gaming"
chmod +x "$DEST/cyg-gaming"

case ":$PATH:" in
    *":$DEST:"*) ;;
    *)
        echo ""
        echo "⚠️  $DEST PATH me nahi hai. Add karo:"
        echo "   export PATH=\"$DEST:\$PATH\"   (~/.bashrc ya ~/.zshrc me bhi)"
        echo ""
        ;;
esac

echo ""
echo "✓ cyg-gaming installed → $DEST/cyg-gaming"
echo ""
echo "Ab 2-minute setup:"
echo "  cyg-gaming init my-game && cd my-game"
echo "  cyg-gaming run"
echo ""
echo "Docs: https://github.com/$REPO#readme"
