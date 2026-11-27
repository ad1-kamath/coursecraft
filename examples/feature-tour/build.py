"""Builds 'The CourseCraft Feature Tour', the example behind Explore an example › Feature tour.

Each slide shows one area of the app: video and sound, layers with states and variables, motion, markers on a
picture, six question types and a results slide. Run from the repository root after `cargo build -p coursecraft-cli`:
    python3 examples/feature-tour/build.py
It rewrites examples/feature-tour/course.json and checks it with validate and simulate (pass.inputs.json).
The pictures in media/ came from Gemini (through the Antigravity CLI); intro.mp4 is cut from three of them with
ffmpeg (see README.md). Neither is rebuilt here.
"""
import os, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
from ccdesign import *  # noqa: E402,F401,F403
import ccdesign as cd  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
W, H = 1280, 720
SRC = 1280 / 714

P = cd.P = dict(ink="#1F1747", body="#3D3566", muted="#635C88", primary="#5B3FD6", primary_dk="#4A31B8",
                deep="#211656", tint="#ECE7FD", tint2="#D9D0FA", accent="#F49A6C", accent_lt="#F8B896",
                accent_text="#7A3412", mint="#7ED9CF", good="#0F7B5F", bad="#B42318", bg="#F6F4FD",
                lav="#EFECFA", white="#FFFFFF")

C = use({
    "schema_version": 1, "id": "coursecraft-feature-tour", "title": "The CourseCraft Feature Tour",
    "slide_size": {"w": W, "h": H},
    "variables": [{"name": "explored", "value": 0}, {"name": "seen_layers", "value": False},
                  {"name": "seen_states", "value": False}, {"name": "seen_variables", "value": False}],
    "quiz": {"passing_percent": 80},
    "player": {"show_nav": True, "show_menu": True, "show_seekbar": True, "show_transcript": True},
    "theme": {"accent": P["primary"], "background": P["bg"], "text": P["ink"], "font": "Inter",
              "player": {"bar": P["deep"], "bar_text": "#FFFFFF"},
              "palette": [{"id": f"pal{i + 1}", "name": n, "color": P[k]} for i, (k, n) in enumerate([
                  ("ink", "Ink"), ("deep", "Night"), ("primary", "Violet"), ("tint", "Lilac"), ("accent", "Peach"),
                  ("accent_text", "Rust"), ("mint", "Mint"), ("bg", "Paper")])]},
    "text_styles": [
        {"id": "ts_title", "name": "Title", "style": style(56, P["ink"], True)},
        {"id": "ts_heading", "name": "Heading", "style": style(36, P["ink"], True)},
        {"id": "ts_eyebrow", "name": "Eyebrow", "style": style(16, P["primary"], True, letter_spacing=2.5)},
        {"id": "ts_body", "name": "Body", "style": style(20, P["body"])},
    ],
    "masters": [{"id": "m_content", "title": "Content", "background": P["bg"], "objects": [
        obj("m_glow", "Corner glow", {"type": "shape", "shape": "ellipse", "fill": P["tint"]},
            F(1110, -150, 320, 320), decorative=True),
        obj("m_dot", "Brand dot", {"type": "shape", "shape": "ellipse", "fill": P["accent"]},
            F(88, 684, 8, 8), decorative=True),
        obj("m_brand", "Footer", {"type": "text", "text": "The CourseCraft Feature Tour",
                                  "style": style(12, P["muted"], True, letter_spacing=1)},
            F(104, 677, 400, 20), decorative=True),
    ]}],
    "scenes": [],
})


def scene(sid, title):
    sc = {"id": sid, "title": title, "slides": []}
    C["scenes"].append(sc)
    return sc


def new_slide(sc, sid, title, bg=None, master=True, transition=None, notes=None):
    s = {"id": sid, "title": title, "duration_ms": 5000, "background": bg or P["bg"], "objects": []}
    if master:
        s["master"] = "m_content"
    if transition:
        s["transition"] = transition
    if notes:
        s["notes"] = notes
    sc["slides"].append(s)
    return sid


def head(sid, key, eb, ttl, sub=None):
    eyebrow(sid, f"{key}-eyebrow", eb, 88, 44)
    title(sid, f"{key}-title", ttl, 84, 76, 900, 56, 40)
    if sub:
        body(sid, f"{key}-sub", sub, 88, 134, 1100, 34, 19)


