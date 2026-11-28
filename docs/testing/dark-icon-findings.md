# Why CourseCraft Shows a Dark Grey Icon on macOS 27.2

## Root cause

**This is macOS 26/27's automatic Dark-icon conversion, not local build or Launch Services cache confusion.**

Two things combine to cause it:

1. **This Mac has Dark icons turned on:** `defaults read -g` shows `AppleIconAppearanceTheme = RegularDark` (System Settings → Appearance → Icon & widget style → **Dark**).
2. **`CourseCraft.app` only ships a flat legacy `.icns` file** (`Contents/Resources/CourseCraft.icns` via `CFBundleIconFile = CourseCraft`), with no compiled asset catalog (`Assets.car`) or `CFBundleIconName` providing a Dark appearance variant.

On macOS 26 (Tahoe) and macOS 27, when Icon & widget style is set to **Dark**, IconServices automatically converts legacy `.icns`-only icons that lack a Dark variant. For CourseCraft, macOS segments out the uniform teal (`#12a594`) rounded square and replaces it with a dark grey/black squircle, leaving the cream (`#f1ebdd`) "C" on top. Because the play triangle and the "C" outline are near-black (`#141414`), they blend into the dark grey background and nearly vanish except for faint Liquid Glass edge highlights.

---

## Who is affected

- **Not just this owner's Mac** — local builds and Launch Services are **not** the cause.
- **Affected:** Every user on **macOS 26 (Tahoe) or macOS 27** whose **System Settings → Appearance → Icon & widget style** is set to **Dark** (including fresh installs on machines that have never built CourseCraft). Users on **Clear** or **Tinted** icon styles also get macOS's automatic fallback styling instead of an intentional dark/layered icon.
- **Not affected:** Users on **macOS 15 (Sequoia) or earlier**, and users on macOS 26/27 whose Icon & widget style is left at **Default** (Light). They see the teal icon.

---

## Evidence

### 1. Every place the app icon comes from in the repo and installed bundles

- **Packaging script (`scripts/package-macos.sh`):** Copies `apps/coursecraft/icons/CourseCraft.icns` to `Contents/Resources/CourseCraft.icns` and writes `CFBundleIconFile = CourseCraft` to `Info.plist`. No `CFBundleIconName`, no `Assets.car`, no `.icon` bundle.
- **Release workflow (`.github/workflows/release.yml`):** Runs on `macos-15`, builds arm64 and x86_64 binaries, and calls `scripts/package-macos.sh`. Does not run `actool`.
- **Desktop window icon (`apps/coursecraft/src/main.rs:3749-3754`):** Loads `icons/window-256.png` only under `#[cfg(not(target_os = "macos"))]`, so macOS uses only the app bundle's icon. (`apps/coursecraft/build.rs` embeds `coursecraft.ico` only on Windows; `apps/coursecraft/src/updates.rs` uses `window-256.png` only inside the in-app update dialog.)
- **`.icns` contents:** Extracted `apps/coursecraft/icons/CourseCraft.icns` with `iconutil -c iconset -o /private/tmp/neon-icon/CourseCraft.iconset apps/coursecraft/icons/CourseCraft.icns`. All 10 sizes (`16x16` through `512x512@2x`, 1024 px) are the exact teal (`#12a594`) rounded square with cream (`#f1ebdd`) "C" and `#141414` play triangle/outline (matching `apps/coursecraft/icons/coursecraft.svg`).
- **Installed app and mounted DMGs have the exact same `.icns`:**
  ```sh
  shasum -a 256 apps/coursecraft/icons/CourseCraft.icns \
    /Applications/CourseCraft.app/Contents/Resources/CourseCraft.icns \
    "/Volumes/CourseCraft/CourseCraft.app/Contents/Resources/CourseCraft.icns" \
    "/Volumes/CourseCraft 1/CourseCraft.app/Contents/Resources/CourseCraft.icns"
  ```
  All four returned the identical hash `5de4b99c2ccb216ff68212be54bb40e26469fe91a56c8e0de9817760885af0de`. Both `/Applications/CourseCraft.app` (`0.6.6`) and the mounted DMGs (`0.6.5` and `0.6.6`) have `CFBundleIconFile = CourseCraft` and only `CourseCraft.icns` in `Contents/Resources/`.

### 2. Local builds & Launch Services ruled out

