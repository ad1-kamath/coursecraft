"""Builds the four starter templates in examples/templates/.

Templates carry no media (the app loads them as course JSON only), so every picture area is a
drawn placeholder and the art is made from shapes. Each template has a theme palette, named text
styles and a master, so Design › Theme can recolour it and new slides match. Run from the
repository root after `cargo build -p coursecraft-cli`:
    python3 examples/templates/build.py
"""
import os, sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tools"))
from ccdesign import *  # noqa: E402,F401,F403
import ccdesign as cd  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
W, H = 1280, 720


def palette(**p):
    p.setdefault("white", "#FFFFFF")
    return p


QUICK = palette(ink="#1E1B4B", body="#3F3D63", muted="#6B6A8A", primary="#4338CA", primary_dk="#3730A3",
                deep="#262273", tint="#EEF0FF", tint2="#DCDFFB", accent="#F5A524", accent_text="#9A5B00",
                good="#0F7B5F", bad="#B42318", bg="#FBFBFF")
LESSON = palette(ink="#0F2A3D", body="#2E4656", muted="#5F7381", primary="#0E6E8C", primary_dk="#0B5870",
                 deep="#0A3B4F", tint="#E3F2F6", tint2="#CBE6EE", accent="#F28C6A", accent_text="#A9472E",
                 good="#0F7B5F", bad="#B42318", bg="#FAFCFC")
BRANCH = palette(ink="#2B1B33", body="#4A3652", muted="#76657D", primary="#7A2E6E", primary_dk="#62235A",
                 deep="#3E1738", tint="#F6E9F3", tint2="#ECD3E6", accent="#F2A541", accent_text="#97570C",
                 good="#0F7B5F", bad="#B42318", bg="#FFFBF7")
BLANK = palette(ink="#1F2937", body="#374151", muted="#6B7280", primary="#2563EB", primary_dk="#1D4ED8",
                deep="#1E3A8A", tint="#EFF4FF", tint2="#DBE6FE", accent="#F59E0B", accent_text="#92400E",
                good="#0F7B5F", bad="#B42318", bg="#FFFFFF")

P = cd.P = QUICK


def course(cid, title, extra=None):
    c = {"schema_version": 1, "id": cid, "title": title, "slide_size": {"w": W, "h": H},
         "theme": {"accent": P["primary"], "background": P["bg"], "text": P["ink"], "font": "Inter",
                   "player": {"bar": P["deep"], "bar_text": "#FFFFFF"},
                   "palette": [{"id": k, "name": n, "color": P[k]} for k, n in
                               [("ink", "Ink"), ("primary", "Primary"), ("primary_dk", "Primary dark"),
                                ("deep", "Deep"), ("tint", "Tint"), ("tint2", "Tint 2"),
                                ("accent", "Accent"), ("accent_text", "Accent text"), ("bg", "Background")]]},
         "text_styles": [
             {"id": "ts_title", "name": "Title", "style": style(52, P["ink"], True), "paragraphs": {"line_spacing": 1.05}},
             {"id": "ts_heading", "name": "Heading", "style": style(36, P["ink"], True), "paragraphs": {"line_spacing": 1.1}},
             {"id": "ts_eyebrow", "name": "Eyebrow", "style": style(16, P["primary"], True, letter_spacing=2.5)},
             {"id": "ts_body", "name": "Body", "style": style(20, P["body"]), "paragraphs": {"line_spacing": 1.45}},
         ],
         "masters": [{"id": "m_content", "title": "Content", "background": P["bg"], "duration_ms": 5000, "objects": [
             obj("m_corner", "Corner shape", {"type": "shape", "shape": "ellipse", "fill": P["tint"] + "B3"}, F(1090, -150, 360, 300), decorative=True),
             obj("m_dot", "Brand dot", {"type": "shape", "shape": "ellipse", "fill": P["accent"]}, F(88, 684, 8, 8), decorative=True),
             obj("m_brand", "Footer", {"type": "text", "text": title, "style": style(12, P["muted"], True, letter_spacing=1)}, F(104, 677, 400, 20), decorative=True),
         ]}],
         "scenes": []}
    if extra:
        c.update(extra)
    return use(c)