def tick_row(sid, oid, label, x, y, w, start, lid=None, icon="✓", fill=None, color=None):
    ic = add(sid, shape(f"{oid}-icon", F(x, y, 32, 32), fill or P["tint"], "ellipse", "Icon", label=icon,
                        lstyle=style(16, color or P["primary_dk"], True, align="center", valign="middle"), decorative=True), lid)
    ent(ic, POP, 400, start, "overshoot")
    t = add(sid, text(oid, label, F(x + 48, y + 1, w - 48, 30), 18, P["body"], True, "Point", valign="middle"), lid)
    ent(t, fly("right"), 500, start + 80)


def next_btn(sid, oid, label="Next  →", start=900, w=180):
    btn(sid, oid, label, 1192 - w, 596, w, start=start)
    trig(sid, click(oid), jump("next"))


TRANSITION = T("push", 600, **{"from": "right"})

# ================================================================ Welcome
sc = scene("sc_welcome", "Welcome")
s = new_slide(sc, "tour", "Welcome", master=False, bg=P["lav"], transition=T("fade", 700),
              notes="Start Tour goes to the video slide, so Start Tour › Play Video plays a video in two clicks.")
hero = add(s, picture("t-hero", "hero.jpg", "A course author smiles at a large screen full of slide cards, pictures and a video.",
                      F(0, 0, W, H), mask="rectangle", name="Hero picture", src_ratio=SRC))
ent(hero, FADE, 900, 0)
eyebrow(s, "t-eyebrow", "WELCOME TO COURSECRAFT", 88, 172, start=300)
title(s, "t-title", "Take the\nfeature tour", 84, 208, 560, 150, 60, start=450)
body(s, "t-body", "Six short stops show what a CourseCraft course can do: media, layers, motion, markers, questions and results.",
     88, 372, 480, 100, start=800)
for i, (lab, w, x) in enumerate([("Media", 84, 88), ("Interactivity", 136, 184), ("Motion", 90, 332), ("Quiz", 72, 434)]):
    pill(s, f"t-chip{i + 1}", lab, x, 486, w, "#FFFFFFB3", P["primary_dk"], start=1000 + 100 * i)
btn(s, "btn-start", "Start Tour", 88, 556, 200, start=1350)
trig(s, click("btn-start"), jump("next"))

# ================================================================ Media
sc = scene("sc_build", "The tour")
s = new_slide(sc, "media", "Video and sound", transition=TRANSITION,
              notes="Play Video starts the clip with play_media. When it ends, a media_ends trigger changes the status pill and shows a layer.")
cd.slide(s)["revisit"] = "reset"
head(s, "md", "STOP 1  ·  MEDIA", "Video and sound")
frame = add(s, shape("md-frame", F(80, 166, 624, 358), P["deep"], name="Video frame", adjust=[0.04], decorative=True))
frame["shadow"] = shadow("33", 14, 34)
ent(frame, ZOOM, 600, 200, "overshoot")
vid = add(s, obj("md-video", "Tour video", {"type": "video", "src": "intro.mp4", "autoplay": False, "captions": "intro.vtt", "poster": "media.jpg"},
                 F(88, 174, 608, 342)))
ent(vid, FADE, 500, 500)
add(s, obj("md-chime", "Chime", {"type": "audio", "src": "chime.wav", "autoplay": False, "captions": "chime.vtt"}, F(0, 0, 0, 0)))
body(s, "md-body", "Video and sound play inside the slide. A trigger starts them, and another reacts when they end.",
     760, 170, 432, 90, 19, start=600)
for i, lab in enumerate(["Captions follow the playhead", "Play, pause and seek from triggers", "A sound effect on any click"]):
    tick_row(s, f"md-pt{i + 1}", lab, 760, 282 + 48 * i, 432, 700 + 120 * i)
