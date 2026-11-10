#!/bin/bash
# Renders each factory preset over our synthetic scene and saves small thumbnails for the
# panel's preset browser (panel/presets/thumbnails/<name>.png). Needs ffmpeg for resizing.
#
#   bash scripts/make-thumbnails.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$ROOT/scripts/env.sh"
CLI="$CARGO_TARGET_DIR/release/raster-cli"
OUT="$ROOT/panel/presets/thumbnails"
WORK="$(mktemp -d /tmp/raster-thumbs.XXXXXX)"
trap 'rm -rf "$WORK"' EXIT

(cd "$ROOT" && cargo build -p raster_cli --release --quiet)
"$CLI" scene "$WORK/scene.png" 960x540
(cd "$ROOT" && "$CLI" presets "$WORK/scene.png" panel/presets/factory.json "$WORK/full" --time 1.3)
mkdir -p "$OUT"
for f in "$WORK"/full/*.png; do
  ffmpeg -v error -y -i "$f" -vf "scale=192:108:flags=area" "$OUT/$(basename "$f")"
done
echo "Thumbnails written to $OUT"
