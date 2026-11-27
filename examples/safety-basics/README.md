# Workplace Safety Basics

A short example course: Explore an example › Workplace Safety Basics in the app opens it. It is small enough to read in one sitting and shows a typical compliance module: a welcome, three hazards in tabs, a hotspot question, three more questions and a results slide.

6 slides in 3 scenes: Welcome, Three hazards and Check your knowledge.

| Slide | What it shows |
|---|---|
| Welcome | A full-bleed picture, a master-free title slide, chips, an About layer that dims the slide and blocks clicks behind it |
| Three everyday hazards | Tabs built from shapes with a Selected state, one layer per hazard, masked picture crops, `revisit: reset` |
| Spot the hazards | A hotspot question with three markers, a distractor region (the fire extinguisher) and partial scoring |
| Questions 1–3 | Multiple choice, multiple response (partial scoring) and sequence questions laid out with a question `frame` |
| Your result | `submit_results`, a score from `%Course.ScorePercent%`, passed and failed layers, Retry quiz and Exit course |

The theme has a palette, named text styles and one master (a hi-vis stripe and a footer), so Design › Theme recolours it.

## Build and check

```sh
cargo build -p coursecraft-cli
python3 examples/safety-basics/build.py
cargo run -p coursecraft-cli -- validate examples/safety-basics/course.json
cargo run -p coursecraft-cli -- a11y-check examples/safety-basics/course.json
cargo run -p coursecraft-cli -- simulate examples/safety-basics/course.json examples/safety-basics/pass.inputs.json
```

`build.py` writes `course.json` with the helpers in `examples/tools/ccdesign.py`. The pictures in `media/` came from Gemini (through the Antigravity CLI) and are not rebuilt by the script.

`pass.inputs.json` opens the About layer, visits every tab, answers every question correctly and finishes the course with a 100% pass.
