#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"

IDEA_HOME="${IDEA_HOME:-${RUSTROVER_HOME:-$HOME/Documents/RustRover-2026.2.3}}"
JAVA_HOME="${JAVAC_HOME:-$IDEA_HOME/jbr}"

if [[ ! -x "$JAVA_HOME/bin/javac" ]]; then
    echo "javac not found at $JAVA_HOME/bin/javac" >&2
    echo "Set IDEA_HOME to a 2026.2+ IDE installation (or JAVA_HOME to a JDK 21+)." >&2
    exit 1
fi

if [[ ! -d "$IDEA_HOME/lib" ]]; then
    echo "IntelliJ platform jars not found at $IDEA_HOME/lib" >&2
    exit 1
fi

build_dir="$script_dir/build"
classes_dir="$build_dir/classes"
stage="$build_dir/plugin/cyg"

rm -rf "$build_dir"
mkdir -p "$classes_dir" "$stage/lib" "$stage/server"

echo "Publishing CYG.Lsp..."
dotnet publish "$repo_root/sdk/unity/Tools~/CYG.Lsp/CYG.Lsp.csproj" \
    --configuration Release \
    --output "$stage/server" \
    --nologo \
    --verbosity quiet

echo "Compiling plugin..."
find "$script_dir/src/main/java" -name '*.java' > "$build_dir/sources.txt"
"$JAVA_HOME/bin/javac" \
    --release 21 \
    -Xlint:-options \
    -classpath "$IDEA_HOME/lib/*" \
    -d "$classes_dir" \
    @"$build_dir/sources.txt"

cp -r "$script_dir/resources/icons" "$classes_dir/"
mkdir -p "$classes_dir/META-INF"
cp "$script_dir/resources/META-INF/plugin.xml" "$classes_dir/META-INF/plugin.xml"
(cd "$classes_dir" && zip -q -r "$stage/lib/cyg-jetbrains.jar" .)

version="$(sed -n 's:.*<version>\(.*\)</version>.*:\1:p' "$script_dir/resources/META-INF/plugin.xml" | head -n 1)"
zip_path="$build_dir/CYG-$version.zip"
(cd "$build_dir/plugin" && zip -q -r "$zip_path" cyg)

echo "Built $zip_path"
du -h "$zip_path"
