#!/bin/sh
# Wraps the desktop binary in CourseCraft.app and a .dmg.
# usage: package-macos.sh <version> <binary> <output folder>
set -eu
version=$1
binary=$2
out=$3

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/CourseCraft.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out"
cp "$binary" "$app/Contents/MacOS/CourseCraft"
cp "$(dirname "$0")/../apps/coursecraft/icons/CourseCraft.icns" "$app/Contents/Resources/CourseCraft.icns"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>CourseCraft</string>
  <key>CFBundleDisplayName</key><string>CourseCraft</string>
  <key>CFBundleIdentifier</key><string>org.coursecraft.CourseCraft</string>
  <key>CFBundleExecutable</key><string>CourseCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>CourseCraft</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>CourseCraft records narration for your slides.</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
</dict>
</plist>
PLIST
# An ad-hoc signature: not a verified developer, but Apple Silicon won't run a bundle without one.
codesign --force --deep --sign - "$app"
ln -s /Applications "$work/Applications"
hdiutil create -quiet -volname CourseCraft -srcfolder "$work" -ov -format UDZO "$out/CourseCraft-$version-macos.dmg"
