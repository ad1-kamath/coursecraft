# The CourseCraft Feature Tour

The example behind Explore an example › Feature tour in the app. Each slide is one stop on a tour of what a CourseCraft course can do.

12 slides in 3 scenes: Welcome, The tour and Quick quiz.

| Slide | What it shows |
|---|---|
| Welcome | A full-bleed picture, chips and the Start Tour button |
| Video and sound | A video with captions started by Play Video, a sound effect, a `media_ends` trigger that changes a state and shows a layer |
| Layers, states and variables | Cards with hover, Selected and Visited states, one layer per card, a counter variable shown as `%explored%`, a `variable_changes` trigger |
| Entrances, paths and transitions | Five entrance effects, a motion path started from a button, a zoom transition and a Replay that restarts the slide |
| Markers on a picture | Four markers on a map, with pulse emphasis |
| Questions 1–6 | Multiple choice, true/false, fill-in, matching, hotspot (on the same map) and drag and drop, all laid out with a question `frame` |
| Your result | `submit_results`, the score as `%Course.ScorePercent%`, passed and failed layers, Retry quiz and Exit course |

**Start Tour › Play Video** plays a video in two clicks, which makes this course a quick check that video playback works on a new machine.

## Build and check

```sh
cargo build -p coursecraft-cli
python3 examples/feature-tour/build.py
cargo run -p coursecraft-cli -- validate examples/feature-tour/course.json
cargo run -p coursecraft-cli -- a11y-check examples/feature-tour/course.json
cargo run -p coursecraft-cli -- simulate examples/feature-tour/course.json examples/feature-tour/pass.inputs.json
```

`build.py` writes `course.json` with the helpers in `examples/tools/ccdesign.py`. `pass.inputs.json` plays the video, opens every card and marker, answers every question correctly and finishes with a 100% pass.

## Media

- The pictures came from Gemini (through the Antigravity CLI).
- `intro.mp4` (7 seconds, 640×360, H.264 and AAC) is cut with ffmpeg from `media.jpg` (also the video's poster), `layers.jpg` and `done.jpg`: a slow zoom on each picture, a fade and a slide between them, and a quiet three-note chord. `intro.vtt` holds its captions.
- `chime.wav` and `chime.vtt` are the same files as in `examples/showcase`.
