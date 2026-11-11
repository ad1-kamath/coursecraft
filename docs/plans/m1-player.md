# M1 plan: the player

Goal (from `docs/roadmap.md`): `examples/hello-course` plays in the browser (wasm, trunk) and on the desktop, and produces exactly the effects `coursecraft-cli simulate examples/hello-course/course.json examples/hello-course/pass.inputs.json` prints.

## Shape

```text
apps/coursecraft-player ── owns Course + Player, ticks the clock, logs effects
        │  Vec<Input>  ▲
        ▼              │ &Player (read only)
crates/render (L3) ── compose → paint → hit-test → Input
```

- **`coursecraft-render` never mutates the player.** `SlideView::show(ui, &player) -> Vec<Input>` reads state, draws, and returns the inputs that this frame's pointer events produced. The app feeds them to `Player::handle`. All play-time decisions stay in `coursecraft-logic` (golden rule 3).
- **The renderer sends, the runtime decides.** A click on any hit object becomes `Input::Click`. Disabled, hidden, timing windows and `prevent_base_clicks` are the runtime's call. The renderer never filters clicks and never changes states.
- **Presentational state only** in the renderer: hovered item and pressed item.

## `crates/render` modules

| Module | Contents | Pure? |
|---|---|---|
| `color.rs` | `parse_color(&Color) -> Option<Color32>` for `#RRGGBB` / `#RRGGBBAA`; callers fall back to a default on `None` | yes |
| `layout.rs` | `SlideTransform` from available screen rect + `slide_size`: uniform scale, centred letterbox, slide⇄screen point/rect mapping. `None` for zero/non-finite sizes | yes |
| `compose.rs` | `compose(&Player) -> Vec<DrawItem>` bottom-to-top: base objects, then question prompt + choices, then each visible layer's objects. Each item has `hit: Hit` (`Object(id)` / `Choice(id)` / `None` for the prompt), `rect` in slide coords, and a resolved `Visual` (fill, text, style, shape kind) after applying the object's current state. Skips objects outside their timing window or in state `hidden`. Non-finite or negative frames are dropped | yes |
| `paint.rs` | Draws `DrawItem`s through a `SlideTransform` with `egui::Painter` | no |
| `view.rs` | `SlideView { hovered, pressed }` and `show(ui, &Player) -> Vec<Input>` | no |
| `ticker.rs` | `Ticker::advance(dt_seconds: f32) -> Option<Input>`: accumulates real time, emits `Tick { ms }` in whole milliseconds, carries the remainder, ignores negative/non-finite dt, caps one step at 1000 ms | yes |
| `chrome.rs` | Player chrome under the slide: Previous/Next (`player.show_nav`), Menu popup listing scenes/slides → `GoTo` (`show_menu`), read-only seekbar of base clock / slide duration (`show_seekbar`), slide title. Ordinary egui widgets, outside the slide rect | no |

### Drawing rules

- Slide background white; letterbox area `#202124`.
- **Object state.** Current state = `player.object_state(id)`, else `initial_state`. A matching `StateDef` overrides `fill` and `text` (`src` is ignored in M1).
- **Text:** `style.size × scale`, `style.color`, wrapped to frame width, top-left. `bold` → draw a second copy offset by 0.6 px (default fonts have no bold face). `font` is ignored in M1.
- **Button:** rounded rect (radius 6 × scale) in fill, label centred, white, 20 px × scale.
- **Shape:** rectangle, rounded rectangle (radius 12 × scale), ellipse.
- **Image:** light grey placeholder with the `alt` (or `src`) text. Real image loading arrives with `media/` in M2.
- **Video:** dark placeholder. **Audio:** nothing drawn.
- **Hover/down visuals** (no runtime state): apply while hovered or pressed, unless the current state is `disabled`. Use the object's `hover`/`down` `StateDef` if it has one. Otherwise, for buttons only, lighten the fill 12% on hover and darken it 15% when down.

### Question layout (no frames in the model)

The prompt and choices are part of the base layer, drawn after the base objects:
- Prompt at slide rect (80, 60, w − 160, 80), 28 px, bold.
- Choice *i* at (80, 160 + 72·i, min(900, w − 160), 56). It has a marker on the left (a circle for multiple choice, a square for multiple response), filled when the choice is in `player.state().selected`, then its text at 22 px.
- Choices are in model order (`shuffle` is not implemented in the runtime yet).

### Input rules

- There is one hit path for the slide: the topmost `DrawItem` with a hit under the pointer wins.
- **Hover:** when the topmost object under the pointer changes, emit `HoverEnd { old }` then `HoverStart { new }`. Choices do not hover.
- **Click:** primary press on an item marks it pressed. Release over the same item emits `Click { object }` for an object, or `SelectChoice { choice }` for a choice. Releasing elsewhere cancels.
- Pointer events are read from `ui.input()` in order, so a press and release in one frame still click.

## `apps/coursecraft-player`

- **Desktop:** `coursecraft-player [COURSE_JSON] [--log-effects]`. The course defaults to `examples/hello-course/course.json`. `--log-effects` prints each effect as one JSON line on stdout, in the same format as `simulate`.
- **Web:** fetches `course.json` next to `index.html`, which trunk copies from the example. Each effect is logged to the browser console as a JSON line.
- **States:** Loading → Error(message) or Playing.
  - Course load and validation failures show an error screen and never panic.
  - The course is leaked with `Box::leak` to give `Player<'static>`. This is safe and keeps it to one course per process.
- **Each frame:**
  1. `Ticker` turns `stable_dt` into a `Tick`.
  2. The slide view returns its inputs, then chrome returns its inputs.
  3. Each input goes to `Player::handle`.
  4. Effects are logged and applied: `OpenUrl` → `ctx.open_url`; `Exit` shows a "course ended" overlay; the rest are visible through rendering.
- `F2` toggles a side panel with the effect log.
- `Input::Start` is fed once, right after loading.

## Parity test (`crates/render/tests/parity.rs`)

1. **Golden:** `crates/render/tests/fixtures/hello-course.pass.effects.jsonl` is the committed output of `simulate`. A test checks that feeding `pass.inputs.json` to `Player::handle` reproduces it line for line.
2. **Through the UI:** a headless `egui::Context` at 1280×800 runs `SlideView::show` each frame.
   - `Start` and `Tick` go straight to the player.
   - `Click { object }` becomes pointer move → press → release at the object's on-screen centre, using `compose` + `SlideTransform`.
   - `SelectChoice` does the same at the choice's centre.
   - Inputs the view returns are fed to the player.
   - The collected effects must equal the golden. Compare effects only: hover inputs are allowed but produce none here.

## Steps (one commit each)

1. Render core: `color`, `layout`, `compose`, `ticker`, with unit tests (hostile sizes, bad colours, timing windows, hidden state, layer order, choice layout).
2. Render view: `paint`, `view` (hit-testing, hover, click); a renderer section in `docs/spec/triggers.md`.
3. Chrome.
4. Parity test.
5. Player app: desktop + web.
6. Verification: `--log-effects` diff on desktop; click through in a browser and diff the console against the golden.