- `mdfind "kMDItemCFBundleIdentifier == 'org.coursecraft.CourseCraft'"` returned only `/Applications/CourseCraft.app`.
- `lsregister -dump` showed 7 entries for `org.coursecraft.CourseCraft`:
  - 4 deleted temp builds under `/private/tmp/claude-501/.../scratchpad/` (`fnfErr: File not found`),
  - 1 unmounted old DMG (`0.1.1` from `~/Downloads/CourseCraft-0.1.1-macos.dmg`),
  - 2 mounted DMGs (`0.6.5` at `/Volumes/CourseCraft`, `0.6.6` at `/Volumes/CourseCraft 1`),
  - `/Applications/CourseCraft.app` (`0.6.6`, pointing to `Contents/Resources/CourseCraft.icns`).
  *(Side note unrelated to the icon: `/Applications/CourseCraft.app` is currently the `x86_64` slice from `coursecraft-0.6.6-macos-x86_64.dmg`.)*
- Two old unmounted DMGs exist in `target/pkg-046/` and `target/pkg-050/`.
- **Why stale LS entries are not the cause:** Creating a brand-new minimal `.app` bundle in `/private/tmp/neon-icon/TestICNS.app` with an unseen bundle ID (`org.coursecraft.TestICNS`) and `CourseCraft.icns` immediately rendered the **exact same dark grey/black icon**.

### 3. System setting & direct rendering proof on this Mac (macOS 27.2, build 26B123)

- `defaults read -g` showed:
  ```text
  AppleIconAppearanceTheme = RegularDark;
  AppleInterfaceStyle = Dark;
  ```
- Rendering `/Applications/CourseCraft.app` and `/Volumes/CourseCraft 1/CourseCraft.app` directly through AppKit (`NSWorkspace.shared.icon(forFile:)`) reproduced the exact reported dark grey/black rounded square with the cream "C" and barely visible dark play triangle.
- Rendering `/Applications/CourseCraft.app` through macOS's `IconServices.framework` (`ISIcon` + `ISImageDescriptor`) across appearance modes confirmed the switch:
  - `appearance = 0` (Default/Light): **teal `#12a594` rounded square** with cream "C" and crisp black play glyph.
  - `appearance = 1` (Dark / `RegularDark`): **dark grey/black rounded square** with cream "C" and near-invisible `#141414` play glyph.
- Sibling `.icns`-only Craft apps on this Mac behave identically: `/Applications/DeckCraft.app` renders orange in `appearance = 0` and dark grey/black in `appearance = 1`.
- Contrast test: compiling a clean-room `CourseCraft.icon` bundle with Xcode 27.2's `actool` into `Assets.car` (with `fill-specializations` for `dark`) and setting `CFBundleIconName = CourseCraft` rendered bright teal in `appearance = 0` and deep dark teal in `appearance = 1` under `RegularDark`.

---

## Fix options

All options keep `CourseCraft.icns` and `CFBundleIconFile = CourseCraft` so macOS 11–15 continues to work unchanged. All assets should be built clean-room from CourseCraft's existing `apps/coursecraft/icons/coursecraft.svg`.

### Option 1: Add an Icon Composer `.icon` bundle and compile it in CI on `macos-26` (Recommended)

- **Files to add/change:**
  - Add `apps/coursecraft/icons/CourseCraft.icon/` (`icon.json` + separated foreground SVG layers from `coursecraft.svg`: the cream "C" and the play glyph).
  - Give the `dark` appearance an intentional design: either a **deep dark-teal background** (e.g. `#0c5950`) or a **dark charcoal background with a teal (`#12a594`) / cream play glyph** so the play triangle stays clearly legible instead of disappearing into dark grey.
  - Update `.github/workflows/release.yml`: change the Mac job from `runs-on: macos-15` to `runs-on: macos-26` (GitHub Actions provides `macos-26` arm64 runners with Xcode 26+, which `actool` needs to compile `.icon` bundles; x86_64 cross-compilation with `MACOSX_DEPLOYMENT_TARGET=11.0` stays the same).
  - Update `scripts/package-macos.sh`: run `xcrun actool ... --compile "$app/Contents/Resources" --platform macosx --minimum-deployment-target 11.0 --app-icon CourseCraft --output-partial-info-plist ...` to produce `Assets.car` (and fallback `.icns`, or pass `--enable-icon-stack-fallback-generation=disabled` and keep the handcrafted `CourseCraft.icns` for macOS < 26), and add `<key>CFBundleIconName</key><string>CourseCraft</string>` to `Info.plist`. Keep a graceful fallback if a local dev machine lacks Xcode 26+.