btn(s, "btn-play-video", "Play Video", 760, 446, 180, start=1100)
trig(s, click("btn-play-video"), {"action": "play_media", "object": "md-video"})
snd = add(s, soft_button("btn-play-sound", "Play Sound", F(956, 446, 170, 56), P["tint"], P["tint2"], P["primary_dk"], 17))
ent(snd, POP, 450, 1200, "overshoot")
trig(s, click("btn-play-sound"), {"action": "play_media", "object": "md-chime"})
status = add(s, shape("md-status", F(88, 544, 300, 38), P["tint"], name="Video status", adjust=[0.5],
                      label="●  Video not played yet", lstyle=style(16, P["primary_dk"], True, align="center", valign="middle")))
status["states"] = [{"name": "finished", "fill": P["good"], "text": "✓  Video finished",
                     "text_style": style(16, "#FFFFFF", True, align="center", valign="middle")}]
ent(status, FADE, 400, 900)
trig(s, {"event": "media_ends", "object": "md-video"}, state("md-status", "finished"))
trig(s, {"event": "media_ends", "object": "md-video"}, show("video-done"))
new_layer(s, "video-done", "Video finished", duration_ms=4000, hide_other_layers=False)
toast = add(s, shape("vd-card", F(760, 520, 432, 60), P["deep"], name="Toast", adjust=[0.3],
                     label="The video ended, so a trigger showed this layer.",
                     lstyle=style(16, "#FFFFFF", True, align="center", valign="middle")), "video-done")
toast["shadow"] = shadow("40", 10, 24)
ent(toast, fly("bottom"), 450, 0, "overshoot")
next_btn(s, "md-next")

# ================================================================ Layers, states and variables
s = new_slide(sc, "interact", "Layers, states and variables", transition=TRANSITION,
              notes="Each card shows its layer, switches to its Visited state and counts itself once in the explored variable.")
cd.slide(s)["revisit"] = "resume"
head(s, "ia", "STOP 2  ·  INTERACTIVITY", "Layers, states and variables", "Select each card. The counter is a variable shown in the text.")
pic = add(s, picture("ia-pic", "layers.jpg", "Translucent slide cards stacked on top of each other.", F(88, 196, 400, 400), x=0.5, src_ratio=SRC))
pic["shadow"] = shadow("22", 12, 30)
ent(pic, reveal_shape("box"), 800, 300, "ease_in_out")
CARDS = [("layers", "Layers", "01", "A layer sits on top of the slide. Show it with a trigger to add feedback, a pop-up or a new step without leaving the slide."),
         ("states", "States", "02", "A state changes how one object looks: hover, selected, visited or any state you name. These cards use three of them."),
         ("variables", "Variables", "03", "A variable remembers a value. This one counts the cards you open and shows the total in the text below.")]
for i, (key, lab, icon, desc) in enumerate(CARDS):
    x = 528 + 224 * i
    c = add(s, shape(f"card-{key}", F(x, 196, 208, 132), P["white"], name=f"{lab} card", adjust=[0.12],
                     label=f"{icon}\n{lab}", lstyle=style(20, P["ink"], True, align="center", valign="middle")))
    c["description"] = f"Show {lab.lower()}"
    c["shadow"] = shadow("1F", 8, 22)
    c["states"] = [{"name": "hover", "fill": P["tint"]},
                   {"name": "visited", "fill": P["tint"], "text": f"✓\n{lab}",
                    "text_style": style(20, P["primary_dk"], True, align="center", valign="middle")},
                   {"name": "selected", "fill": P["primary"], "text": f"{icon}\n{lab}",
                    "text_style": style(20, "#FFFFFF", True, align="center", valign="middle")}]
    ent(c, floaty("bottom"), 600, 500 + 120 * i)
    lid = f"about-{key}"
    new_layer(s, lid, lab, duration_ms=3000)
    panel = add(s, shape(f"{key}-panel", F(522, 346, 668, 162), P["white"], name="Explanation", adjust=[0.1], decorative=True), lid)
    panel["shadow"] = shadow("1F", 8, 22)
    ent(panel, FADE, 300, 0)
    ent(add(s, text(f"{key}-head", lab, F(560, 372, 600, 36), 24, P["ink"], True, "Heading"), lid), RISE, 400, 80)
    ent(add(s, text(f"{key}-desc", desc, F(560, 412, 600, 80), 18, P["body"], False, "Body", ls=1.4), lid), FADE, 400, 160)
    for j, (k2, *_r) in enumerate(CARDS):
        if j != i:
            trig(s, click(f"card-{key}"), state(f"card-{k2}", "visited"),
                 cond=cmp(f"seen_{k2}", "eq", True))
    trig(s, click(f"card-{key}"), setv("explored", 1, "add"), cond=cmp(f"seen_{key}", "eq", False))
    trig(s, click(f"card-{key}"), setv(f"seen_{key}", True))
    trig(s, click(f"card-{key}"), state(f"card-{key}", "selected"))
    trig(s, click(f"card-{key}"), show(lid))
