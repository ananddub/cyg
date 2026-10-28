#!/usr/bin/env bash
# Local release builder — GitHub CI/CD ke bina, tumhare system pe.
#
# Kya karta hai:
#   1. Web console build (dashboard/dist — binary me embed hota hai)
#   2. cyg-gaming release build (is machine ke OS/arch ke liye)
#   3. Tar.gz archive banao (binary + README + LICENSE)
#   4. Windows cross-build AGAR toolchain installed hai (mingw-w64), warna skip
#   5. GitHub release create ka READY COMMAND print karo — chalana TUMHE hai
#
# Use:  ./scripts/release-local.sh [version]     (default: v0.1.0-local)
set -euo pipefail
VERSION="${1:-v0.1.0-local}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/release-out"
mkdir -p "$OUT"

echo "━━━ 1/4 Web console (dashboard/dist) ━━━━━━━━━━━━━━━━━━━━━━━━"
cd "$ROOT/dashboard"
if [ ! -d node_modules ]; then npm install --no-audit --no-fund; fi
npm run build

echo "━━━ 2/4 cyg-gaming release build (native) ━━━━━━━━━━━━━━━━━━━"
cd "$ROOT"
cargo build --release --bin cyg-gaming
BIN="target/release/cyg-gaming"
[ -f "$BIN" ] || { echo "✗ build failed"; exit 1; }

echo "━━━ 3/4 Archives ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
TAG_NAME="cyg-gaming-$(cargo metadata --no-deps --format-version 1 2>/dev/null | grep -o '"name":"[^"]*"' | head -1 | cut -d'"' -f4 2>/dev/null || echo x86_64-linux)"
HOST_TRIPLE="$(rustc -vV | grep host | cut -d' ' -f2)"
NAME="cyg-gaming-$HOST_TRIPLE"
rm -rf "$OUT/$NAME" "$OUT/$NAME.tar.gz"
mkdir -p "$OUT/$NAME"
cp "$BIN" "$OUT/$NAME/"
cp README.md LICENSE "$OUT/$NAME/"
tar czf "$OUT/$NAME.tar.gz" -C "$OUT" "$NAME"
echo "✓ $OUT/$NAME.tar.gz"

echo "━━━ 4/4 Windows cross-build (optional) ━━━━━━━━━━━━━━━━━━━━━"
if rustup target list --installed | grep -q x86_64-pc-windows-gnu && command -v x86_64-w64-mingw32-gcc > /dev/null; then
    cargo build --release --bin cyg-gaming --target x86_64-pc-windows-gnu
    WNAME="cyg-gaming-x86_64-pc-windows-gnu"
    rm -rf "$OUT/$WNAME" "$OUT/$WNAME.zip"
    mkdir -p "$OUT/$WNAME"
    cp "target/x86_64-pc-windows-gnu/release/cyg-gaming.exe" "$OUT/$WNAME/"
    cp README.md LICENSE "$OUT/$WNAME/"
    (cd "$OUT" && zip -q -r "$WNAME.zip" "$WNAME")
    echo "✓ $OUT/$WNAME.zip"
else
    echo "— skip (mingw-w64 nahi hai; chaaho to: sudo dnf install mingw64-gcc && rustup target add x86_64-pc-windows-gnu)"
    echo "  (Windows builds GitHub Actions pe bhi the — release.yml wapas laa sakte ho)"
fi

echo
echo "━━━ Sab ready: $OUT ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "GitHub release TUM chalaoge (jab version final ho):"
echo
echo "  gh release create $VERSION $OUT/*.tar.gz $OUT/*.zip \\"
echo "      --title \"$VERSION\" --generate-notes"
echo
echo "  (gh nahi hai to https://github.com/ananddub/cyg/releases/new pe"
echo "   archives manually drag kar do — same baat hai.)"