def scene(sid, title):
    sc = {"id": sid, "title": title, "slides": []}
    cd.C["scenes"].append(sc)
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


# ================================================================ Quick Knowledge Check
P = cd.P = QUICK
course("quick-knowledge-check", "Quick Knowledge Check", {"quiz": {"passing_percent": 80}})
sc = scene("sc-quiz", "Knowledge Check")

s = new_slide(sc, "s-intro", "Introduction", master=False, transition=T("fade", 500),
              notes="Replace the title, the short description and the three facts. The art on the right is made of shapes: recolour it with Design › Theme.")
eyebrow(s, "intro-eyebrow", "KNOWLEDGE CHECK", 88, 170)
title(s, "intro-title", "Quick Knowledge\nCheck", 84, 206, 600, 140, 56)
body(s, "intro-body", "Answer two short questions to check your understanding. You need 80% to pass.", 88, 360, 520, 70)
for i, (lab, w) in enumerate([("2 questions", 130), ("About 3 minutes", 160), ("Pass mark 80%", 150)]):
    pill(s, f"intro-chip{i + 1}", lab, 88 + sum([130, 160, 150][:i]) + 12 * i, 452, w, start=900 + 120 * i)
btn(s, "btn-start", "Start the quiz  →", 88, 530, 230, start=1300)
emph(find(s, "btn-start"), "pulse", 2600, 1200, 2)
trig(s, click("btn-start"), jump("next"))
# art: a deep panel with stacked answer cards
panel = add(s, shape("intro-panel", F(720, 0, 560, 720), P["deep"], "rectangle", "Art panel", decorative=True))
ent(panel, wipe("right"), 700, 0, "ease_in_out")
blob(s, "intro-ring", F(820, 90, 420, 420), P["primary"] + "66", start=300)
blob(s, "intro-ring2", F(1060, 470, 260, 260), P["primary"] + "44", start=400)
for i, (y, rot, mark) in enumerate([(176, -4, False), (300, 3, True), (424, -2, False)]):
    c = card(s, f"intro-card{i + 1}", F(800, y, 380, 96), start=500 + 150 * i, radius=0.18)
    c["rotation"] = rot
    dot = add(s, shape(f"intro-card{i + 1}-dot", F(826, y + 30, 36, 36), P["good"] if mark else P["tint2"], "ellipse",
                       "Answer mark", label="✓" if mark else "", lstyle=style(18, "#FFFFFF", True, align="center", valign="middle"),
                       decorative=True))
    dot["rotation"] = rot
    ent(dot, POP, 400, 650 + 150 * i, "overshoot")
    for j, (w, col) in enumerate([(220, P["tint2"]), (150, P["tint"])]):
        ln = add(s, shape(f"intro-card{i + 1}-line{j + 1}", F(880, y + 30 + 22 * j, w, 12), col, name="Answer line",
                          adjust=[0.5], decorative=True))
        ln["rotation"] = rot
        ent(ln, wipe("left"), 500, 700 + 150 * i + 80 * j)
star = add(s, shape("intro-star", F(1150, 150, 64, 64), P["accent"], "star_5", "Star", decorative=True))
ent(star, SPIN_GROW, 900, 1100, "overshoot")
emph(star, "teeter", 2400, 900)