hint = add(s, shape("ia-hint", F(528, 352, 656, 150), P["tint"] + "88", name="Hint", adjust=[0.1],
                    outline={"color": P["primary"] + "55", "width": 2, "dash": "dash"},
                    label="Select a card to see what it does", lstyle=style(18, P["muted"], True, align="center", valign="middle")))
ent(hint, FADE, 400, 900)
cd.slide(s)["objects"].insert(1, cd.slide(s)["objects"].pop())
counter = add(s, shape("ia-counter", F(528, 528, 270, 44), P["tint"], name="Counter", adjust=[0.5],
                       label="%explored% of 3 explored", lstyle=style(17, P["primary_dk"], True, align="center", valign="middle")))
counter["states"] = [{"name": "done", "fill": P["good"], "text": "✓  All 3 explored",
                      "text_style": style(17, "#FFFFFF", True, align="center", valign="middle")}]
ent(counter, POP, 400, 1000, "overshoot")
trig(s, {"event": "variable_changes", "variable": "explored"}, state("ia-counter", "done"), cond=cmp("explored", "eq", 3))
next_btn(s, "ia-next")

# ================================================================ Motion
s = new_slide(sc, "motion", "Entrances, paths and transitions", transition=T("zoom", 700),
              notes="Five tiles, five entrance effects. Move it runs a motion path; Replay jumps back to this slide (revisit: reset).")
cd.slide(s)["revisit"] = "reset"
head(s, "mo", "STOP 3  ·  MOTION", "Entrances, paths and transitions", "Every tile below enters with a different effect. This slide arrived with a zoom transition.")
EFFECTS = [("Fly in", "arrow_right", fly("left")), ("Zoom", "magnifier", ZOOM), ("Spin and grow", "gear", SPIN_GROW),
           ("Bounce", "ellipse", BOUNCE), ("Wipe", "arrow_chevron", wipe("bottom"))]
for i, (lab, icon, eff) in enumerate(EFFECTS):
    x = 88 + 224 * i
    tile = add(s, shape(f"mo-tile{i + 1}", F(x, 200, 200, 180), [P["primary"], P["accent"], P["mint"], P["deep"], P["tint2"]][i],
                        name=f"{lab} tile", adjust=[0.12], decorative=True))
    ent(tile, eff, 800, 400 + 350 * i, "overshoot" if lab in ("Zoom", "Bounce") else "ease_out")
    dark = i in (0, 3)
    ent(add(s, shape(f"mo-icon{i + 1}", F(x + 72, 234, 56, 56), "#FFFFFF" if dark else P["ink"], icon, "Icon",
                     decorative=True)), POP, 400, 700 + 350 * i, "overshoot")
    ent(add(s, text(f"mo-lab{i + 1}", lab, F(x, 316, 200, 34), 19, "#FFFFFF" if dark else P["ink"], True, "Effect name",
                    align="center")), FADE, 300, 750 + 350 * i)
track = add(s, shape("mo-track", F(88, 470, 760, 4), P["tint2"], "rectangle", "Track", decorative=True))
ent(track, wipe("left"), 600, 2200)
star = add(s, shape("mo-star", F(88, 436, 72, 72), P["accent"], "star_5", "Moving star"))
star["description"] = "A star that moves along a motion path"
star["motion_paths"] = [{"id": "glide", "points": [{"x": 0.0, "y": 0.0}, {"x": 340.0, "y": -40.0}, {"x": 688.0, "y": 0.0}],
                         "duration_ms": 1400, "easing": "ease_in_out", "relative": True}]
