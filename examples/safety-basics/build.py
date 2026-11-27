"""Builds 'Workplace Safety Basics', the short example course behind Explore an example › Workplace Safety Basics.

Six slides: a welcome with an About layer, three hazards in tabs, a spot-the-hazard hotspot question, three more
graded questions and a results slide with passed and failed layers. Run from the repository root after
`cargo build -p coursecraft-cli`:
    python3 examples/safety-basics/build.py
It rewrites examples/safety-basics/course.json and checks it with validate and simulate (pass.inputs.json).
The pictures in media/ came from Gemini (through the Antigravity CLI) and are not rebuilt here.
"""
import os, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
from ccdesign import *  # noqa: E402,F401,F403
import ccdesign as cd  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
W, H = 1280, 720
SRC = 1280 / 714   # picture aspect ratio

P = cd.P = dict(ink="#14213D", body="#33405A", muted="#5E6880", primary="#1F4E8C", primary_dk="#183D6E",
                deep="#0F1B33", tint="#EAF0F8", tint2="#D3DEEE", accent="#F2B705", accent_dk="#C99700",
                accent_text="#6B4F00", good="#0F7B5F", bad="#B42318", bg="#F7F5F1", warm="#EFEBE5",
                white="#FFFFFF")

C = use({
    "schema_version": 1, "id": "workplace-safety-basics", "title": "Workplace Safety Basics",
    "slide_size": {"w": W, "h": H},
    "variables": [],
    "quiz": {"passing_percent": 75},
    "player": {"show_nav": True, "show_menu": True, "show_seekbar": True},
    "theme": {"accent": P["primary"], "background": P["bg"], "text": P["ink"], "font": "Inter",
              "player": {"bar": P["deep"], "bar_text": "#FFFFFF"},
              "palette": [{"id": f"pal{i + 1}", "name": n, "color": P[k]} for i, (k, n) in enumerate([
                  ("ink", "Ink"), ("deep", "Night"), ("primary", "Safety blue"), ("tint", "Mist"),
                  ("accent", "Hi-vis yellow"), ("accent_text", "Amber text"), ("warm", "Wall"), ("bg", "Paper")])]},
    "text_styles": [
        {"id": "ts_title", "name": "Title", "style": style(56, P["ink"], True)},
        {"id": "ts_heading", "name": "Heading", "style": style(36, P["ink"], True)},
        {"id": "ts_eyebrow", "name": "Eyebrow", "style": style(16, P["primary"], True, letter_spacing=2.5)},
        {"id": "ts_body", "name": "Body", "style": style(20, P["body"])},
    ],
    "masters": [{"id": "m_content", "title": "Content", "background": P["bg"], "objects": [
        obj("m_stripe", "Hi-vis stripe", {"type": "shape", "shape": "rectangle", "fill": P["accent"]},
            F(0, 0, 1280, 6), decorative=True),
        obj("m_dot", "Brand dot", {"type": "shape", "shape": "ellipse", "fill": P["accent_dk"]},
            F(88, 684, 8, 8), decorative=True),
        obj("m_brand", "Footer", {"type": "text", "text": "Workplace Safety Basics",
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


def tick_row(sid, oid, label, x, y, w, start, lid=None, color=None, icon="✓", icon_fill=None):
    """A row with a round icon and one line of text."""
    ic = add(sid, shape(f"{oid}-icon", F(x, y, 32, 32), icon_fill or P["accent"], "ellipse", "Icon", label=icon,
                        lstyle=style(16, P["ink"], True, align="center", valign="middle"), decorative=True), lid)
    ent(ic, POP, 400, start, "overshoot")
    t = add(sid, text(oid, label, F(x + 48, y + 1, w - 48, 30), 18, color or P["body"], True, "Point", valign="middle"), lid)
    ent(t, fly("right"), 500, start + 80)


def side_picture(sid, oid, src, alt, x=0.5, start=200):
    """The picture panel on the right of a question slide."""
    pic = add(sid, picture(oid, src, alt, F(808, 96, 384, 500), x=x, src_ratio=SRC))
    pic["shadow"] = shadow("26", 12, 30)
    ent(pic, reveal_shape("box"), 800, start, "ease_in_out")
    return pic


# ================================================================ Welcome
sc = scene("sc_welcome", "Welcome")
s = new_slide(sc, "welcome", "Welcome", master=False, bg=P["warm"], transition=T("fade", 700),
              notes="The course opens here. About this course opens a layer with what to expect.")
hero = add(s, picture("w-hero", "hero.jpg", "A smiling warehouse worker in a hi-vis vest holds a tablet beside shelves of boxes.",
                      F(0, 0, W, H), mask="rectangle", name="Hero picture", src_ratio=SRC))
ent(hero, FADE, 900, 0)
stripe = add(s, shape("w-stripe", F(0, 0, 1280, 8), P["accent"], "rectangle", "Hi-vis stripe", decorative=True))
ent(stripe, wipe("left"), 700, 200)
eyebrow(s, "w-eyebrow", "SAFETY ESSENTIALS  ·  5 MINUTES", 88, 168, start=300)
title(s, "w-title", "Workplace\nSafety Basics", 84, 204, 560, 150, 60, start=450)
body(s, "w-body", "Most workplace injuries start with something small. Learn to spot three everyday hazards and the habit that stops each one.",
     88, 370, 470, 100, start=800)
for i, (lab, w, x) in enumerate([("3 hazards", 112, 88), ("4 questions", 128, 212), ("Pass mark 75%", 156, 352)]):
    pill(s, f"w-chip{i + 1}", lab, x, 492, w, "#FFFFFFB3", P["primary_dk"], start=1000 + 120 * i)
btn(s, "w-start", "Start the course  →", 88, 560, 240, start=1300)
trig(s, click("w-start"), jump("next"))
about = add(s, soft_button("w-about", "About this course", F(344, 560, 200, 56), "#FFFFFFB3", P["white"], P["primary_dk"], 17))
about["kind"]["outline"] = {"color": P["primary"] + "55", "width": 1.5}
ent(about, POP, 450, 1420, "overshoot")
trig(s, click("w-about"), show("about"))

new_layer(s, "about", "About this course", prevent_base_clicks=True, hide_other_layers=True)
dim = add(s, shape("ab-dim", F(0, 0, W, H), P["deep"] + "99", "rectangle", "Dim", decorative=True), "about")
ent(dim, FADE, 300, 0)
panel = add(s, shape("ab-card", F(340, 150, 600, 410), P["white"], name="About panel", adjust=[0.05], decorative=True), "about")
panel["shadow"] = shadow("40", 18, 40)
ent(panel, ZOOM, 450, 0, "overshoot")
ent(add(s, shape("ab-rule", F(388, 186, 40, 5), P["accent"], "rectangle", "Hi-vis rule", decorative=True), "about"), wipe("left"), 400, 150)
ent(add(s, text("ab-title", "About this course", F(388, 198, 500, 50), 32, P["ink"], True, "Heading", ts="ts_heading"), "about"), FADE, 400, 150)
ent(add(s, text("ab-body", "A short course for anyone who works in a shop, a warehouse or an office.", F(388, 250, 500, 60), 18, P["body"], False, "Body", ls=1.4), "about"), FADE, 400, 250)
for i, (lab, icon) in enumerate([("About 5 minutes, at your own pace", "5"), ("Three hazards, each with one simple habit", "3"),
                                  ("Four graded questions. You need 75% to pass", "✓")]):
    tick_row(s, f"ab-row{i + 1}", lab, 388, 322 + 48 * i, 500, 350 + 100 * i, lid="about", icon=icon)
close = add(s, soft_button("ab-close", "Got it", F(388, 478, 150, 50), P["primary"], P["primary_dk"], P["white"], 17), "about")
ent(close, POP, 400, 700, "overshoot")
trig(s, click("ab-close"), hide("about"), lid="about")

# ================================================================ Hazards
sc = scene("sc_learn", "Three hazards")
s = new_slide(sc, "hazards", "Three everyday hazards", transition=T("push", 600, **{"from": "right"}),
              notes="Each tab shows one hazard on its own layer. The selected tab keeps its Selected state.")
cd.slide(s)["revisit"] = "reset"
eyebrow(s, "hz-eyebrow", "THREE EVERYDAY HAZARDS", 88, 48)
title(s, "hz-title", "Know what to look for", 84, 82, 700, 60, 40)
TABS = [("trips", "1  Trips", "trips.jpg", 0.72, "A worker steps over a loose cable and an open box left on the floor.",
         "Trips and falls", "Cables, boxes and open drawers in a walkway catch your feet when you are busy.",
         ["Keep walkways clear", "Tape down or reroute cables", "Close drawers after use"]),
        ("lifting", "2  Lifting", "lifting.jpg", 0.66, "A worker squats with a straight back to lift a box from the floor.",
         "Lifting and carrying", "Lifting with a bent back strains it. Your legs are much stronger.",
         ["Test the weight first", "Bend your knees, not your back", "Hold the load close"]),
        ("spills", "3  Spills", "spills.jpg", 0.62, "A wet floor sign and a mop bucket stand beside a puddle in a corridor.",
         "Spills and wet floors", "A small puddle near a door is easy to miss and easy to slip on.",
         ["Stop and warn others", "Put out a wet floor sign", "Clean it up or report it"])]
for i, (key, lab, *_rest) in enumerate(TABS):
    b = add(s, soft_button(f"tab-{key}", lab, F(88 + 186 * i, 166, 172, 46), P["tint"], P["tint2"], P["primary_dk"], 17, radius=0.5))
    b["states"].append({"name": "selected", "fill": P["primary"], "text_style": style(17, "#FFFFFF", True, align="center", valign="middle")})
    ent(b, POP, 400, 500 + 100 * i, "overshoot")
for i, (key, lab, src, fx, alt, head, desc, tips) in enumerate(TABS):
    lid = f"layer-{key}"
    new_layer(s, lid, head, duration_ms=4000)
    pic = add(s, picture(f"{key}-pic", src, alt, F(640, 166, 552, 404), x=fx, src_ratio=SRC), lid)
    pic["shadow"] = shadow("26", 12, 30)
    ent(pic, fly("right"), 600, 0, "ease_out")
    ent(add(s, text(f"{key}-head", head, F(88, 250, 520, 46), 30, P["ink"], True, "Heading", ts="ts_heading"), lid), RISE, 500, 100)
    ent(add(s, text(f"{key}-desc", desc, F(88, 302, 500, 64), 19, P["body"], False, "Body", ls=1.4), lid), FADE, 500, 200)
    for j, tip in enumerate(tips):
        tick_row(s, f"{key}-tip{j + 1}", tip, 88, 392 + 50 * j, 500, 350 + 120 * j, lid=lid)
    for j in range(3):
        trig(s, click(f"tab-{TABS[i][0]}"), state(f"tab-{TABS[j][0]}", "selected" if i == j else "normal"))
    trig(s, click(f"tab-{key}"), show(lid))
trig(s, {"event": "timeline_starts"}, show("layer-trips"))
trig(s, {"event": "timeline_starts"}, state("tab-trips", "selected"))
btn(s, "hz-next", "Spot the hazards  →", 976, 600, 216, start=900)
trig(s, click("hz-next"), jump("next"))

# ================================================================ Spot the hazards (hotspot question)
s = new_slide(sc, "spot", "Spot the hazards", transition=T("push", 600, **{"from": "right"}),
              notes="A hotspot question with three markers. Each marker must land on a hazard.")
eyebrow(s, "sp-eyebrow", "YOUR TURN", 88, 32)
IMG = F(88, 136, 760, 424)
ws = add(s, image("sp-scene", "workshop.jpg", "A workshop with a tall unsteady stack of boxes, a loose cable across the floor and a puddle under a table.",
                  IMG, name="Workshop picture", mask="rounded_rectangle"))
ws["shadow"] = shadow("26", 12, 30)
ent(ws, FADE, 700, 200)


def region(rid, fx0, fy0, fx1, fy1, shape_kind="rect", correct=True):
    return {"id": rid, "shape": shape_kind, "x": round(IMG["x"] + fx0 * IMG["w"]), "y": round(IMG["y"] + fy0 * IMG["h"]),
            "w": round((fx1 - fx0) * IMG["w"]), "h": round((fy1 - fy0) * IMG["h"]), "correct": correct}


cd.slide(s)["question"] = {
    "kind": "hotspot", "prompt": "Find the three hazards. Place a marker on each one.",
    "choices": [], "points": 10, "attempts": 2, "scoring": "partial", "markers": 3,
    "hotspot_image": "sp-scene",
    "regions": [region("boxes", 0.06, 0.10, 0.37, 0.92), region("cable", 0.36, 0.62, 0.67, 0.99),
                region("spill", 0.70, 0.76, 0.86, 0.93, "ellipse"),
                region("extinguisher", 0.90, 0.40, 0.99, 0.68, correct=False)],
    "frame": {"x": 88, "y": 70, "w": 760, "h": 50},
    "correct_layer": "sp-correct", "incorrect_layer": "sp-incorrect"}
side = card(s, "sp-side", F(880, 136, 312, 424), P["white"], start=300, effect=fly("right"))
ent(add(s, text("sp-side-title", "Look for", F(912, 156, 250, 36), 24, P["ink"], True, "Heading"), None), FADE, 400, 500)
for i, hint in enumerate(["Stacked too high", "Across the floor", "Slippery underfoot"]):
    tick_row(s, f"sp-hint{i + 1}", hint, 912, 214 + 56 * i, 270, 600 + 120 * i, icon=str(i + 1), icon_fill=P["tint"])
ent(add(s, text("sp-tip", "Tip: select a marker again to move it.", F(912, 400, 250, 60), 16, P["muted"], False, "Tip", ls=1.4)), FADE, 400, 1000)
btn(s, "sp-submit", "Submit", 88, 584, 180, start=800)
trig(s, click("sp-submit"), {"action": "submit_question"})
new_layer(s, "sp-correct", "Correct", prevent_base_clicks=True)
new_layer(s, "sp-incorrect", "Incorrect", prevent_base_clicks=True)
feedback(s, "sp-correct", "sp-ok-text", "Well spotted! Boxes stacked high, a loose cable and a puddle.", True, "Continue  →", "sp-ok-next", jump("next"))
feedback(s, "sp-incorrect", "sp-no-text", "Not all of your markers are on a hazard. Look again at the floor and the stack.", False, "Try again", "sp-no-retry", hide())

# ================================================================ Quiz
sc = scene("sc_quiz", "Check your knowledge")
QUESTIONS = [
    ("q1", "multiple_choice", "QUESTION 1 OF 3", "spills.jpg", 0.66, "A wet floor sign and a mop bucket beside a puddle.",
     "You notice water on the floor near a busy doorway. What should you do first?",
     [("a", "Walk around it carefully and carry on", False), ("b", "Warn people nearby and put out a wet floor sign", True),
      ("c", "Wait until the end of your shift to report it", False)],
     "Right. Warn people first, then clean it up or report it.", "Not quite. Someone could slip before it is dealt with."),
    ("q2", "multiple_response", "QUESTION 2 OF 3", "lifting.jpg", 0.66, "A worker lifting a box with bent knees.",
     "Which of these make lifting a box safer? Choose all that apply.",
     [("a", "Bend your knees and keep your back straight", True), ("b", "Twist your body to turn while you lift", False),
      ("c", "Hold the box close to your body", True), ("d", "Test the weight before you lift", True)],
     "Exactly. Bent knees, a close load and a quick test lift.", "Not quite. Twisting while you lift strains your back."),
    ("q3", "sequence", "QUESTION 3 OF 3", "trips.jpg", 0.72, "A loose cable across a walkway.",
     "You find a cable across a walkway. Put these steps in the right order.",
     [("a", "Stop and warn anyone walking past"), ("b", "Unplug the cable if it is safe to do so"),
      ("c", "Reroute it or cover it with a cable protector"), ("d", "Report it so it is fixed for good")],
     "Well done. Warn, make safe, fix, then report.", "Not quite. Think about keeping people safe first."),
]
for n, (sid, kind, eb, src, fx, alt, prompt, choices, ok, no) in enumerate(QUESTIONS, 1):
    s = new_slide(sc, sid, f"Question {n}", transition=T("push", 600, **{"from": "right"}))
    eyebrow(s, f"{sid}-eyebrow", eb, 88, 40)
    progress(s, f"{sid}-progress", n / 3, 200)
    q = {"kind": kind, "prompt": prompt, "points": 10, "attempts": 2, "frame": {"x": 88, "y": 112, "w": 660, "h": 440},
         "correct_layer": f"{sid}-correct", "incorrect_layer": f"{sid}-incorrect"}
    if kind == "sequence":
        q["choices"] = [{"id": i, "text": t} for i, t in choices]
    else:
        q["choices"] = [{"id": i, "text": t, **({"correct": True} if c else {})} for i, t, c in choices]
    if kind == "multiple_response":
        q["scoring"] = "partial"
    cd.slide(s)["question"] = q
    side_picture(s, f"{sid}-pic", src, alt, fx)
    btn(s, f"{sid}-submit", "Submit", 88, 584, 180, start=600)
    trig(s, click(f"{sid}-submit"), {"action": "submit_question"})
    new_layer(s, f"{sid}-correct", "Correct", prevent_base_clicks=True)
    new_layer(s, f"{sid}-incorrect", "Incorrect", prevent_base_clicks=True)
    last = n == len(QUESTIONS)
    feedback(s, f"{sid}-correct", f"{sid}-ok-text", ok, True, "See result  →" if last else "Continue  →", f"{sid}-ok-next", jump("next"))
    feedback(s, f"{sid}-incorrect", f"{sid}-no-text", no, False, "Try again", f"{sid}-no-retry", hide())

# ================================================================ Results
s = new_slide(sc, "results", "Your result", master=False, bg=P["deep"], transition=T("zoom", 700),
              notes="Submits the quiz on start, then shows the passed or failed layer.")
blob(s, "r-glow", F(-180, 360, 600, 600), P["primary"] + "66", start=0)
blob(s, "r-glow2", F(1010, -200, 480, 480), P["primary"] + "44", start=100)
stripe = add(s, shape("r-stripe", F(0, 0, 1280, 8), P["accent"], "rectangle", "Hi-vis stripe", decorative=True))
ent(stripe, wipe("left"), 700, 0)
ring = add(s, shape("r-ring", F(800, 150, 360, 360), "#FFFFFF14", "ellipse", "Score ring",
                    outline={"color": P["accent"], "width": 10}, decorative=True))
ent(ring, wheel(1), 1000, 300)
ent(add(s, text("r-score", "%Course.ScorePercent%%", F(800, 260, 360, 110), 80, "#FFFFFF", True, "Score", align="center", valign="middle")),
    ZOOM, 700, 900, "overshoot")
ent(add(s, text("r-score-label", "YOUR SCORE", F(800, 370, 360, 30), 16, P["tint2"], True, "Score label", align="center",
                letter_spacing=2.5)), FADE, 500, 1100)
trig(s, {"event": "timeline_starts"}, {"action": "submit_results"})
trig(s, {"event": "timeline_starts"}, show("passed"), cond=cmp("Quiz.Passed", "eq", True))
trig(s, {"event": "timeline_starts"}, show("failed"), cond=cmp("Quiz.Passed", "eq", False))
for lid, good in (("passed", True), ("failed", False)):
    new_layer(s, lid, "Passed" if good else "Failed", duration_ms=3000, hide_other_layers=True)
    tag = add(s, shape(f"{lid}-tag", F(88, 150, 170, 40), P["good"] if good else P["bad"], name="Result", adjust=[0.5],
                       label="✓  Passed" if good else "Not yet", lstyle=style(16, "#FFFFFF", True, align="center", valign="middle")), lid)
    ent(tag, POP, 450, 200, "overshoot")
    ent(add(s, text(f"{lid}-title", "You're ready to\nwork safely" if good else "Not quite\nthere yet", F(84, 206, 620, 140), 52, "#FFFFFF", True,
                    "Title", ls=1.05, ts="ts_title"), lid), RISE, 800, 300, "overshoot", "by_paragraph")
    ent(add(s, text(f"{lid}-body",
                    "Keep walkways clear, lift with your legs and deal with spills straight away." if good else
                    "You need 75% to pass. Review the three hazards and try the questions again.",
                    F(88, 360, 520, 70), 20, P["tint2"], False, "Body", ls=1.45), lid), FADE, 600, 600)
    if good:
        b = add(s, soft_button("r-finish", "Finish  ✓", F(88, 470, 180, 56), P["accent"], "#F7CB3D", P["ink"], 18), lid)
        trig(s, click("r-finish"), {"action": "exit_course"}, lid=lid)
        b2 = add(s, soft_button("r-restart", "Start again", F(284, 470, 180, 56), "#FFFFFF1F", "#FFFFFF33", "#FFFFFF", 17), lid)
        trig(s, click("r-restart"), jump("welcome"), lid=lid)
    else:
        b = add(s, soft_button("r-retry", "Try the questions again", F(88, 470, 260, 56), P["accent"], "#F7CB3D", P["ink"], 18), lid)
        trig(s, click("r-retry"), {"action": "retry_quiz"}, lid=lid)
        b2 = add(s, soft_button("r-review", "Review the hazards", F(364, 470, 220, 56), "#FFFFFF1F", "#FFFFFF33", "#FFFFFF", 17), lid)
        trig(s, click("r-review"), jump("hazards"), lid=lid)
    ent(b, POP, 450, 900, "overshoot")
    ent(b2, POP, 450, 1000, "overshoot")

path = os.path.join(HERE, "course.json")
save(path)
check(path, os.path.join(HERE, "pass.inputs.json") if os.path.exists(os.path.join(HERE, "pass.inputs.json")) else None)
