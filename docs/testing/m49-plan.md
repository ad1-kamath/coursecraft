# M49: Animations on slide masters, groups that animate as one, and the dark app icon

Goal: close two animation gaps that Luca (a contributor) reported, and ship the macOS dark app icon. Ship as the next release together with the matching-dropdown fix already on main (merge 0eb8501).

Luca's words: "CourseCraft master slides do not have animations, while other tools do. And group animations: if you apply the animation on the group it should merge and animate. Now it applies it to the children. It is not 100% the same result."

Rules for every step: AGENTS.md golden rules (never crash, everything is a command, one runtime in `coursecraft-logic`, layering, wasm stays working, clean-room wording: describe behaviour in our own words and never copy another product's names, UI or look; `tl!` for every user-facing string). Runtime behaviour changes update `docs/spec/triggers.md` in the same step; model changes update `docs/spec/course-model.md`. Editor UI changes follow `docs/ui/` and update its status tables (`ribbon.md` or `contextual-tabs.md`, and `feature-parity.md` with its summary counts). Add a `CHANGELOG.md` line for each user-visible change under a `## Unreleased` heading above `## 0.6.6` (create it if missing), in plain words, no internal ids; credit "Thanks to Luca" on the two animation lines. New model fields are optional and omitted from JSON when unset, so existing courses and `simulate` output stay byte-identical. Every step adds tests (logic/render unit tests, and headless egui_kittest UI tests for editor changes, like the existing `*_tests.rs` files in `crates/ui-egui/src`).

## Steps

1. **Groups animate as one unit.**
   Find how an entrance, exit or emphasis animation is applied when the selected object is a group today (Animations ribbon, the Animate inspector, `object.set_animation`, `object.add_emphasis`, the animation painter, and PowerPoint import in `crates/pptx/src/timing.rs`). Today it is copied onto, or played by, each child separately, so a group that flies in, zooms, spins, wipes or fades looks wrong: each child scales and turns around its own centre, wipes reveal each child separately, and overlapping children show through each other while fading.
   - A group can carry its own `Animation` (entrance, exit, emphasis), stored on the group. When set, the whole group plays it as one picture: one shared frame (the group's bounding box) for the motion, scale and turn centre, the wipe/clip/shape reveal, and the sequence; one opacity for the whole group so overlapping children do not show through each other mid-fade (render the group into one offscreen layer and fade that, if the painter can; if that is not possible on every target, say why in the report and pick the closest correct result, but shared frame for motion/scale/turn/clip is required).
   - Children keep their own animations too; a child's animation plays inside the group's (the group's transform and opacity apply on top). Timing: the group's animation uses the group's timing window.
   - Applying an animation with a group selected sets it on the group, not on the children. Existing courses where children already carry copies keep playing exactly as before (no migration that changes what plays).
   - Editor: the timeline shows the group's animation on the group's bar; the canvas animation preview (`anim_preview.rs`) plays the group as one; the Animate inspector edits the group's animation when the group is selected.
   - Player, editor preview and web export must agree (one runtime; the renderer in `crates/render` draws it).
   - Exit animations and the "finished exit" rule for interactive groups (`docs/spec/triggers.md`, liveness) must use the group's exit too.
   - Tests: a group of two overlapping shapes with a zoom entrance scales around the group centre (pixel or draw-item probes at 50% progress); a fade at 50% shows no darker overlap area; a wipe reveals across the group's box, not per child; a child with its own fly-in inside a group with a fade gets both; an old course with per-child animations renders the same draw items as before; undo/redo of setting a group animation.

2. **Objects on a slide master can animate.**
   Today `docs/spec/course-model.md` ("Slide masters and templates") says master objects "ignore timing and animation". Change that so master objects can have entrance, exit and emphasis animations and timing, like slide objects.
   - Behaviour (write it into the spec in our own words): each time a slide that uses the master starts (including when revisited and reset), its master objects play their animations on that slide's timeline, from time 0, with their own start times and durations. Pick and document what happens when moving between two slides that use the same master: recommended default is that master objects replay on every slide (simple and predictable); if you think a "do not replay when the next slide uses the same master" option is needed, add it as an optional master setting, default off.
   - Master objects still never take a press, never join the keyboard focus list, add no triggers and no states. Their animations add no events. A master object's exit hides it until the slide ends.
   - Editor: in Master View, the Animations tab, the Animate inspector and the timeline work on master objects (today they may be disabled there; enable them). The canvas animation preview works in Master View.
   - Player, editor preview and web export must agree. Check the slide transition still looks right when master objects animate.
   - Validation: keep the existing master rules (no triggers, layers, question, audio, video, Lottie).
   - Tests: a master text box with a 500 ms fade entrance is invisible at t=0 and fully visible at t=600 on two different slides using that master; revisiting a slide replays it; a master object with an exit is gone after its exit; master objects still can't be clicked or focused; an old course without master animations renders exactly as before; editor test that the Animations tab is enabled in Master View and sets an animation on a master object.

3. **macOS dark app icon (packaging).**
   The artwork is already committed: `apps/coursecraft/icons/coursecraft-dark.svg` and `coursecraft-dark-1024.png`. The cause and decision are in `docs/testing/dark-icon-findings.md` (read it first). On macOS 26 and later with the Dark icon style, an app that ships only an `.icns` gets an automatic grey look; we must ship our own dark look.
   - Create an Icon Composer `.icon` bundle `apps/coursecraft/icons/CourseCraft.icon` with the existing light artwork (from `coursecraft.svg` / the current `.icns`) and the dark artwork as the dark appearance. Compile it once on this Mac with `xcrun actool` into an `Assets.car` and commit that file (the release runner is macos-15 and may lack the newest actool, so do not compile in CI).
   - `scripts/package-macos.sh`: copy `Assets.car` into `CourseCraft.app/Contents/Resources/` and add `CFBundleIconName` (`CourseCraft`) to the Info.plist it writes. Keep the `.icns` and `CFBundleIconFile` so macOS 15 and older still show the green icon. Keep the release workflow on macos-15.
   - Write the exact commands used to make `Assets.car` into `apps/coursecraft/icons/README.md` (create it) so it can be rebuilt.
   - Check: build the app bundle with the script (skip the dmg if it needs anything outside the sandbox) and confirm `Assets.car` and both plist keys are in it (`plutil -p`, `assetutil --info`). If `actool` cannot run in your environment, stop this step, leave everything else done, and say exactly what is missing in the report; do not fake the file.

4. **Gate, player build, perf and report** (driver steps).

## Owner's manual checks

- Group two overlapping shapes, give the group a Zoom entrance: in Preview the pair grows from the middle of the group as one picture; a Fade shows no darker overlap.
- In Master View, give the master's logo a Fly In: every slide using that master flies the logo in when it starts.
- After installing the build on a Mac with Dark icons: the CourseCraft icon is black with the teal C and cream outline; with Dark icons off it is green.

## Reviewer notes

- Step 1: existing courses must play exactly as before. Not acceptable: silently moving child animations onto the group, or the reverse.
- Step 1: performance. An offscreen layer per animating group is fine while it animates; it must not be allocated every frame for groups that are not animating.
- Step 2: master objects must stay non-interactive. Check the 300-slide case does not slow down (no per-frame scan of all slides).
- Step 3: never commit a placeholder or empty `Assets.car`. The light icon on macOS 15 must be unchanged.
