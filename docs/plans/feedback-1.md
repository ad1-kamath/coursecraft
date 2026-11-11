# Tester feedback, round 1

What the first real use of the app turned up. Each step ends with `cargo xtask ci` passing.

| Step | What |
|---|---|
| 1 | Bugs: removing a question fails, Ungroup enabled for a single object, Restart preview doesn't restart |
| 2 | Drag on the slide to size a new object |
| 3 | Reorder objects from the timeline, by dragging and from a right-click menu |
| 4 | Runtime: learner pause, resume and seek on the slide timeline, and player volume |
| 5 | Player controls: play and pause, a seek bar for the slide, replay, previous and next, volume |

Not in this round: ElevenLabs narration.

## Decisions

- **Dragging places, clicking still works.** With a tool armed, a drag sets the frame and a plain click places the default size at that point.
- **Timeline rows run front to back,** as they do now if that's the existing order; top of the list is frontmost only if the code already treats it so. A drag or a menu item changes stacking through `object.reorder`, one undo step.
- **The seek bar belongs to the slide,** not the course. It replaces the course-wide progress rule, under the same `show_seekbar` setting. The strip still shows `n / N`.
- **Pausing is the learner's, and separate from authors' `pause_timeline`.** Resuming doesn't undo an author's pause or `pause_media`.
- **Seeking doesn't undo anything.** Crossing a cue forward fires it. Going back re-arms later cues, but variables, states and layers stay as they are.
- **Volume is a player setting,** not course content or part of `suspend_data`.