- **Trade-offs:**
  - `+` Full native macOS 26/27 support across **Default, Dark, Clear, and Tinted** Liquid Glass styles.
  - `+` Source-driven in git (small text `icon.json` + SVGs), reproducible in CI, no checked-in binary `Assets.car`.
  - `-` Requires moving the release workflow's Mac runner from `macos-15` to `macos-26` (and testing that Rust/trunk/cross-compile builds stay green on `macos-26`).

### Option 2: Commit `CourseCraft.icon` plus a pre-compiled `Assets.car`, keep CI on `macos-15`

- **Files to add/change:**
  - Add `apps/coursecraft/icons/CourseCraft.icon/` (source) and `apps/coursecraft/icons/Assets.car` (compiled once locally with `xcrun actool` on Xcode 26+/27, plus a short script/doc comment to regenerate it when the icon changes).
  - Update `scripts/package-macos.sh`: copy `Assets.car` into `Contents/Resources/` alongside `CourseCraft.icns`, and add `<key>CFBundleIconName</key><string>CourseCraft</string>` to `Info.plist`.
  - No change to `.github/workflows/release.yml`.
- **Trade-offs:**
  - `+` Identical user-facing result to Option 1 (full Light/Dark/Clear/Tinted support).
  - `+` Zero CI runner migration risk; works on `macos-15` and any local Mac regardless of Xcode version.
  - `-` Commits a binary `Assets.car` (~1–2 MB for a vector `.icon` stack) to git and requires re-running `actool` on a macOS 26+ Mac whenever the icon changes. (The repo already commits binary `.icns`, `.ico`, and `.png` icon files.)

### Option 3: Compile a classic `Assets.xcassets` (`AppIcon.appiconset`) on `macos-15`

- **Files to add/change:**
  - Add `apps/coursecraft/icons/Assets.xcassets/AppIcon.appiconset/` referencing the existing icon PNGs.
  - Run `xcrun actool` in `scripts/package-macos.sh` (supported by Xcode 16 on `macos-15`) and set `CFBundleIconName = AppIcon`.
- **Trade-offs:**
  - `+` Works on `macos-15` without committing `Assets.car`.
  - `+` In our test on macOS 27.2 (`TestXCFullRealBin.app`), having `Assets.car` + `CFBundleIconName` prevented macOS from auto-replacing the teal squircle with dark grey.
  - `-` **Does not actually support Dark, Clear, or Tinted icon styles** — the icon stays bright Light-mode teal in Dark mode (sticking out next to dark icons).
  - `-` Relies on an undocumented fallback heuristic; should be tested with an Xcode 16–compiled `Assets.car` (our local test used Xcode 27.2's `actool`), and future macOS updates could still darken legacy `AppIcon.appiconset` assets.

### Option 4: Do nothing (stay `.icns`-only)

- **Trade-offs:**
  - `+` No code or CI changes.
  - `-` Every macOS 26/27 user with Dark icons enabled sees a dark grey icon whose `#141414` play triangle is nearly invisible.

---

## Recommendation

Go with **Option 1** (clean-room `CourseCraft.icon` compiled by `actool` on the `macos-26` GitHub Actions runner) — or **Option 2** (same `CourseCraft.icon` source with a pre-compiled `Assets.car` committed) if you want an immediate fix without touching the `macos-15` release runner right now. Option 2 can be switched to Option 1 later with a 5-line CI/script edit once the release workflow moves to `macos-26`.

Either way, design the Dark variant explicitly in `CourseCraft.icon` (either a dark-teal background or a dark charcoal background with a bright teal/cream play glyph) so the play symbol remains crisp and the icon stays unmistakably CourseCraft in Dark mode.

---

## Decision (2026-11-28)

- Dark icon artwork: `apps/coursecraft/icons/coursecraft-dark.svg` and `coursecraft-dark-1024.png`. Black `#141414` background, C filled with teal `#12a594`, cream `#f1ebdd` outline at 26 units, teal play button.
- Use Option 2 (commit the compiled icon catalog, keep the release runner as it is). Ship it in the next release.
