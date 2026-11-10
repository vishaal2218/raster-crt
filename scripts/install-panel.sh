#!/bin/bash
# Links the Raster CRT panel into this user's CEP extensions folder (no password needed).
# PlayerDebugMode must be 1 (it is on this Mac). Restart Premiere afterwards, then open
# Window > Extensions > Raster CRT.
#
#   bash scripts/install-panel.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PANEL="$ROOT/panel"
DEST="$HOME/Library/Application Support/Adobe/CEP/extensions/com.rastercrt.panel"

if [ -L "$DEST" ]; then
  current="$(readlink "$DEST")"
  if [ "$current" != "$PANEL" ]; then
    echo "$DEST already links to $current; remove it first." >&2
    exit 1
  fi
  echo "Panel already linked: $DEST -> $PANEL"
elif [ -e "$DEST" ]; then
  echo "$DEST exists and is not our link; leaving it alone." >&2
  exit 1
else
  ln -s "$PANEL" "$DEST"
  echo "Linked: $DEST -> $PANEL"
fi
echo "Restart Premiere Pro, then open Window > Extensions > Raster CRT."