for n, (sid, prompt, choices, ok, no, last) in enumerate([
    ("s-q1", "Replace this with your first question. Keep it short and clear.",
     [("a", "Option A (a believable wrong answer)", False), ("b", "Option B (the correct answer)", True),
      ("c", "Option C (a believable wrong answer)", False)],
     "Correct! Add one sentence that explains why.", "Not quite. Give a helpful hint and let the learner try again.", False),
    ("s-q2", "Replace this with your second question. One idea per question works best.",
     [("a", "Option A (the correct answer)", True), ("b", "Option B (a believable wrong answer)", False),
      ("c", "Option C (a believable wrong answer)", False)],
     "Well done! Continue to see your result.", "Not quite. Read the question again and have another go.", True)], 1):
    s = new_slide(sc, sid, f"Question {n}", transition=T("push", 600, **{"from": "right"}))
    q = n - 1
    eyebrow(s, f"q{n}-eyebrow", f"QUESTION {n} OF 2", 88, 40)
    progress(s, f"q{n}-progress", n / 2, 200)
    cd.slide(s)["question"] = {
        "kind": "multiple_choice", "prompt": prompt,
        "choices": [{"id": i, "text": t, **({"correct": True} if c else {})} for i, t, c in choices],
        "points": 10, "attempts": 2, "frame": {"x": 88, "y": 120, "w": 700, "h": 420},
        "correct_layer": f"q{n}-correct", "incorrect_layer": f"q{n}-incorrect"}
    # side art: a numbered panel with a hint
    side = card(s, f"q{n}-side", F(852, 120, 340, 420), P["tint"], start=200, effect=fly("right"))
    side.pop("shadow")
    num = add(s, text(f"q{n}-num", f"0{n}", F(888, 150, 200, 110), 96, P["primary"], True, "Question number", decorative=True))
    ent(num, RISE, 700, 400, "overshoot")
    add(s, shape(f"q{n}-tip-dot", F(892, 300, 10, 10), P["accent"], "ellipse", "Hint dot", decorative=True))
    ent(add(s, text(f"q{n}-tip", "Choose one answer, then select Submit. You have two tries.", F(912, 290, 250, 90),
                    17, P["body"], False, "Hint", ls=1.4)), FADE, 500, 600)
    ent(add(s, shape(f"q{n}-art1", F(900, 420, 120, 90), P["primary"] + "33", "ellipse", "Decoration", decorative=True)), ZOOM, 800, 500)
    ent(add(s, shape(f"q{n}-art2", F(1010, 400, 140, 110), P["accent"] + "55", "rounded_rectangle", "Decoration", adjust=[0.3],
                     decorative=True)), ZOOM, 800, 650)
    btn(s, f"q{n}-submit", "Submit", 88, 584, 180, start=600)
    trig(s, click(f"q{n}-submit"), {"action": "submit_question"})
    new_layer(s, f"q{n}-correct", "Correct", prevent_base_clicks=True)
    new_layer(s, f"q{n}-incorrect", "Incorrect", prevent_base_clicks=True)
    feedback(s, f"q{n}-correct", f"q{n}-ok-text", ok, True, "See result  →" if last else "Continue  →", f"q{n}-ok-next", jump("next"))
    feedback(s, f"q{n}-incorrect", f"q{n}-no-text", no, False, "Try again", f"q{n}-no-retry", hide())

s = new_slide(sc, "s-results", "Results", master=False, bg=P["deep"], transition=T("zoom", 700))
blob(s, "res-glow", F(-160, 380, 560, 560), P["primary"] + "55", start=0)
blob(s, "res-glow2", F(1000, -180, 460, 460), P["primary"] + "44", start=100)
eyebrow(s, "res-eyebrow", "YOUR RESULT", 88, 150, color=P["tint2"])
title(s, "results-heading", "Here's how you did", 84, 186, 600, 70, 48, "#FFFFFF")
body(s, "results-body", "Your score and the pass mark are below. Change this text to tell learners what to do next.",
     88, 270, 540, 70, color=P["tint2"])
ring = add(s, shape("res-ring", F(800, 150, 360, 360), "#FFFFFF14", "ellipse", "Score ring",
                    outline={"color": P["accent"], "width": 10}, decorative=True))
ent(ring, wheel(1), 1000, 300)
score = add(s, text("results-score", "%Course.ScorePercent%%", F(800, 260, 360, 110), 80, "#FFFFFF", True, "Score",
                    align="center", valign="middle"))
ent(score, ZOOM, 700, 900, "overshoot")
ent(add(s, text("res-score-label", "YOUR SCORE", F(800, 370, 360, 30), 16, P["tint2"], True, "Score label", align="center",
                letter_spacing=2.5)), FADE, 500, 1100)
pill(s, "res-pass", "Pass mark  80%", 88, 380, 220, "#FFFFFF1F", "#FFFFFF", start=1200)
trig(s, {"event": "timeline_starts"}, {"action": "submit_results"})
save(os.path.join(HERE, "quick-knowledge-check.json"))

# ================================================================ Linear Lesson
P = cd.P = LESSON
course("linear-lesson", "Linear Lesson")
sc = scene("sc-lesson", "Lesson")

