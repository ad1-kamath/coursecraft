# Healthy Habits at Work

The sample course CourseCraft opens on a fresh launch. It is a short wellbeing module, and every slide shows off a different part of the app, so a new author can open it, press F12 and see what CourseCraft can do.

35 slides in 7 scenes: Welcome, Lesson 1: Move, Lesson 2: Breathe, Lesson 3: Connect, Final quiz, Reflect and finish, and Extras. Each lesson opens on a chapter slide and ends on a quick check.

## What it shows

| Area | Where to look |
|---|---|
| Custom font (DM Serif Display from Google Fonts, SIL OFL) | Every heading. Course Settings ▸ Fonts; the file is `media/fonts/` with its licence |
| Background music across the whole course | Course Settings ▸ Background audio (`music.mp3`, 10% volume, loops) |
| Theme, palette, slide template | Every slide. Course Settings ▸ Theme; Lesson page (New slide ▸ Your templates) |
| Entrance and exit effects (fade, rise, fly in, float, wipe, split, peek, shape reveal, wheel, strips, flip, zoom, grow, spin and grow, pop) with easings (overshoot, elastic, bounce) | Every slide; the chapter slides use the most |
| Text that plays in by letter or by paragraph | The Welcome title, chapter titles, Well done |
| Emphasis effects (pulse, heartbeat, jump, shake, teeter, spin, grow and shrink, color pulse) | Chapter rings spin slowly; desk markers pulse; quiz feedback jumps or shakes; the results badge |
| Motion paths and `move_on_path` | The drifting Welcome hero, the dots on each chapter slide, Breathe with the circle |
| Slide transitions (fade, push, cover, wipe, circle and diamond shapes, clock, zoom, dissolve) | Every slide; each lesson opens with a different one |
| Objects timed to narration | Most content slides. Open the Timeline panel to see each line start on its spoken words |
| Audio narration with captions, plus a Pause voice button | 29 clips, one voice throughout |
| Hover states and custom states | Every button; habit cards; the eye-rest timer; the sort cards |
| Layers (feedback, modal, celebration) | Three small habits, Rest your eyes, Helps or hurts?, every question, Your results |
| Variables, `%var%` text, conditions | Your name everywhere; habits explored; stress and energy check-in; plan choices |
| Text entry, sliders, checkbox, radio group, progress meters | Nice to meet you, How are you today?, Make your plan |
| Picture fills, gradient fills, image masks, shadows, glows | Chapter slides, quiz picture cards, the certificate seal |
| Markers | Set up your desk |
| Video with cue points and captions | Three desk stretches |
| Lottie animation, `timeline_reaches`, pause and resume timeline | Breathe with the circle |
| Timer with start, pause and reset | Rest your eyes |
| Draggable objects and `dropped_on`, with a confetti layer | Helps or hurts? |
| Ready-made interactions | Flip cards (Myth or fact?), process steps, scenario, timeline, reveal grid |
| Table | Make your plan |
| Lightbox, Open URL | Bonus slide |
| All 12 question types | Lesson quick checks (multiple choice, multiple response, true/false), Final quiz (fill-in, sequence, matching, hotspot, drag and drop, freeform slider), Reflections (Likert, short answer, essay; not scored) |
| Question bank and random draw | Quick checks bank (3 questions); one is drawn after Question 6 |
| Results, retry, review and certificate | Your results, Your certificate (with print) |
| Per-slide player rules | No seekbar on graded questions |

## Checks

```sh
cargo run -p coursecraft-cli -- validate examples/healthy-habits/course.json
cargo run -p coursecraft-cli -- a11y-check examples/healthy-habits/course.json
cargo run -p coursecraft-cli -- simulate examples/healthy-habits/course.json examples/healthy-habits/pass.inputs.json
```

`pass.inputs.json` plays the whole course and scores 100%.

## Media

The pictures were made with Gemini through the Antigravity CLI in one soft editorial style. The narration was made with Gemini text-to-speech (one voice, Sulafat, for all 29 clips) and the background music with Lyria, both through `gemini-api`. `stretch.mp4` was made with ffmpeg. `breathe-circle.json` is a hand-written Lottie file. Captions are WebVTT files next to their media; `media/narration.json` holds the sentence timings `build.py` uses to time objects and write the captions.

## Rebuilding

`build.py` builds `course.json` from engine commands plus direct JSON edits. Run it from the repository root after `cargo build -p coursecraft-cli`:

```sh
python3 examples/healthy-habits/build.py
```
