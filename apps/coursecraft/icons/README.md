# CourseCraft App Icons

## macOS Icon Assets

- `coursecraft.svg` — Light appearance vector source.
- `coursecraft-dark.svg` — Dark appearance vector source.
- `CourseCraft.icon/` — Clean-room Icon Composer bundle (`icon.json` + light/dark foreground SVGs in `Assets/`) for macOS 26+ Liquid Glass appearances (Default/Light, Dark, Clear, and Tinted).
- `Assets.car` — Pre-compiled asset catalog built from `CourseCraft.icon/` using `xcrun actool`.
- `CourseCraft.icns` — Legacy icon file kept alongside `Assets.car` (with `CFBundleIconFile`) so macOS 15 and earlier continue to render the teal icon unchanged.

## Why `Assets.car` Is Committed

The GitHub Actions release workflow runs on `macos-15`, whose older Xcode toolchain cannot compile `.icon` bundles. Committing `Assets.car` lets `scripts/package-macos.sh` bundle it directly into `Contents/Resources/Assets.car` (with `CFBundleIconName = CourseCraft`) on any runner.

## Regenerating `Assets.car`

On macOS 26+ with Xcode 26+ installed, run from the repository root (compiling into a temporary directory so `CourseCraft.icns` is not overwritten):

```sh
tmp=$(mktemp -d)
xcrun actool \
  --compile "$tmp" \
  --platform macosx \
  --minimum-deployment-target 11.0 \
  --app-icon CourseCraft \
  --enable-icon-stack-fallback-generation=disabled \
  --output-partial-info-plist "$tmp/partial.plist" \
  apps/coursecraft/icons/CourseCraft.icon
cp "$tmp/Assets.car" apps/coursecraft/icons/Assets.car
rm -rf "$tmp"
```

Verify the compiled catalog contains both Light and Dark foreground vectors and distinct appearance renditions:

```sh
xcrun assetutil --info apps/coursecraft/icons/Assets.car | grep -E '"AssetType"|"Appearance"|"Name"|"LayerOpacity"'
```