s = new_slide(sc, "s-title", "Welcome", master=False, bg=P["deep"], transition=T("fade", 600),
              notes="Replace the title and the one-line promise. Add your own picture in the placeholder, or delete it.")
blob(s, "t-wave", F(-120, 470, 1520, 420), P["primary"] + "66", "wave", start=0)
blob(s, "t-circle", F(1010, -140, 420, 420), P["primary"] + "55", start=100)
blob(s, "t-dot", F(960, 90, 26, 26), P["accent"], start=900)
eyebrow(s, "title-eyebrow", "LESSON 1  ·  10 MINUTES", 88, 190, color=P["tint2"])
title(s, "title-heading", "Lesson title goes\nright here", 84, 226, 640, 140, 56, "#FFFFFF")
body(s, "title-sub", "A short, structured lesson with objectives, key ideas and a wrap-up.", 88, 380, 560, 70, color=P["tint2"])
ent(add(s, soft_button("btn-begin", "Begin the lesson  →", F(88, 480, 250, 56), P["accent"], "#F6A88C", P["deep"], 18,
                      shadow=shadow("33", 8, 18))), POP, 450, 1200, "overshoot")
trig(s, click("btn-begin"), jump("next"))
placeholder(s, "title-pic", F(760, 150, 420, 400), start=400, fill="#FFFFFF14")

s = new_slide(sc, "s-objectives", "Learning Objectives", transition=T("push", 600, **{"from": "right"}))
eyebrow(s, "obj-eyebrow", "WHAT YOU'LL LEARN", 88, 96)
title(s, "obj-heading", "By the end of this lesson, you will be able to", 84, 128, 900, 110, 40)
for i, txt in enumerate(["Explain the core idea in plain language", "Apply the three-step process to your daily work",
                         "Spot common mistakes before they happen"]):
    x = 88 + i * 372
    card(s, f"obj-card{i + 1}", F(x, 300, 348, 260), start=600 + 200 * i)
    n = add(s, shape(f"obj-num{i + 1}", F(x + 28, 328, 56, 56), P["tint"], "ellipse", f"Number {i + 1}", label=f"0{i + 1}",
                     lstyle=style(20, P["primary"], True, align="center", valign="middle"), decorative=True))
    ent(n, POP, 400, 800 + 200 * i, "overshoot")
    ent(add(s, text(f"obj-item{i + 1}", txt, F(x + 28, 404, 292, 120), 22, P["ink"], True, f"Objective {i + 1}", ls=1.3)),
        FADE, 500, 900 + 200 * i)
btn(s, "obj-next", "Continue  →", 982, 600, 210, start=1500)
trig(s, click("obj-next"), jump("next"))

s = new_slide(sc, "s-concepts", "Key Concepts", transition=T("push", 600, **{"from": "right"}))
cd.slide(s)["revisit"] = "reset"
eyebrow(s, "con-eyebrow", "KEY IDEAS", 88, 96)
title(s, "con-heading", "Explore the three steps", 84, 128, 800, 60, 40)
ent(add(s, text("con-intro", "Select each step to read about it.", F(88, 190, 600, 30), 18, P["muted"], False, "Instruction")), FADE, 500, 500)
for i, lab in enumerate(["1  Prepare", "2  Do", "3  Review"]):
    b = add(s, soft_button(f"tab-{i + 1}", lab, F(88 + i * 196, 240, 184, 50), P["tint"], P["tint2"], P["primary_dk"], radius=0.5))
    b["states"].append({"name": "selected", "fill": P["primary"], "text_style": style(17, "#FFFFFF", True, align="center", valign="middle")})
    ent(b, floaty("bottom"), 450, 600 + 120 * i)
    trig(s, click(f"tab-{i + 1}"), show(f"step-{i + 1}"))
    for j in range(3):
        trig(s, click(f"tab-{i + 1}"), state(f"tab-{j + 1}", "selected" if j == i else "normal"))
