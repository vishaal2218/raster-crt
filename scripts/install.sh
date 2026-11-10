#!/bin/bash
# One step: build Raster CRT, install the effect (macOS asks for your password once) and the
# panel, and refresh the preset thumbnails. Quit Premiere Pro first, then reopen it after.
#
#   bash scripts/install.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$ROOT/scripts/env.sh"

if pgrep -x "Adobe Premiere Pro 2026" >/dev/null || pgrep -x "Adobe After Effects 2026" >/dev/null; then
  echo "Please quit Premiere Pro and After Effects first (they keep the old effect loaded)." >&2
  exit 1
fi

echo "1/4  Testing"
(cd "$ROOT" && cargo test --workspace --quiet)
(cd "$ROOT" && node --test tests/panel/ >/dev/null)

echo "2/4  Building the effect"
(cd "$ROOT/effect" && just release >/dev/null)

echo "3/4  Installing the effect and the panel"
bash "$ROOT/scripts/install-dev.sh" release
bash "$ROOT/scripts/install-panel.sh"

echo "4/4  Preset thumbnails"
bash "$ROOT/scripts/make-thumbnails.sh" >/dev/null

echo "Done. Open Premiere Pro, then Window > Extensions > Raster CRT."