ent(star, POP, 450, 2400, "overshoot")
mv = add(s, soft_button("mo-move", "Move it", F(900, 444, 140, 56), P["tint"], P["tint2"], P["primary_dk"], 17))
ent(mv, POP, 400, 2500, "overshoot")
trig(s, click("mo-move"), {"action": "move_on_path", "object": "mo-star", "path": "glide"})
rp = add(s, soft_button("mo-replay", "Replay", F(1056, 444, 136, 56), P["tint"], P["tint2"], P["primary_dk"], 17))
ent(rp, POP, 400, 2600, "overshoot")
trig(s, click("mo-replay"), jump("motion"))
next_btn(s, "mo-next", start=2700)

# ================================================================ Markers on a map
s = new_slide(sc, "campus", "Markers on a picture", transition=TRANSITION,
              notes="Markers open a small pop-up with a title and a sentence. The learner opens them in any order.")
head(s, "cp", "STOP 4  ·  EXPLORE", "Markers on a picture", "Select each marker to open it. Markers are a quick way to label a picture.")
MAP = F(88, 186, 680, 380)
mp = add(s, picture("cp-map", "campus-map.jpg", "An illustrated campus map with a library, a cafe, a round science lab and a park with a pond.",
                    MAP, src_ratio=SRC))
mp["shadow"] = shadow("22", 12, 30)
ent(mp, reveal_shape("circle"), 900, 300, "ease_in_out")
MARKERS = [("Library", "Quiet study rooms and the help desk.", 0.13, 0.13), ("Cafe", "Coffee, lunch and the best seats outside.", 0.79, 0.16),
           ("Science lab", "Labs on two floors around a glass roof.", 0.14, 0.66), ("Park", "A pond, shady trees and a walking loop.", 0.72, 0.64)]
for i, (ttl, bd, fx, fy) in enumerate(MARKERS):
    m = add(s, obj(f"cp-m{i + 1}", f"Marker: {ttl}", {"type": "marker", "title": ttl, "body": bd, "icon": "plus", "color": P["accent"]},
                   F(round(MAP["x"] + fx * MAP["w"]) - 21, round(MAP["y"] + fy * MAP["h"]) - 21, 42, 42)))
    ent(m, POP, 450, 1200 + 150 * i, "overshoot")
    emph(m, "pulse", 2200 + 150 * i, 800, 2)
side = card(s, "cp-side", F(808, 186, 384, 380), P["white"], start=400, effect=fly("right"))
ent(add(s, text("cp-side-title", "What markers do", F(840, 214, 320, 36), 24, P["ink"], True, "Heading")), FADE, 400, 600)
for i, lab in enumerate(["Open with a click or the keyboard", "Hold a title and one sentence", "Pick a colour and an icon"]):
    tick_row(s, f"cp-pt{i + 1}", lab, 840, 272 + 52 * i, 330, 700 + 120 * i)
ent(add(s, text("cp-tip", "You'll use this map again in the quiz.", F(840, 448, 320, 50), 16, P["muted"], False, "Tip", ls=1.4)), FADE, 400, 1200)
next_btn(s, "cp-next", "To the quiz  →", start=1300, w=200)