for i, (head, txt) in enumerate([("Prepare", "Gather what you need and confirm the goal before you start. Replace this with your own step."),
                                 ("Do", "Follow the checklist in order and note anything unusual. Replace this with your own step."),
                                 ("Review", "Check the result and share a short summary with your team. Replace this with your own step.")]):
    lid = f"step-{i + 1}"
    new_layer(s, lid, f"Step {i + 1}", hide_other_layers=True)
    c = card(s, f"step-{i + 1}-card", F(88, 316, 1104, 260), start=0, lid=lid, effect=FADE)
    ent(add(s, text(f"step-{i + 1}-head", head, F(124, 346, 540, 50), 30, P["ink"], True, "Step heading"), lid), RISE, 500, 100)
    ent(add(s, text(f"step-{i + 1}-text", txt, F(124, 404, 540, 140), 20, P["body"], False, "Step text", ls=1.45), lid), FADE, 500, 200)
    placeholder(s, f"step-{i + 1}-pic", F(720, 340, 444, 212), start=150, lid=lid)
btn(s, "con-next", "Continue  →", 982, 600, 210, start=1200)
trig(s, click("con-next"), jump("next"))
trig(s, {"event": "timeline_starts"}, show("step-1"))
trig(s, {"event": "timeline_starts"}, state("tab-1", "selected"))

s = new_slide(sc, "s-summary", "Summary", master=False, bg=P["deep"], transition=T("fade", 600))
blob(s, "sum-glow", F(880, 300, 560, 560), P["primary"] + "55", start=0)
eyebrow(s, "sum-eyebrow", "LESSON COMPLETE", 88, 110, color=P["tint2"])
title(s, "sum-heading", "Key takeaways", 84, 142, 700, 70, 48, "#FFFFFF")
for i, txt in enumerate(["Write your first takeaway here", "Write your second takeaway here", "Write your third takeaway here"]):
    y = 250 + i * 92
    row = add(s, shape(f"sum-row{i + 1}", F(88, y, 640, 72), "#FFFFFF14", name=f"Takeaway {i + 1}", adjust=[0.2], decorative=True))
    ent(row, fly("left"), 500, 500 + 200 * i)
    ic = add(s, shape(f"sum-check{i + 1}", F(108, y + 16, 40, 40), P["accent"], "ellipse", "Check", label="✓",
                      lstyle=style(20, P["deep"], True, align="center", valign="middle"), decorative=True))
    ent(ic, POP, 400, 700 + 200 * i, "overshoot")
    ent(add(s, text(f"sum-item{i + 1}", txt, F(168, y + 18, 540, 40), 20, "#FFFFFF", True, f"Takeaway {i + 1} text", valign="middle")),
        FADE, 400, 750 + 200 * i)
ent(add(s, text("sum-body", "Add a next step here: a job aid, a follow-up lesson or who to ask.", F(88, 540, 640, 60), 18, P["tint2"], False,
                "Next step", ls=1.4)), FADE, 500, 1400)
trig(s, {"event": "timeline_starts"}, {"action": "complete_course", "status": "completed"})
save(os.path.join(HERE, "linear-lesson.json"))

# ================================================================ Branching Scenario
P = cd.P = BRANCH
course("branching-scenario", "Branching Scenario")
sc = scene("sc-scenario", "Scenario")


def person(sid, oid, x, y, scale=1.0, fill=None, start=0):
    """A character drawn with the person shape: replace it with a picture of your own."""
    fill = fill or P["tint2"]
    p = add(sid, shape(oid, F(x, y, int(240 * scale), int(300 * scale)), fill, "person", "Character", decorative=True))
    ent(p, floaty("bottom"), 700, start)
    return p


s = new_slide(sc, "s-setup", "The Situation", master=False, transition=T("fade", 600),
              notes="Set the scene: who the learner is, where they are and what is at stake. Replace the drawn character with a picture if you have one.")
add(s, shape("setup-panel", F(700, 0, 580, 720), P["tint"], "rectangle", "Art panel", decorative=True))
ent(find(s, "setup-panel"), wipe("right"), 700, 0, "ease_in_out")
blob(s, "setup-sun", F(1060, 80, 140, 140), P["accent"] + "AA", start=300)
person(s, "setup-person", 860, 260, 1.15, P["primary"] + "55", start=400)
bub = add(s, shape("setup-bubble", F(760, 120, 300, 110), P["white"], "callout_rounded", "Speech bubble",
                   label="“We need this out by Friday.”", lstyle=style(18, P["ink"], True, align="center", valign="middle"),
                   decorative=True))
