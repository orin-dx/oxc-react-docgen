#!/bin/bash
# Pull real .d.ts files from npm for fixture testing
# Run: bash fixtures/update-fixtures.sh

set -e
TMPDIR=$(mktemp -d)

pull_dts() {
  local pkg="$1"
  local file="$2"
  local dest="$3"
  npm pack "$pkg" --pack-destination="$TMPDIR" --silent
  TARBALL=$(ls "$TMPDIR"/*.tgz | head -1)
  tar -xzf "$TARBALL" -C "$TMPDIR" "package/$file" 2>/dev/null
  cp "$TMPDIR/package/$file" "$dest"
  rm -rf "$TMPDIR"/*.tgz "$TMPDIR"/package
}

echo "Pulling Radix UI Button..."
pull_dts "@radix-ui/react-button@latest" "dist/index.d.ts" "fixtures/radix/button.d.ts"

echo "Pulling MUI Button..."
pull_dts "@mui/material@latest" "Button/Button.d.ts" "fixtures/mui/Button.d.ts"

echo "Done. Commit the updated fixtures."
