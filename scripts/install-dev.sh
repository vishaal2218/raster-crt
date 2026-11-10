#!/bin/bash
# Installs the built Raster CRT effect into Adobe's shared plug-in folder, where both
# Premiere Pro and After Effects look for effects. macOS asks for the password in its
# own dialog; this script never sees it. Restart Premiere afterwards.
#
#   bash scripts/install-dev.sh [release|debug]
set -euo pipefail

PROFILE="${1:-release}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUNDLE="$ROOT/.build/target/$PROFILE/Raster CRT.plugin"
DEST="/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/Raster"

if [ ! -d "$BUNDLE" ]; then
  echo "Build it first: (cd effect && just $PROFILE)" >&2
  exit 1
fi

# Stage a copy under /tmp so the privileged step never reads through the .build link.
STAGE="$(mktemp -d /tmp/raster-crt-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
cp -R "$BUNDLE" "$STAGE/"

echo "Copying 'Raster CRT.plugin' to: $DEST"
osascript -e "do shell script \"mkdir -p '$DEST' && rm -rf '$DEST/Raster CRT.plugin' && cp -R '$STAGE/Raster CRT.plugin' '$DEST/'\" with prompt \"Raster CRT wants to install its effect into Adobe's plug-in folder.\" with administrator privileges"

codesign --verify --strict "$DEST/Raster CRT.plugin"
echo "Installed. Restart Premiere Pro to load it."