# ================================================================ Quiz
sc = scene("sc_quiz", "Quick quiz")
QUIZ = [
    ("q-mc", "multiple_choice", "Which one shows extra content on top of a slide without leaving it?",
     dict(choices=[("a", "A layer", True), ("b", "A new scene", False), ("c", "The slide master", False)]),
     "Yes. Layers sit on top of their slide.", "Not quite. A layer shows content on top of the slide you're on."),
    ("q-tf", "true_false", "True or false: a trigger can react when a video ends.",
     dict(choices=[("true", "True", True), ("false", "False", False)]),
     "True. That's how the video stop showed its message.", "It can. A media ends trigger reacts to the end of a video."),
    ("q-fill", "fill_in", "Type the missing word: a ________ remembers a value, like the counter on stop 2.",
     dict(accepted=["variable", "a variable"]),
     "Right, a variable.", "Not quite. The answer is variable."),
    ("q-match", "matching", "Match each feature to what it does.",
     dict(pairs=[("p1", "Trigger", "Runs an action when something happens"), ("p2", "State", "Changes how an object looks"),
                 ("p3", "Marker", "Opens a pop-up on a picture")]),
     "All matched.", "Not quite. Check the pairs and try again."),
    ("q-map", "hotspot", "On the campus map, select the library.", {},
     "That's the library.", "Not quite. The library has the dome, top left."),
    ("q-sort", "drag_drop", "Drag each item into the right group.", {},
     "Sorted!", "Not quite. Videos and sounds are media; the rest are questions."),
]
NQ = len(QUIZ)
for n, (sid, kind, prompt, extra, ok, no) in enumerate(QUIZ, 1):
    s = new_slide(sc, sid, f"Question {n}", transition=TRANSITION)
    eyebrow(s, f"{sid}-eyebrow", f"QUESTION {n} OF {NQ}", 88, 40)
    progress(s, f"{sid}-progress", n / NQ, 200)
    q = {"kind": kind, "prompt": prompt, "points": 10, "attempts": 2, "choices": [],
         "correct_layer": f"{sid}-correct", "incorrect_layer": f"{sid}-incorrect"}
    if "choices" in extra:
        q["choices"] = [{"id": i, "text": t, **({"correct": True} if c else {})} for i, t, c in extra["choices"]]
    if "accepted" in extra:
        q["accepted"] = extra["accepted"]
    if "pairs" in extra:
        q["pairs"] = [{"id": i, "left": a, "right": b} for i, a, b in extra["pairs"]]
    if kind in ("multiple_choice", "true_false", "fill_in", "matching"):
        q["frame"] = {"x": 88, "y": 112, "w": 660, "h": 440}
        pic = add(s, picture(f"{sid}-pic", "quiz.jpg", "A learner thinks about a question, surrounded by answer cards.",
                             F(808, 96, 384, 500), x=0.86, src_ratio=SRC))
        pic["shadow"] = shadow("22", 12, 30)
        ent(pic, reveal_shape("box"), 800, 200, "ease_in_out")
    elif kind == "hotspot":
        q["frame"] = {"x": 88, "y": 80, "w": 900, "h": 44}
        MAPQ = F(88, 136, 760, 424)
        add(s, image(f"{sid}-map", "campus-map.jpg", "The campus map: library top left, cafe top right, science lab bottom left, park bottom right.",
                     MAPQ, name="Campus map", mask="rounded_rectangle"))["shadow"] = shadow("22", 12, 30)
        q["hotspot_image"] = f"{sid}-map"
        q["regions"] = [{"id": "library", "shape": "rect", "x": round(88 + 0.04 * 760), "y": round(136 + 0.02 * 424),
                         "w": round(0.39 * 760), "h": round(0.40 * 424), "correct": True}]
        side = card(s, f"{sid}-side", F(880, 136, 312, 424), P["white"], start=300, effect=fly("right"))
        ent(add(s, text(f"{sid}-hint-title", "Hint", F(912, 164, 250, 36), 24, P["ink"], True, "Heading")), FADE, 400, 500)
        ent(add(s, text(f"{sid}-hint", "The library has a dome and a wide staircase. You saw it on stop 4.",
                        F(912, 210, 250, 120), 18, P["body"], False, "Hint", ls=1.4)), FADE, 400, 600)
    elif kind == "drag_drop":
        for j, (gid, glab) in enumerate([("grp-media", "Media"), ("grp-quiz", "Questions")]):
            g = add(s, shape(gid, F(88 + 420 * j, 300, 400, 250), P["tint"], name=f"{glab} group", adjust=[0.06],
                             outline={"color": P["primary"] + "66", "width": 2, "dash": "dash"}))
            g["description"] = f"Drop zone: {glab}"
            ent(g, FADE, 500, 200 + 100 * j)
            ent(add(s, text(f"{gid}-lab", glab.upper(), F(112 + 420 * j, 316, 352, 30), 16, P["primary"], True, "Group label",
                            letter_spacing=2.5)), FADE, 400, 400)
        items = [("it-video", "Video clip", "grp-media"), ("it-sound", "Sound effect", "grp-media"),
                 ("it-match", "Matching", "grp-quiz"), ("it-hotspot", "Hotspot", "grp-quiz")]
        for j, (iid, ilab, _t) in enumerate(items):
            it = add(s, soft_button(iid, ilab, F(88 + 208 * j, 196, 192, 56), P["white"], P["tint"], P["ink"], 17))
            it["shadow"] = shadow("22", 6, 16)
            it.pop("states", None)
            ent(it, POP, 400, 500 + 100 * j, "overshoot")
        q["drag_items"] = [{"item": i, "target": t} for i, _l, t in items]
        q["drop_targets"] = ["grp-media", "grp-quiz"]
        q["one_per_target"] = False
        q["frame"] = {"x": 88, "y": 112, "w": 900, "h": 50}
    cd.slide(s)["question"] = q
    btn(s, f"{sid}-submit", "Submit", 88, 584, 180, start=600)
    trig(s, click(f"{sid}-submit"), {"action": "submit_question"})
    new_layer(s, f"{sid}-correct", "Correct", prevent_base_clicks=True)
    new_layer(s, f"{sid}-incorrect", "Incorrect", prevent_base_clicks=True)
    last = n == NQ
    feedback(s, f"{sid}-correct", f"{sid}-ok-text", ok, True, "See result  →" if last else "Continue  →", f"{sid}-ok-next", jump("next"))
    feedback(s, f"{sid}-incorrect", f"{sid}-no-text", no, False, "Try again", f"{sid}-no-retry", hide())