bub["shadow"] = shadow()
ent(bub, POP, 500, 1000, "overshoot")
eyebrow(s, "setup-eyebrow", "SCENARIO", 88, 170)
title(s, "setup-title", "The situation", 84, 206, 560, 70, 56)
body(s, "setup-body", "Set the scene for your learner here. Describe the situation, the stakes and the role they play.",
     88, 300, 540, 100)
pill(s, "setup-chip1", "Your role: team lead", 88, 420, 200, start=900)
pill(s, "setup-chip2", "2 choices", 300, 420, 120, start=1020)
btn(s, "btn-enter", "Enter the scenario  →", 88, 500, 260, start=1300)
trig(s, click("btn-enter"), jump("next"))

s = new_slide(sc, "s-decision", "Decision Point", transition=T("push", 600, **{"from": "right"}))
eyebrow(s, "dec-eyebrow", "DECISION POINT", 88, 96)
title(s, "dec-heading", "What do you do next?", 84, 128, 900, 60, 44)
body(s, "dec-body", "A colleague asks to skip the usual review to hit a tight deadline. Choose how you respond.", 88, 196, 900, 60)
for i, (letter, head, txt, target) in enumerate([
        ("A", "Agree to skip the review", "Keep the deadline and trust that it's fine this time.", "s-outcome-a"),
        ("B", "Offer a focused 15-minute review", "Check only the risky parts together, then ship.", "s-outcome-b")]):
    x = 88 + i * 564
    oid = f"btn-branch-{letter.lower()}"
    cardo = add(s, shape(oid, F(x, 300, 540, 260), P["white"], name=f"Option {letter}", adjust=[0.08]))
    cardo["shadow"] = shadow()
    cardo["states"] = [{"name": "hover", "fill": P["tint"]}]
    cardo["description"] = f"Option {letter}: {head}"
    ent(cardo, floaty("bottom"), 600, 600 + 200 * i)
    badge = add(s, shape(f"dec-badge-{letter.lower()}", F(x + 32, 332, 56, 56), P["primary"], "ellipse", f"Letter {letter}", label=letter,
                         lstyle=style(24, "#FFFFFF", True, align="center", valign="middle"), decorative=True))
    ent(badge, POP, 400, 800 + 200 * i, "overshoot")
    ent(add(s, text(f"dec-head-{letter.lower()}", head, F(x + 32, 412, 470, 44), 24, P["ink"], True, "Option title", decorative=True)),
        FADE, 400, 850 + 200 * i)
    ent(add(s, text(f"dec-text-{letter.lower()}", txt, F(x + 32, 460, 470, 70), 18, P["body"], False, "Option text", ls=1.4,
                    decorative=True)), FADE, 400, 900 + 200 * i)
    ent(add(s, text(f"dec-go-{letter.lower()}", "Choose  →", F(x + 400, 332, 110, 30), 15, P["primary"], True, "Choose hint",
                    decorative=True, align="right")), FADE, 400, 1000 + 200 * i)
    trig(s, click(oid), jump(target))

