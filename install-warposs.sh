#!/bin/bash
set -e

BUNDLE_SRC="/Users/jaredscar/Desktop/Development/warp/target/release/bundle/osx/WarpOss.app"
INSTALL_DEST="/Applications/WarpOss.app"

if [ ! -d "$BUNDLE_SRC" ]; then
  echo "Error: Bundle not found at $BUNDLE_SRC"
  exit 1
fi

echo "Killing any running WarpOss instance..."
pkill -x warp-oss 2>/dev/null || true
sleep 1

echo "Removing old installation..."
rm -rf "$INSTALL_DEST"

echo "Installing WarpOss.app..."
cp -R "$BUNDLE_SRC" "$INSTALL_DEST"

echo ""
echo "Done! Binary: $(ls $INSTALL_DEST/Contents/MacOS/)"
echo "Installed at: $INSTALL_DEST"
echo ""
echo "You can now open WarpOss from /Applications or Spotlight."
