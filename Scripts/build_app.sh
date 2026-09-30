#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# The SwiftPM product keeps its internal name; the shipped bundle is OpenNaga.
APP_BUNDLE="$PROJECT_ROOT/OpenNaga.app"
CONFIGURATION="${CONFIGURATION:-release}"
DEV_IDENTITY="NagaController Dev"
if [[ -z "${SIGNING_IDENTITY:-}" ]]; then
  # A stable identity keeps TCC grants across rebuilds; see Scripts/make_dev_certificate.sh.
  if security find-identity -v -p codesigning 2>/dev/null | grep -Fq "\"$DEV_IDENTITY\""; then
    SIGNING_IDENTITY="$DEV_IDENTITY"
  else
    SIGNING_IDENTITY="-"
  fi
fi

printf 'Building OpenNaga (%s)...\n' "$CONFIGURATION"
swift build --package-path "$PROJECT_ROOT" -c "$CONFIGURATION" --product NagaController
BIN_DIR="$(swift build --package-path "$PROJECT_ROOT" -c "$CONFIGURATION" --show-bin-path)"

STAGING="$(mktemp -d "$PROJECT_ROOT/.build/app-stage.XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT
STAGED_APP="$STAGING/OpenNaga.app"
mkdir -p "$STAGED_APP/Contents/MacOS" "$STAGED_APP/Contents/Resources"
cp "$BIN_DIR/NagaController" "$STAGED_APP/Contents/MacOS/OpenNaga"
cp -R "$PROJECT_ROOT/Resources/." "$STAGED_APP/Contents/Resources/"
cp "$PROJECT_ROOT/Resources/Info.plist" "$STAGED_APP/Contents/Info.plist"
plutil -lint "$STAGED_APP/Contents/Info.plist"
ICONSET="$STAGING/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$PROJECT_ROOT/dmg-assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$PROJECT_ROOT/dmg-assets/AppIcon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$STAGED_APP/Contents/Resources/AppIcon.icns"

codesign --force --sign "$SIGNING_IDENTITY" "$STAGED_APP"
codesign --verify --strict "$STAGED_APP"

if [[ -e "$APP_BUNDLE" ]]; then
  mv "$APP_BUNDLE" "$STAGING/previous.app"
fi
if ! mv "$STAGED_APP" "$APP_BUNDLE"; then
  if [[ -e "$STAGING/previous.app" ]]; then
    mv "$STAGING/previous.app" "$APP_BUNDLE"
  fi
  exit 1
fi
printf '\nBuilt: %s\n' "$APP_BUNDLE"
if [[ "$SIGNING_IDENTITY" == "-" ]]; then
  printf 'Local ad-hoc signature. This build is not notarized.\n'
  printf 'Permissions must be granted again after every rebuild. Run Scripts/make_dev_certificate.sh once to avoid this.\n'
else
  printf 'Signed with "%s". Not notarized.\n' "$SIGNING_IDENTITY"
fi
printf 'Launch: open "%s"\n' "$APP_BUNDLE"