# ================================================================ Results
s = new_slide(sc, "results", "Your result", master=False, bg=P["lav"], transition=T("zoom", 700),
              notes="Submits the quiz, then shows the passed or failed layer.")
done = add(s, picture("r-pic", "done.jpg", "A happy learner raises both arms next to a big trophy and confetti.",
                      F(660, 0, 620, 720), x=0.47, mask="rectangle", name="Trophy picture", src_ratio=SRC))
ent(done, FADE, 800, 0)
eyebrow(s, "r-eyebrow", "TOUR COMPLETE", 88, 150, start=200)
score = add(s, text("r-score", "%Course.ScorePercent%%", F(84, 184, 400, 120), 104, P["primary"], True, "Score"))
ent(score, ZOOM, 700, 500, "overshoot")
trig(s, {"event": "timeline_starts"}, {"action": "submit_results"})
trig(s, {"event": "timeline_starts"}, show("passed"), cond=cmp("Quiz.Passed", "eq", True))
trig(s, {"event": "timeline_starts"}, show("failed"), cond=cmp("Quiz.Passed", "eq", False))
for lid, good in (("passed", True), ("failed", False)):
    new_layer(s, lid, "Passed" if good else "Failed", duration_ms=3000, hide_other_layers=True)
    ent(add(s, text(f"{lid}-title", "You've seen the tour" if good else "So close", F(88, 310, 560, 60), 40, P["ink"], True,
                    "Heading", ts="ts_heading"), lid), RISE, 600, 300)
    ent(add(s, text(f"{lid}-body",
                    "Open Healthy Habits at Work for a full course, or start your own from a template." if good else
                    "You need 80% to pass. Have another go at the questions.",
                    F(88, 376, 500, 70), 20, P["body"], False, "Body", ls=1.45), lid), FADE, 600, 500)
    if good:
        a = add(s, soft_button("r-finish", "Finish  ✓", F(88, 470, 170, 56), P["primary"], P["primary_dk"], "#FFFFFF", 18), lid)
        trig(s, click("r-finish"), {"action": "exit_course"}, lid=lid)
        b = add(s, soft_button("r-restart", "Start again", F(274, 470, 170, 56), P["tint"], P["tint2"], P["primary_dk"], 17), lid)
        trig(s, click("r-restart"), jump("tour"), lid=lid)
    else:
        a = add(s, soft_button("r-retry", "Try again", F(88, 470, 170, 56), P["primary"], P["primary_dk"], "#FFFFFF", 18), lid)
        trig(s, click("r-retry"), {"action": "retry_quiz"}, lid=lid)
        b = add(s, soft_button("r-restart2", "Restart the tour", F(274, 470, 200, 56), P["tint"], P["tint2"], P["primary_dk"], 17), lid)
        trig(s, click("r-restart2"), jump("tour"), lid=lid)
    ent(a, POP, 450, 800, "overshoot")
    ent(b, POP, 450, 900, "overshoot")

path = os.path.join(HERE, "course.json")
save(path)
inputs = os.path.join(HERE, "pass.inputs.json")
check(path, inputs if os.path.exists(inputs) else None)
