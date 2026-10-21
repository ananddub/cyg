#!/usr/bin/env bash
set -euo pipefail

# Copies the original Sports Arena art into an existing Unity project and
# installs the CYG-native port scripts. The original sample is never modified.
#
# Usage:
#   ./setup.sh [quantum-sample-path] [unity-project-path]
#
# The Unity project must already reference com.cookyourgame.sdk.

SAMPLE="${1:-/home/das/Downloads/quantum-sports-arena-brawler-3.0.9}"
PROJECT="${2:-/mnt/SSD/Devloper/unity/sports_arena_cyg}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
UNITY="${UNITY:-/mnt/SSD/unity/editor_hub/6000.6.2f1/Editor/Unity}"

if [[ ! -d "$SAMPLE/Assets/SportsArenaBrawler/_Prefabs" ]]; then
    echo "Sports Arena sample not found: $SAMPLE" >&2
    exit 1
fi
if [[ ! -f "$PROJECT/Packages/manifest.json" ]]; then
    echo "Unity project not found: $PROJECT" >&2
    exit 1
fi
if ! grep -q 'com.cookyourgame.sdk' "$PROJECT/Packages/manifest.json"; then
    echo "Add com.cookyourgame.sdk to $PROJECT/Packages/manifest.json first." >&2
    exit 1
fi

DEST="$PROJECT/Assets/SportsArenaCyg"
mkdir -p "$DEST/Resources" "$DEST/Scripts" "$DEST/Editor" "$DEST/Tests"

# Art is large and contains prefabs/materials/models. Scripts and scenes from
# the reference sample are intentionally excluded.
if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete \
        --exclude 'Scripts' --exclude 'Scenes' \
        "$SAMPLE/Assets/SportsArenaBrawler/" "$DEST/Art/"
else
    rm -rf "$DEST/Art"
    cp -a "$SAMPLE/Assets/SportsArenaBrawler" "$DEST/Art"
    rm -rf "$DEST/Art/Scripts" "$DEST/Art/Scenes"
fi

if [[ ! -d "$PROJECT/Assets/TextMesh Pro" ]]; then
    cp -a "$SAMPLE/Assets/TextMesh Pro" "$PROJECT/Assets/"
fi

cp "$SAMPLE/Assets/referenceUser/Resources/DB/_Player/Player.prefab" \
    "$DEST/Resources/Player.prefab"
cp "$SAMPLE/Assets/referenceUser/Resources/DB/Ball/Ball.prefab" \
    "$DEST/Resources/Ball.prefab"
cp "$SAMPLE/Assets/SportsArenaBrawler/_Prefabs/Environment/Environment.prefab" \
    "$DEST/Resources/Environment.prefab"

cp -a "$HERE/Assets/SportsArenaCyg/Scripts/." "$DEST/Scripts/"
cp -a "$HERE/Assets/SportsArenaCyg/Editor/." "$DEST/Editor/"
cp -a "$HERE/Assets/SportsArenaCyg/Tests/." "$DEST/Tests/"

if [[ -x "$UNITY" ]]; then
    "$UNITY" -batchmode -nographics -quit \
        -projectPath "$PROJECT" \
        -executeMethod CookYourGame.SportsArena.Editor.SportsArenaSceneBuilder.CreateBatch \
        -logFile -
else
    echo "Unity not found at $UNITY; open the project and run"
    echo "  CookYourGame > Sports Arena > Create Play Scene"
fi

echo "Sports Arena port installed at $DEST"