for sid, ttl, good, head, txt, buttons in [
        ("s-outcome-a", "Outcome A", False, "The review was skipped",
         "The release goes out on time, but a preventable problem reaches customers and needs an urgent fix.",
         [("btn-a-retry", "Try another choice", "s-decision", False), ("btn-a-debrief", "Go to the debrief  →", "s-debrief", True)]),
        ("s-outcome-b", "Outcome B", True, "A focused review",
         "Together you catch a small mistake in ten minutes and still meet the deadline with confidence.",
         [("btn-b-debrief", "Continue to the debrief  →", "s-debrief", True)])]:
    s = new_slide(sc, sid, ttl, transition=T("fade", 500))
    tone = P["good"] if good else P["bad"]
    letter = sid[-1]
    band = add(s, shape(f"out-{letter}-band", F(0, 0, 1280, 8), tone, "rectangle", "Result band", decorative=True))
    ent(band, wipe("left"), 600, 0)
    tag = add(s, shape(f"out-{letter}-tag", F(88, 110, 210, 40), tone, name="Result", adjust=[0.5],
                       label="✓  Good outcome" if good else "⚠  Costly outcome",
                       lstyle=style(16, "#FFFFFF", True, align="center", valign="middle")))
    ent(tag, POP, 450, 200, "overshoot")
    title(s, f"out-{letter}-title", head, 84, 170, 620, 70, 48)
    body(s, f"out-{letter}-body", txt, 88, 256, 560, 110)
    for j, (bid, lab, target, primary) in enumerate(buttons):
        x = 88 + j * 250
        if primary:
            btn(s, bid, lab, x, 420, 300 if len(buttons) == 1 else 270, start=900)
        else:
            ent(add(s, soft_button(bid, lab, F(x, 420, 230, 56), P["tint"], P["tint2"], P["primary_dk"])), POP, 450, 900, "overshoot")
        trig(s, click(bid), jump(target))
    art = add(s, shape(f"out-{letter}-art", F(760, 110, 420, 460), P["tint"], name="Art panel", adjust=[0.08], decorative=True))
    ent(art, fly("right"), 600, 100)
    person(s, f"out-{letter}-person", 860, 230, 1.0, P["primary"] + "55", start=300)
    mark = add(s, shape(f"out-{letter}-mark", F(1040, 160, 96, 96), tone, "ellipse", "Outcome mark", label="✓" if good else "!",
                        lstyle=style(44, "#FFFFFF", True, align="center", valign="middle"), decorative=True))
    ent(mark, SPIN_GROW, 700, 700, "overshoot")
    emph(mark, "jump" if good else "shake", 1600, 800)

s = new_slide(sc, "s-debrief", "Debrief", master=False, bg=P["deep"], transition=T("zoom", 700))
blob(s, "deb-glow", F(900, -120, 520, 520), P["primary"] + "66", start=0)
eyebrow(s, "deb-eyebrow", "DEBRIEF", 88, 110, color=P["tint2"])
title(s, "deb-title", "What made the difference", 84, 142, 900, 70, 48, "#FFFFFF")
body(s, "deb-body", "Explain why the stronger choice worked and link it to your team's guidelines.", 88, 220, 760, 60,
     color=P["tint2"])
for i, (head, txt) in enumerate([("Pause", "Name the risk before you decide."), ("Size it", "A small, focused check costs little."),
                                 ("Share it", "Tell the team what you found.")]):
    x = 88 + i * 372
    c = add(s, shape(f"deb-card{i + 1}", F(x, 320, 348, 220), "#FFFFFF14", name=f"Lesson {i + 1}", adjust=[0.08], decorative=True))
    ent(c, floaty("bottom"), 600, 600 + 200 * i)
    ent(add(s, text(f"deb-num{i + 1}", f"0{i + 1}", F(x + 28, 344, 100, 50), 34, P["accent"], True, "Number", decorative=True)),
        POP, 400, 800 + 200 * i, "overshoot")
    ent(add(s, text(f"deb-head{i + 1}", head, F(x + 28, 404, 290, 40), 24, "#FFFFFF", True, f"Lesson {i + 1} title")), FADE, 400,
        850 + 200 * i)
    ent(add(s, text(f"deb-text{i + 1}", txt, F(x + 28, 448, 290, 70), 18, P["tint2"], False, f"Lesson {i + 1} text", ls=1.4)), FADE,
        400, 900 + 200 * i)
trig(s, {"event": "timeline_starts"}, {"action": "complete_course", "status": "completed"})
save(os.path.join(HERE, "branching-scenario.json"))

# ================================================================ Blank
P = cd.P = BLANK
course("blank-course", "Untitled Course")
c = cd.C
c.pop("masters")
sc = scene("sc-1", "Scene 1")
s = new_slide(sc, "s-1", "Slide 1", master=False)
trig(s, {"event": "timeline_ends"}, {"action": "complete_course", "status": "completed"})
save(os.path.join(HERE, "blank.json"))

for f in ("quick-knowledge-check", "linear-lesson", "branching-scenario", "blank"):
    print("==", f)
    check(os.path.join(HERE, f + ".json"))
