"""Builds the 'Healthy Habits at Work' welcome course: a polished wellbeing module that shows off CourseCraft.

Pass 1 runs engine commands (inserts give the ready-made interactions, questions, results and certificate).
Pass 2 edits course.json directly: copy, layout, extra objects, triggers, layers, animations, masters and settings.
Run from the repository root after `cargo build -p coursecraft-cli`:
    python3 examples/healthy-habits/build.py
It rewrites examples/healthy-habits/course.json and the caption files. Working files go to target/healthy-habits-build/.
Media in media/ is not rebuilt here: the pictures came from Gemini (through the Antigravity CLI), the narration
and the music from the Gemini API (`gemini-api tts` with the Sulafat voice, `gemini-api music`). Sentence
timings for the captions are in media/narration.json.
"""
import copy, json, os, subprocess, sys

ROOT = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(ROOT, "../.."))
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
CLI = os.path.join(TARGET, "debug/coursecraft-cli")
OUT = os.path.join(TARGET, "healthy-habits-build")
os.makedirs(OUT, exist_ok=True)

# ------------------------------------------------------------------ design tokens
INK, BODY, MUTED = "#1F2D2B", "#33433F", "#5C6B68"
FOREST, DEEP, TEAL, TEAL_DK = "#163B37", "#1E4D49", "#2F6F6A", "#245A55"
SAGE, MIST, CREAM, SAND, WHITE = "#A8C3B1", "#E6EFEA", "#FBF6EE", "#F4EBDD", "#FFFFFF"
CORAL, CORAL_DK, BRIGHT, BLUSH = "#B84C30", "#963A22", "#E8775A", "#FCE6DE"
GOLD = "#E9B44C"              # decorative only
ON_DARK, ON_DARK_SOFT = "#FBF6EE", "#CFE0D6"
SERIF = "DM Serif Display"    # custom font from Google Fonts (SIL OFL), in media/fonts/

SOFT = {"offset_x": 0, "offset_y": 14, "blur": 34, "color": "#1F2D2B24"}
CARD = {"offset_x": 0, "offset_y": 8, "blur": 22, "color": "#1F2D2B1C"}
LIFT = {"offset_x": 0, "offset_y": 8, "blur": 18, "color": "#B84C3050"}
TEAL_LIFT = {"offset_x": 0, "offset_y": 6, "blur": 16, "color": "#1E4D4950"}

NARR = json.load(open(os.path.join(ROOT, "media", "narration.json")))

# ------------------------------------------------------------------ pass 1
SCENES = [("sc_welcome", "Welcome"), ("sc_move", "Lesson 1: Move"), ("sc_breathe", "Lesson 2: Breathe"),
          ("sc_connect", "Lesson 3: Connect"), ("sc_quiz", "Final quiz"), ("sc_reflect", "Reflect and finish"),
          ("sc_extras", "Extras")]
SLIDES = [
    ("sc_welcome", "name", "Nice to meet you"), ("sc_welcome", "habits", "Three small habits"),
    ("sc_move", "move_intro", "Lesson 1: Move"), ("sc_move", "desk", "Set up your desk"),
    ("sc_move", "video", "Three desk stretches"), ("sc_move", "eyes", "Rest your eyes"),
    ("sc_move", "myth", "Myth or fact?"), ("sc_move", "sort", "Helps or hurts?"),
    ("sc_breathe", "breathe_intro", "Lesson 2: Breathe"), ("sc_breathe", "breathe", "Breathe with the circle"),
    ("sc_breathe", "steps", "A one-minute reset"), ("sc_breathe", "checkin", "How are you today?"),
    ("sc_connect", "connect_intro", "Lesson 3: Connect"), ("sc_connect", "scenario", "What would you do?"),
    ("sc_connect", "day", "A healthy day"), ("sc_connect", "tips", "Six quick wins"),
    ("sc_connect", "plan", "Make your plan"),
]
# q1-q3 close each lesson as a quick check; q4-q9 are the final quiz. All are graded.
QUIZ = [("q1", "multiple_choice", "sc_move"), ("q2", "multiple_response", "sc_breathe"), ("q3", "true_false", "sc_connect"),
        ("q4", "fill_in", "sc_quiz"), ("q5", "sequence", "sc_quiz"), ("q6", "matching", "sc_quiz"),
        ("q7", "hotspot", "sc_quiz"), ("q8", "drag_drop", "sc_quiz"), ("q9", "freeform", "sc_quiz")]
REFLECT = [("r1", "likert"), ("r2", "short_answer"), ("r3", "essay")]
INSERTS = {"myth": "flip_cards", "steps": "process_steps", "scenario": "scenario_choice",
           "day": "timeline", "tips": "reveal_grid"}

cmds = []
def c(command, **params):
    cmds.append({"command": command, "params": params})

c("course.update", title="Healthy Habits at Work", id="healthy-habits-at-work")
c("course.set_theme", accent=TEAL, background=CREAM, text=INK, font="Inter", bar=FOREST, bar_text=CREAM)
c("theme.set_palette", colors=[
    {"name": "Ink", "color": INK}, {"name": "Forest", "color": FOREST}, {"name": "Deep teal", "color": DEEP},
    {"name": "Teal", "color": TEAL}, {"name": "Sage", "color": SAGE}, {"name": "Mist", "color": MIST},
    {"name": "Coral", "color": BRIGHT}, {"name": "Terracotta", "color": CORAL}, {"name": "Sand", "color": SAND},
    {"name": "Cream", "color": CREAM}])
c("course.set_player", show_title=True, show_menu=True, show_seekbar=True, show_transcript=True,
  show_captions=True, navigation="free")
c("course.set_quiz", passing_percent=80)
for sid, title in SCENES[1:]:
    c("scene.add", id=sid, title=title)
prev = "slide1"
for scene, sid, title in SLIDES:
    if scene == "sc_welcome":
        c("slide.add", id=sid, after=prev, title=title); prev = sid
    else:
        c("slide.add", id=sid, scene=scene, title=title)
    if sid == "sort":
        c("slide.add", id="q1", scene="sc_move", title="Quick check")
    if sid == "checkin":
        c("slide.add", id="q2", scene="sc_breathe", title="Quick check")
c("slide.add", id="q3", scene="sc_connect", title="Quick check")
for sid, kind, scene in QUIZ[3:]:
    c("slide.add", id=sid, scene=scene, title=f"Question {int(sid[1:]) - 3}")
for sid, kind in REFLECT:
    c("slide.add", id=sid, scene="sc_reflect", title=f"Reflection {sid[1:]}")
c("slide.add", id="finish", scene="sc_reflect", title="Well done")
c("slide.add", id="bonus", scene="sc_extras", title="Bonus: blink and the 20-20-20 rule")
c("variable.add", name="change_minutes", value=60)
for sid, kind in INSERTS.items():
    c("interaction.insert", slide=sid, kind=kind)
for sid, kind, _ in QUIZ:
    c("question.insert", slide=sid, kind=kind)
for sid, kind in REFLECT:
    c("question.insert", slide=sid, kind=kind)
c("results.insert", after="q9")
c("certificate.insert", after="r3")
json.dump(cmds, open(os.path.join(OUT, "pass1.script.json"), "w"), indent=1)

base = os.path.join(OUT, "base.json")
subprocess.run([CLI, "new", base, "--title", "Healthy Habits at Work"], check=True, capture_output=True)
r = subprocess.run([CLI, "run", base, os.path.join(OUT, "pass1.script.json"), "-o", os.path.join(OUT, "pass1.json")],
                   capture_output=True, text=True)
if r.returncode != 0:
    print(r.stdout[-3000:], r.stderr[-3000:]); sys.exit(1)

# ------------------------------------------------------------------ pass 2 helpers
C = json.load(open(os.path.join(OUT, "pass1.json")))
C["scenes"][0]["id"], C["scenes"][0]["title"] = "sc_welcome", "Welcome"
C["scenes"][0]["slides"][0]["id"] = "welcome"
C["scenes"][0]["slides"][0]["title"] = "Welcome"

def slide(sid):
    for sc in C["scenes"]:
        for s in sc["slides"]:
            if s["id"] == sid:
                return s
    raise KeyError(sid)

def layer(sid, lid):
    for l in slide(sid).get("layers", []):
        if l["id"] == lid:
            return l
    raise KeyError(lid)

def find(sid, oid):
    s = slide(sid)
    for o in s.get("objects", []):
        if o["id"] == oid:
            return o
    for l in s.get("layers", []):
        for o in l.get("objects", []):
            if o["id"] == oid:
                return o
    raise KeyError(f"{sid}/{oid}")

def F(x, y, w, h):
    return {"x": x, "y": y, "w": w, "h": h}

def style(size=20, color=INK, bold=False, **kw):
    s = {"font": "Inter", "size": size, "color": color, "bold": bold}
    s.update(kw)
    return s

def add(sid, o, lid=None, index=None):
    target = layer(sid, lid) if lid else slide(sid)
    lst = target.setdefault("objects", [])
    if index is None:
        lst.append(o)
    else:
        lst.insert(index, o)
    return o

def obj(oid, name, kind, frame, **extra):
    o = {"id": oid, "name": name, "kind": kind, "frame": frame}
    o.update(extra)
    return o

def text(oid, s, frame, size=20, color=INK, bold=False, name=None, ls=None, paras=None, **st):
    k = {"type": "text", "text": s, "style": style(size, color, bold, **st)}
    if paras:
        k["paragraphs"] = paras
    elif ls:
        k["paragraphs"] = [{"line_spacing": ls} for _ in s.split("\n")]
    return obj(oid, name or oid, k, frame)

def eyebrow(sid, oid, s, x, y, color=CORAL, w=640, lid=None, start=None):
    """A short rule and a spaced capital label: the course's section marker."""
    rule = add(sid, obj(f"{oid}_rule", "Eyebrow rule", {"type": "shape", "shape": "rectangle", "fill": BRIGHT if color in (CORAL, CORAL_DK) else color},
                        F(x, y + 9, 28, 3), decorative=True), lid)
    o = text(oid, s, F(x + 40, y - 1, w, 24), 16, color, True, "Eyebrow", letter_spacing=2.5)
    o["kind"]["text_style"] = "ts_eyebrow"
    add(sid, o, lid)
    if start is not None:
        ent(rule, wipe("left"), 450, start)
        ent(o, FADE, 500, start + 150)
    return o

def heading(oid, s, x, y, w=900, h=70, size=46, color=INK):
    o = text(oid, s, F(x, y, w, h), size, color, False, "Title", ls=1.02, font=SERIF)
    o["kind"]["text_style"] = "ts_heading"
    return o

def body(oid, s, x, y, w, h, size=20, color=BODY, name="Body"):
    o = text(oid, s, F(x, y, w, h), size, color, False, name, ls=1.45)
    o["kind"]["text_style"] = "ts_body"
    return o

def shape(oid, frame, fill, kind="rounded_rectangle", name=None, label=None, lstyle=None, adjust=None, outline=None):
    k = {"type": "shape", "shape": kind, "fill": fill}
    if label is not None:
        k["text"] = {"text": label, "style": lstyle or style(18, INK, True, align="center", valign="middle")}
    if adjust:
        k["adjust"] = adjust
    if outline:
        k["outline"] = outline
    return obj(oid, name or oid, k, frame)

def pill(oid, label, frame, fill=MIST, color=DEEP, size=15, name=None):
    return shape(oid, frame, fill, name=name or label, label=label, lstyle=style(size, color, True, align="center", valign="middle"), adjust=[0.5])

def button(oid, label, frame, fill=TEAL, hover=None, name=None):
    o = obj(oid, name or f"{label.strip(' →')} button", {"type": "button", "label": label, "fill": fill}, frame)
    o["states"] = [{"name": "hover", "fill": hover or {TEAL: TEAL_DK, CORAL: CORAL_DK, DEEP: FOREST, FOREST: INK}.get(fill, fill)}]
    o["shadow"] = LIFT if fill == CORAL else TEAL_LIFT
    return o

def image(oid, src, alt, frame, crop=None, name=None, mask=None):
    k = {"type": "image", "src": src, "alt": alt}
    if crop:
        k["crop"] = crop
    if mask:
        k["shape"] = mask
    return obj(oid, name or oid, k, frame)

def crop_for(frame, x=0.5, src_ratio=1376 / 768):
    """A crop that fills `frame` from a 16:9-ish picture without stretching, centred on fraction `x`."""
    r = frame["w"] / frame["h"]
    if r < src_ratio:
        w = r / src_ratio
        return {"x": round(min(max(x - w / 2, 0), 1 - w), 4), "y": 0.0, "w": round(w, 4), "h": 1.0}
    h = src_ratio / r
    return {"x": 0.0, "y": round((1 - h) / 2, 4), "w": 1.0, "h": round(h, 4)}

def picture_card(oid, src, alt, frame, radius=0.06, name=None):
    """A rounded picture: a rounded rectangle with a picture fill (frame should be about 16:9)."""
    o = shape(oid, frame, SAND, name=name or "Picture", adjust=[radius])
    o["kind"]["fill_style"] = {"type": "picture", "src": src, "mode": "stretch"}
    o["description"] = alt
    return o

# ---- motion
def ent(o, effect, ms=600, start=None, easing="ease_out", seq=None):
    o["entrance"] = {"effect": effect, "duration_ms": ms, "easing": easing}
    if seq:
        o["entrance"]["sequence"] = seq
    if start is not None:
        o.setdefault("timing", {})["start_ms"] = start
    return o

def leave(o, effect, ms=500, end=None, easing="ease_in"):
    o["exit"] = {"effect": effect, "duration_ms": ms, "easing": easing}
    if end is not None:
        o.setdefault("timing", {})["end_ms"] = end
    return o

def emph(o, effect, start=0, ms=None, repeat=1, easing=None):
    e = {"effect": {"type": effect}, "start_ms": start, "duration_ms": ms or 1000}
    if repeat > 1:
        e["repeat"] = repeat
    if easing:
        e["easing"] = easing
    o.setdefault("emphasis", []).append(e)
    return o

FADE = {"type": "fade"}
ZOOM = {"type": "zoom"}
POP = {"type": "pop"}
RISE = {"type": "rise"}
EXPAND = {"type": "expand"}
DISSOLVE = {"type": "dissolve"}
GROW_TURN = {"type": "grow_turn"}
SPIN_GROW = {"type": "spin_grow"}
BOUNCE = {"type": "bounce"}
def fly(d="left"): return {"type": "fly_in", "from": d}
def wipe(d="left"): return {"type": "wipe", "from": d}
def floaty(d="bottom"): return {"type": "float", "from": d}
def peek(d="bottom"): return {"type": "peek", "from": d}
def reveal_shape(s="circle", io="out"): return {"type": "shape", "shape": s, "inout": io}
def flip(o="vertical"): return {"type": "flip", "orientation": o}
def wheel(n=1): return {"type": "wheel", "spokes": n}
def split(o="vertical", io="out"): return {"type": "split", "orientation": o, "inout": io}
def strips(d="bottom_right"): return {"type": "strips", "from": d}

# ---- transitions
def T(kind, ms=700, **opts):
    t = {"type": kind, "duration_ms": ms}
    t.update(opts)
    return t
T_FADE = T("fade", 500)

# ---- triggers
def trig(sid, when, action, cond=None, lid=None):
    t = {"action": action, "when": when}
    if cond:
        t["condition"] = cond
    (layer(sid, lid) if lid else slide(sid)).setdefault("triggers", []).append(t)
    return t

def click(o): return {"event": "click", "object": o}
def jump(target): return {"action": "jump_to_slide", "target": target if target in ("next", "previous") else {"id": target}}
def show(l): return {"action": "show_layer", "layer": l}
def hide(l=None): return {"action": "hide_layer", **({"layer": l} if l else {})}
def state(o, s): return {"action": "change_state", "object": o, "state": s}
def setv(v, value, op="assign"): return {"action": "set_variable", "variable": v, "op": op, "value": value}
def cmp(v, op, value): return {"compare": {"variable": v, "op": op, "value": value}}
def ostate(o, s, is_=True): return {"object_state": {"object": o, "state": s, "is": is_}}

def var(name, value):
    C.setdefault("variables", [])
    C["variables"] = [v for v in C["variables"] if v["name"] != name] + [{"name": name, "value": value}]

def new_layer(sid, lid, name, **opts):
    l = {"id": lid, "name": name, "duration_ms": opts.pop("duration_ms", 5000), "objects": [],
         "hide_other_layers": opts.pop("hide_other_layers", True),
         "pause_base_timeline": opts.pop("pause_base_timeline", False),
         "prevent_base_clicks": opts.pop("prevent_base_clicks", False)}
    l.update(opts)
    slide(sid).setdefault("layers", []).append(l)
    return l

def set_label(sid, oid, s, **st):
    o = find(sid, oid)
    k = o["kind"]
    if k["type"] == "text":
        k["text"] = s; k.pop("runs", None); k["style"].update(st)
    elif k["type"] == "shape":
        k.setdefault("text", {"text": "", "style": style(18)})
        k["text"]["text"] = s; k["text"].pop("runs", None); k["text"]["style"].update(st)
    elif k["type"] == "button":
        k["label"] = s
    return o

def setup(sid, bg=CREAM, transition=None, notes=None, master=True, duration=None):
    s = slide(sid)
    s["background"] = bg
    if transition:
        s["transition"] = transition
    if notes:
        s["notes"] = notes
    if master:
        s["master"] = "m_content"
    if duration:
        s["duration_ms"] = duration
    return s

def next_button(sid, x=1040, y=620, label="Continue  →", oid=None, lid=None, fill=CORAL, w=180, start=None):
    oid = oid or f"{sid}_next"
    b = add(sid, button(oid, label, F(x, y, w, 54), fill), lid)
    trig(sid, click(oid), jump("next"), lid=lid)
    if start is not None:
        ent(b, POP, 450, start, "overshoot")
    return b

# ---- narration
def vtt(key):
    lines = ["WEBVTT", ""]
    f = lambda ms: "00:%02d:%02d.%03d" % (ms // 60000, ms // 1000 % 60, ms % 1000)
    for a, b, t in NARR[key]["cues"]:
        lines += [f"{f(a)} --> {f(b)}", t, ""]
    with open(os.path.join(ROOT, "media", f"{key}.vtt"), "w") as fh:
        fh.write("\n".join(lines))

def cue(key, i):
    """When sentence `i` of clip `key` starts, in ms from the clip's start."""
    cues = NARR[key]["cues"]
    if i < len(cues):
        return cues[i][0]
    return cues[-1][0] + 1600 * (i - len(cues) + 1)   # past the last sentence: keep a steady beat

def voice(sid, key, lid=None, start=0, x=1112, y=28, dark=False, pause=True, oid=None):
    """Autoplaying narration with captions, and (WCAG 1.4.2) a small Pause voice control."""
    vtt(key)
    oid = oid or f"{sid}_voice" + (f"_{lid}" if lid else "")
    o = add(sid, obj(oid, "Narration", {"type": "audio", "src": f"{key}.mp3", "autoplay": True, "captions": f"{key}.vtt", "volume": 0.95},
                     F(x, y, 32, 32)), lid)
    if start:
        o.setdefault("timing", {})["start_ms"] = start
    if pause:
        b = add(sid, button(f"{oid}_pause", "Pause voice", F(x, y, 136, 34), "#FFFFFF2E" if dark else DEEP, name="Pause narration button"), lid)
        b["states"] = [{"name": "hover", "fill": "#FFFFFF4D" if dark else FOREST}]
        b.pop("shadow", None)
        trig(sid, click(f"{oid}_pause"), {"action": "pause_media", "object": oid}, lid=lid)
    return o

def confetti(sid, lid, area, start=200, n=14, seed=7):
    """Little shapes that drop in, bounce and twirl: built from shapes, entrances and emphasis."""
    import random
    rnd = random.Random(seed)
    x0, y0, w, h = area
    colors = [BRIGHT, GOLD, SAGE, TEAL, "#F2C9B8"]
    for i in range(n):
        size = rnd.choice([10, 12, 14, 16])
        kind = rnd.choice(["ellipse", "rectangle", "rounded_rectangle", "triangle"])
        o = add(sid, shape(f"{sid}_cf{i}", F(x0 + rnd.random() * w, y0 + rnd.random() * h, size, size * (0.6 if kind == "rectangle" else 1)),
                           colors[i % len(colors)], kind, name="Confetti"), lid)
        o["decorative"] = True
        o["rotation"] = rnd.randint(0, 359)
        ent(o, rnd.choice([BOUNCE, fly("top"), POP]), rnd.randint(700, 1100), start + rnd.randint(0, 700),
            rnd.choice(["bounce", "overshoot", "ease_out"]))
        emph(o, rnd.choice(["spin", "teeter", "jump"]), start + 1400 + rnd.randint(0, 600), rnd.randint(1200, 2200), 2)

# ------------------------------------------------------------------ course-wide settings
C["text_styles"] = [
    {"id": "ts_heading", "name": "Heading", "style": style(46, INK, False, font=SERIF), "paragraphs": {"line_spacing": 1.02}},
    {"id": "ts_eyebrow", "name": "Eyebrow", "style": style(16, CORAL, True, letter_spacing=2.5)},
    {"id": "ts_body", "name": "Body", "style": style(20, BODY), "paragraphs": {"line_spacing": 1.45}},
]
C["glossary"] = [
    {"id": "g_ergonomics", "term": "Ergonomics", "definition": "Fitting the workspace to the person: chair, desk, screen and tools set up so your body stays comfortable."},
    {"id": "g_microbreak", "term": "Microbreak", "definition": "A pause of one to five minutes to stand, stretch, look away or breathe."},
    {"id": "g_202020", "term": "20-20-20 rule", "definition": "Every 20 minutes, look at something 20 feet (about 6 metres) away for 20 seconds."},
    {"id": "g_box", "term": "Paced breathing", "definition": "Breathing in, holding and breathing out to a steady count, such as 4-4-6, to calm the body."},
    {"id": "g_walking", "term": "Walking meeting", "definition": "A one-to-one or small catch-up held on foot, ideally outside, instead of at a screen."},
]
C["resources"] = [
    {"id": "res_who", "title": "WHO: Physical activity fact sheet", "target": {"kind": "url", "url": "https://www.who.int/news-room/fact-sheets/detail/physical-activity"}},
    {"id": "res_coursecraft", "title": "CourseCraft on GitHub", "target": {"kind": "url", "url": "https://github.com/ad1-kamath/coursecraft"}},
]
var("learner_name", "friend")
for v, val in [("habits_seen", 0.0), ("seen_move", False), ("seen_breathe", False), ("seen_connect", False),
               ("tips_opened", 0.0), ("tip_1", False), ("tip_2", False), ("tip_3", False), ("tip_4", False),
               ("sorted", 0.0), ("stress", 4.0), ("energy", 6.0), ("first_habit", "move"),
               ("reminders", True), ("change_minutes", 60.0), ("breaths", 0.0), ("plan_minutes", 10.0)]:
    var(v, val)

# Master: a quiet brand mark and a soft corner shape on every content slide.
C["masters"] = [{
    "id": "m_content", "title": "Content", "background": CREAM, "duration_ms": 5000,
    "objects": [
        obj("m_blob", "Corner shape", {"type": "shape", "shape": "ellipse", "fill": "#E6EFEA99"}, F(1080, -140, 360, 300)),
        obj("m_dot", "Brand dot", {"type": "shape", "shape": "ellipse", "fill": BRIGHT}, F(88, 684, 8, 8)),
        obj("m_brand", "Brand mark", {"type": "text", "text": "Healthy Habits at Work", "style": style(12, MUTED, True, letter_spacing=1)}, F(104, 677, 300, 20)),
    ]}]
for o in C["masters"][0]["objects"]:
    o["decorative"] = True

# ------------------------------------------------------------------ 1. Welcome
s = "welcome"
setup(s, CREAM, T("zoom", 900), "Welcome to Healthy Habits at Work. In about fifteen minutes, you'll pick up small habits that make every workday calmer.",
      master=False, duration=NARR["welcome"]["duration_ms"] + 1500)
hero = add(s, image("w_hero", "hero.jpg", "A woman stretching happily at a standing desk in morning sun", F(90, 0, 1320, 737),
                    name="Hero picture"))
ent(hero, FADE, 1400, 0, "ease_in_out")
hero["motion_paths"] = [{"id": "p_drift", "points": [{"x": 0, "y": 0}, {"x": -40, "y": -17}], "duration_ms": 16000,
                         "easing": "ease_in_out", "relative": True}]
trig(s, {"event": "timeline_starts"}, {"action": "move_on_path", "object": "w_hero", "path": "p_drift"})
scrim = add(s, shape("w_scrim", F(0, 0, 760, 720), CREAM, "rectangle", name="Soft fade"))
scrim["kind"]["fill_style"] = {"type": "linear", "angle": 0, "stops": [
    {"color": "#FBF6EEF5", "position": 0}, {"color": "#FBF6EEE6", "position": 0.6}, {"color": "#FBF6EE00", "position": 1}]}
scrim["decorative"] = True
eyebrow(s, "w_eyebrow", "WELLBEING SERIES  ·  A COURSECRAFT SAMPLE", 88, 168, start=500)
title = add(s, text("w_title", "Healthy Habits\nat Work", F(84, 196, 640, 200), 80, INK, False, "Title", ls=0.9, font=SERIF))
ent(title, RISE, 1300, 800, "overshoot", "by_letter")
add(s, ent(body("w_sub", "Small changes to how you move, breathe and connect add up to calmer, more focused days.", 88, 410, 470, 70, 21, "#4A5B58", "Subtitle"),
           FADE, 700, cue("welcome", 1) + 300))
st = add(s, button("w_start", "Start the course  →", F(88, 512, 252, 60), CORAL))
ent(st, POP, 500, cue("welcome", 2), "overshoot")
emph(st, "heartbeat", cue("welcome", 2) + 2200, 1000, 2)
chip = add(s, pill("w_chip", "15 min  ·  3 lessons  ·  quiz", F(356, 521, 236, 42), "#FFFFFFCC", DEEP, 15, "Duration chip"))
ent(chip, FADE, 500, cue("welcome", 2) + 300)
add(s, ent(text("w_hint", "Press Enter to start", F(90, 588, 260, 22), 15, MUTED, False, "Keyboard hint"), FADE, 500, cue("welcome", 2) + 900))
voice(s, "welcome", x=88, y=640, pause=False, oid="w_voice")
add(s, button("w_listen", "Listen again", F(88, 640, 136, 36), DEEP, name="Listen again button")).pop("shadow")
add(s, button("w_pause", "Pause", F(232, 640, 90, 36), DEEP, name="Pause narration button")).pop("shadow")
trig(s, click("w_start"), jump("next"))
trig(s, {"event": "key_pressed", "key": "enter"}, jump("next"))
trig(s, click("w_listen"), {"action": "play_media", "object": "w_voice"})
trig(s, click("w_pause"), {"action": "pause_media", "object": "w_voice"})
for n, oid in enumerate(["w_start", "w_listen", "w_pause"], 1):
    find(s, oid)["focus_order"] = n

# ------------------------------------------------------------------ 2. Name
s = "name"
setup(s, CREAM, T("push", 650, **{"from": "right"}), "Type your name so we can greet you and put it on your certificate.",
      duration=NARR["name"]["duration_ms"] + 1000)
blob = add(s, shape("n_blob", F(640, 40, 600, 600), SAND, "ellipse", name="Backdrop circle"))
blob["decorative"] = True
ent(blob, GROW_TURN, 900, 0, "overshoot")
pic = add(s, image("n_pic", "wave.jpg", "A friendly colleague waving hello from his desk", F(660, 60, 560, 560),
                   crop={"x": 0.44, "y": 0.0, "w": 0.558, "h": 1.0}, name="Wave picture", mask="ellipse"))
ent(pic, reveal_shape("circle"), 1000, 250)
ring = add(s, shape("n_ring", F(632, 32, 616, 616), "#FFFFFF00", "ellipse", name="Dotted ring", outline={"color": SAGE, "width": 2, "dash": "dot"}))
ring["decorative"] = True
ent(ring, wheel(1), 1200, 600)
emph(ring, "spin", 2000, 24000, 1, "linear")
eyebrow(s, "n_eyebrow", "BEFORE WE START", 88, 170, start=200)
add(s, ent(heading("n_title", "First things first.\nWhat should we call you?", 84, 200, 560, 130, 48), RISE, 900, cue("name", 1), "overshoot", "by_paragraph"))
add(s, ent(body("n_body", "We'll greet you by name, and it goes on your certificate at the end.", 88, 348, 480, 64), FADE, 600, cue("name", 2)))
entry = add(s, obj("n_entry", "Name entry", {"type": "text_entry", "variable": "learner_name", "placeholder": "Your first name", "max_chars": 40},
                   F(88, 430, 440, 58), description="Your first name"))
ent(entry, wipe("left"), 600, 900)
greet = add(s, text("n_greet", "Nice to meet you, %learner_name%!", F(88, 506, 560, 40), 26, TEAL, True, "Greeting"))
greet["initial_state"] = "hidden"
ent(greet, POP, 450, None, "overshoot")
next_button(s, 88, 572, "Continue  →", w=190, start=1400)
trig(s, {"event": "variable_changes", "variable": "learner_name"}, state("n_greet", "normal"), cmp("learner_name", "ne", ""))
trig(s, {"event": "variable_changes", "variable": "learner_name"}, state("n_greet", "hidden"), cmp("learner_name", "eq", ""))
voice(s, "name")

# ------------------------------------------------------------------ 3. Habits: course map with cards, layers and a ring
s = "habits"
setup(s, CREAM, T("reveal", 700, **{"from": "right"}), "Open each card to see what the habit is about. The Next button appears when you've opened all three.",
      duration=NARR["habits"]["duration_ms"] + 800)
eyebrow(s, "h_eyebrow", "YOUR PLAN", 88, 48, start=0)
add(s, ent(heading("h_title", "Three small habits, one better day", 84, 76, 820, 64, 44), RISE, 800, 200, "overshoot"))
add(s, ent(body("h_hint", "Open each card. When you've explored all three, the ring fills up.", 88, 138, 700, 30, 18, MUTED, "Hint"), FADE, 500, cue("habits", 2)))
meter = add(s, obj("h_meter", "Habits explored", {"type": "progress_meter", "variable": "habits_seen", "min": 0, "max": 3, "style": "ring", "show_value": True},
                   F(1124, 80, 72, 72), description="Habits explored"))
ent(meter, wheel(1), 800, cue("habits", 3))
add(s, ent(text("h_meter_label", "explored", F(1100, 154, 120, 22), 14, MUTED, True, "Meter label", align="center"), FADE, 400, cue("habits", 3) + 300))
cards = [("move", "Move", "Change position every 30 minutes, and let your desk fit you.", "move.jpg", "A woman taking the stairs at work", 0.62,
          ["Sitting still is the problem, not sitting.", "Small, frequent movement beats one long workout."]),
         ("breathe", "Breathe", "A one-minute breathing pattern resets you between tasks.", "breathe.jpg", "A woman breathing calmly with her eyes closed", 0.62,
          ["Slow breathing tells your body it is safe.", "Four counts in, four held, six out. About a minute."]),
         ("connect", "Connect", "Swap one meeting a week for a walk and talk outside.", "connect.jpg", "Two colleagues on a walking meeting", 0.65,
          ["A walk and a real conversation lift your mood.", "Most catch-ups don't need slides."])]
starts = [cue("habits", 1) + 1500, cue("habits", 1) + 2300, cue("habits", 1) + 3100]
for i, (key, head, blurb, src, alt, focus, lines) in enumerate(cards):
    x = 88 + i * 376
    g = f"card_{key}"
    card = add(s, shape(f"h_card_{key}", F(x, 190, 352, 452), WHITE, name=f"{head} card", adjust=[0.07]))
    card["shadow"] = CARD
    card["states"] = [{"name": "hover", "fill": "#F6FAF7"}]
    card["description"] = f"Open the {head} habit"
    pfr = F(x + 14, 204, 324, 214)
    add(s, image(f"h_pic_{key}", src, alt, pfr, crop=crop_for(pfr, focus), name=f"{head} picture"))
    num = add(s, shape(f"h_num_{key}", F(x + 26, 396, 46, 46), [TEAL, CORAL, DEEP][i], "ellipse", name=f"{head} number",
                       label=f"0{i + 1}", lstyle=style(16, WHITE, True, align="center", valign="middle")))
    num["states"] = [{"name": "done", "fill": FOREST, "text": "✓"}]
    num["shadow"] = TEAL_LIFT
    add(s, text(f"h_head_{key}", head, F(x + 26, 456, 300, 44), 30, INK, False, f"{head} heading", font=SERIF))
    add(s, body(f"h_body_{key}", blurb, x + 26, 502, 300, 80, 17, "#4A5B58", f"{head} text"))
    add(s, text(f"h_open_{key}", "Open  →", F(x + 26, 598, 200, 24), 15, CORAL, True, f"{head} open hint"))
    for oid in (f"h_card_{key}", f"h_pic_{key}", f"h_num_{key}", f"h_head_{key}", f"h_body_{key}", f"h_open_{key}"):
        find(s, oid)["group"] = g
        ent(find(s, oid), RISE, 750, starts[i], "overshoot")
    # each habit's details layer: dim, panel, picture, narration, and lines timed to the words
    lid = f"h_layer_{key}"
    vkey = f"h_{key}"
    new_layer(s, lid, f"{head} details", prevent_base_clicks=True, pause_base_timeline=True,
              duration_ms=NARR[vkey]["duration_ms"] + 600)
    dim = add(s, ent(shape(f"h_dim_{key}", F(0, 0, 1280, 720), "#163B37B3", "rectangle", name="Dim"), FADE, 300), lid)
    dim["decorative"] = True
    panel = add(s, ent(shape(f"h_panel_{key}", F(160, 96, 960, 528), WHITE, name="Panel", adjust=[0.04]), ZOOM, 450, 0, "overshoot"), lid)
    panel["shadow"] = SOFT
    bfr = F(184, 120, 420, 480)
    add(s, ent(image(f"h_big_{key}", src, alt, bfr, crop=crop_for(bfr, focus), name="Picture"), peek("left"), 600, 150), lid)
    eyebrow(s, f"h_l_eyebrow_{key}", f"HABIT {i + 1} OF 3", 648, 150, lid=lid, start=200)
    add(s, ent(heading(f"h_l_title_{key}", head, 646, 180, 420, 70, 52), RISE, 700, 300, "overshoot", "by_letter"), lid)
    for j, line in enumerate(lines):
        ln = add(s, body(f"h_l_line{j + 1}_{key}", line, 648 + 34, 286 + j * 100, 400, 84, 21, BODY, f"Point {j + 1}"), lid)
        ent(ln, floaty("bottom"), 500, cue(vkey, j + 1))
        dot = add(s, shape(f"h_l_dot{j + 1}_{key}", F(648, 294 + j * 100, 18, 18), [TEAL, CORAL, DEEP][i], "ellipse", name="Bullet"), lid)
        dot["decorative"] = True
        ent(dot, POP, 400, cue(vkey, j + 1), "overshoot")
    close = add(s, button(f"h_close_{key}", "Got it", F(648, 530, 150, 52), DEEP), lid)
    ent(close, POP, 400, cue(vkey, len(lines)) + 400, "overshoot")
    voice(s, vkey, lid=lid, x=962, y=120)
    trig(s, click(f"h_close_{key}"), hide(), lid=lid)
    trig(s, {"event": "key_pressed", "key": "escape"}, hide(), lid=lid)
    flag = f"seen_{key}"
    for part in (f"h_card_{key}", f"h_pic_{key}"):
        trig(s, click(part), setv("habits_seen", 1.0, "add"), cmp(flag, "eq", False))
        trig(s, click(part), setv(flag, True))
        trig(s, click(part), show(lid))
        trig(s, click(part), state(f"h_num_{key}", "done"))
hn = add(s, button("habits_next", "Let's begin  →", F(880, 100, 200, 48), CORAL))
hn["initial_state"] = "hidden"
ent(hn, POP, 450, None, "overshoot")
emph(hn, "heartbeat", 600, 1000, 2)
trig(s, click("habits_next"), jump("next"))
trig(s, {"event": "variable_changes", "variable": "habits_seen"}, state("habits_next", "normal"), cmp("habits_seen", "ge", 3))
for k in ("move", "breathe", "connect"):
    trig(s, {"event": "timeline_starts"}, setv(f"seen_{k}", False))
trig(s, {"event": "timeline_starts"}, setv("habits_seen", 0.0))
voice(s, "habits", x=1100, y=28)

# ------------------------------------------------------------------ chapter openers
def opener(sid, num, word, line, src, alt, focus, vkey, transition):
    setup(sid, FOREST, transition, line, master=False, duration=NARR[vkey]["duration_ms"] + 1800)
    big = add(sid, text(f"{sid}_num", num, F(70, 20, 440, 260), 230, "#A8C3B140", False, "Lesson number", font=SERIF))
    big["decorative"] = True
    ent(big, fly("bottom"), 1100, 0, "elastic")
    ring = add(sid, shape(f"{sid}_ring", F(640, 60, 600, 600), "#FFFFFF00", "ellipse", name="Dotted ring",
                          outline={"color": "#A8C3B188", "width": 2, "dash": "dot"}))
    ring["decorative"] = True
    ent(ring, wheel(1), 1400, 200)
    emph(ring, "spin", 1600, 30000, 1, "linear")
    halo = add(sid, shape(f"{sid}_halo", F(680, 100, 520, 520), DEEP, "ellipse", name="Halo"))
    halo["decorative"] = True
    halo["glow"] = {"size": 30, "color": "#2F6F6A55"}
    ent(halo, ZOOM, 900, 300, "overshoot")
    p = add(sid, image(f"{sid}_pic", src, alt, F(700, 120, 480, 480), crop={"x": focus - 0.28, "y": 0.0, "w": 0.558, "h": 1.0},
                       name="Lesson picture", mask="ellipse"))
    ent(p, reveal_shape("circle"), 1100, 600, "ease_in_out")
    eyebrow(sid, f"{sid}_eyebrow", f"LESSON {num.lstrip('0')} OF 3", 92, 300, SAGE, start=cue(vkey, 0))
    t = add(sid, text(f"{sid}_title", word, F(88, 330, 560, 130), 112, ON_DARK, False, "Title", font=SERIF))
    ent(t, POP, 1000, cue(vkey, 1), "overshoot", "by_letter")
    add(sid, ent(text(f"{sid}_line", line, F(92, 470, 470, 80), 22, ON_DARK_SOFT, False, "Lesson line", ls=1.4), FADE, 700, cue(vkey, 2)))
    leaf = add(sid, shape(f"{sid}_leaf", F(1150, 560, 18, 18), BRIGHT, "ellipse", name="Floating dot"))
    leaf["decorative"] = True
    ent(leaf, POP, 400, 1400, "overshoot")
    leaf["motion_paths"] = [{"id": "p_orbit", "points": [{"x": 0, "y": 0}, {"x": -120, "y": 40}, {"x": -330, "y": 20}, {"x": -470, "y": -60}],
                             "duration_ms": 9000, "easing": "ease_in_out", "relative": True}]
    trig(sid, {"event": "timeline_reaches", "ms": 1500}, {"action": "move_on_path", "object": f"{sid}_leaf", "path": "p_orbit"})
    b = next_button(sid, 92, 580, "Let's go  →", w=180, start=cue(vkey, 2) + 900)
    emph(b, "pulse", cue(vkey, 2) + 2600, 700, 2)
    voice(sid, vkey, dark=True)

opener("move_intro", "01", "Move", "Let's start where you spend most of your day: your desk.", "move.jpg",
       "A woman taking the stairs in a bright office", 0.7, "move_intro", T("shape", 1000, shape="circle", inout="out"))

# ------------------------------------------------------------------ 4. Desk markers
s = "desk"
setup(s, MIST, T("wipe", 700, **{"from": "right"}), "Tap each marker on the picture to see why it matters.", master=False,
      duration=NARR["desk"]["duration_ms"] + 4000)
dfr = F(0, 0, 760, 720)
dp = add(s, image("d_desk", "desk.jpg", "Side view of a woman sitting well at her desk", dfr, crop={"x": 0.17, "y": 0.0, "w": 0.59, "h": 1.0}, name="Desk picture"))
ent(dp, peek("left"), 800)
MARKS = [(560, 214, "Screen at eye level", "The top of the monitor sits at or just below your eyes, about an arm's length away."),
         (190, 474, "Supported back", "Sit back in the chair so it holds the curve of your lower back."),
         (420, 640, "Feet flat", "Both feet rest on the floor or a footrest, knees about level with your hips."),
         (708, 372, "Water in reach", "A bottle you can see is a bottle you'll drink. Refilling it is a free movement break.")]
for i, (mx, my, title, bd) in enumerate(MARKS, 1):
    m = add(s, obj(f"d_m{i}", f"Marker {i}", {"type": "marker", "title": title, "body": bd, "icon": "plus", "color": BRIGHT}, F(mx, my, 42, 42)))
    ent(m, POP, 450, cue("desk", 1) + 300 + i * 260, "overshoot")
    emph(m, "pulse", cue("desk", 1) + 2400 + i * 200, 800, 2)
    trig(s, {"event": "marker_opened", "object": f"d_m{i}"}, setv("tips_opened", 1.0, "add"), cmp(f"tip_{i}", "eq", False))
    trig(s, {"event": "marker_opened", "object": f"d_m{i}"}, setv(f"tip_{i}", True))
eyebrow(s, "d_eyebrow", "LESSON 1  ·  MOVE", 820, 120, TEAL, start=300)
add(s, ent(heading("d_title", "Make the desk fit you,\nnot the other way round", 816, 148, 420, 130, 38), RISE, 800, 500, "overshoot", "by_paragraph"))
add(s, ent(body("d_body", "Four quick checks keep your neck, back and legs happy all day.", 820, 290, 400, 64, 19), FADE, 600, cue("desk", 0) + 1200))
cnt = add(s, pill("d_count", "%tips_opened% of 4 checked", F(820, 384, 220, 42), WHITE, DEEP, 15, "Tips counter"))
ent(cnt, FADE, 500, cue("desk", 1))
done = add(s, text("d_done", "All four checked. Nice set-up!", F(820, 444, 400, 30), 19, TEAL, True, "All checked"))
done["initial_state"] = "hidden"
ent(done, POP, 400, None, "overshoot")
trig(s, {"event": "variable_changes", "variable": "tips_opened"}, state("d_done", "normal"), cmp("tips_opened", "ge", 4))
hint = add(s, pill("d_hint", "Tap a  +  to begin", F(60, 40, 220, 44), INK, WHITE, 16, "Start hint"))
ent(hint, fly("top"), 450, cue("desk", 1) + 1600, "overshoot")
hint["timing"]["end_ms"] = cue("desk", 1) + 7500
leave(hint, FADE, 500)
next_button(s, 820, 600, "Next  →", w=170, start=cue("desk", 1) + 1000)
for i in (1, 2, 3, 4):
    trig(s, {"event": "timeline_starts"}, setv(f"tip_{i}", False))
trig(s, {"event": "timeline_starts"}, setv("tips_opened", 0.0))
voice(s, "desk", x=1104, y=40)

# ------------------------------------------------------------------ 5. Video with cues and captions
s = "video"
setup(s, CREAM, T("cover", 650, **{"from": "bottom"}), "Follow along with three gentle stretches you can do at your desk.", duration=15000)
slide(s)["revisit"] = "reset"
eyebrow(s, "v_eyebrow", "LESSON 1  ·  MOVE", 88, 48, start=0)
add(s, ent(heading("v_title", "Three stretches you can do at your desk", 84, 76, 1000, 64, 42), RISE, 700, 150, "overshoot"))
vid = add(s, obj("v_video", "Stretch video", {"type": "video", "src": "stretch.mp4", "autoplay": False, "captions": "stretch.vtt", "cues": [
    {"id": "c_neck", "at_ms": 300, "name": "Neck stretch"}, {"id": "c_shoulder", "at_ms": 4700, "name": "Shoulder stretch"},
    {"id": "c_twist", "at_ms": 9100, "name": "Seated twist"}]}, F(88, 160, 712, 400)))
vid["shadow"] = SOFT
ent(vid, EXPAND, 700, 0)
add(s, ent(button("v_play", "Play", F(88, 584, 120, 48), CORAL), POP, 400, 700, "overshoot"))
add(s, ent(button("v_pause", "Pause", F(218, 584, 120, 48), DEEP), POP, 400, 800, "overshoot"))
add(s, ent(text("v_cc", "Captions: use the CC button in the player bar.", F(356, 598, 440, 24), 15, MUTED, False, "Captions hint"), FADE, 400, 900))
trig(s, click("v_play"), {"action": "play_media", "object": "v_video"})
trig(s, click("v_pause"), {"action": "pause_media", "object": "v_video"})
steps = [("neck", "Neck stretch", "Tilt your head towards one shoulder, then the other."),
         ("shoulder", "Shoulder stretch", "Pull one arm across your chest. Switch sides."),
         ("twist", "Seated twist", "Sit tall, hold the backrest and turn gently.")]
for i, (key, title, bd) in enumerate(steps):
    y = 160 + i * 134
    row = add(s, shape(f"v_row_{key}", F(832, y, 368, 118), WHITE, name=f"{title} row", adjust=[0.1]))
    row["states"] = [{"name": "active", "fill": BLUSH}, {"name": "done", "fill": MIST}]
    row["shadow"] = CARD
    n = add(s, shape(f"v_num_{key}", F(852, y + 22, 38, 38), MUTED, "ellipse", name=f"{title} number", label=str(i + 1), lstyle=style(16, WHITE, True, align="center", valign="middle")))
    n["states"] = [{"name": "active", "fill": CORAL}, {"name": "done", "fill": TEAL, "text": "✓"}]
    add(s, text(f"v_head_{key}", title, F(906, y + 20, 276, 30), 20, INK, True, f"{title} heading"))
    add(s, text(f"v_text_{key}", bd, F(906, y + 52, 276, 56), 16, BODY, False, f"{title} text", ls=1.3))
    for oid in (f"v_row_{key}", f"v_num_{key}", f"v_head_{key}", f"v_text_{key}"):
        ent(find(s, oid), fly("right"), 550, 400 + i * 160, "overshoot")
    trig(s, {"event": "cue_reached", "object": "v_video", "cue": f"c_{key}"}, state(f"v_row_{key}", "active"))
    trig(s, {"event": "cue_reached", "object": "v_video", "cue": f"c_{key}"}, state(f"v_num_{key}", "active"))
    if i > 0:
        prev = steps[i - 1][0]
        trig(s, {"event": "cue_reached", "object": "v_video", "cue": f"c_{key}"}, state(f"v_row_{prev}", "done"))
        trig(s, {"event": "cue_reached", "object": "v_video", "cue": f"c_{key}"}, state(f"v_num_{prev}", "done"))
trig(s, {"event": "media_ends", "object": "v_video"}, state("v_row_twist", "done"))
trig(s, {"event": "media_ends", "object": "v_video"}, state("v_num_twist", "done"))
new_layer(s, "v_done", "All done", duration_ms=15000)
toast = add(s, shape("v_toast", F(832, 578, 188, 60), DEEP, name="Done toast", label="Nice work!", lstyle=style(18, CREAM, True, align="center", valign="middle"), adjust=[0.5]), "v_done")
ent(toast, fly("bottom"), 450, 0, "overshoot")
emph(toast, "jump", 600, 800)
trig(s, {"event": "media_ends", "object": "v_video"}, show("v_done"))
add(s, button("video_next", "Continue", F(1040, 584, 160, 48), CORAL))
trig(s, click("video_next"), jump("next"))

# ------------------------------------------------------------------ 6. Eyes: timer, lightbox, stats on the words
s = "eyes"
setup(s, CREAM, T("split", 800, orientation="vertical", inout="out"), "Start the 20-second timer and look at something far away until it ends.",
      master=False, duration=45000)   # long enough for the 20-second timer started after the narration
efr = F(0, 0, 560, 720)
ep = add(s, image("e_pic", "eyes.jpg", "A man resting his eyes by looking out of a window at distant hills", efr, crop={"x": 0.12, "y": 0.0, "w": 0.524, "h": 1.0}, name="Eyes picture"))
ent(ep, wipe("bottom"), 900)
eyebrow(s, "e_eyebrow", "LESSON 1  ·  MOVE", 620, 72, start=200)
add(s, ent(heading("e_title", "Give your eyes a\n20-second holiday", 616, 98, 600, 120, 44), RISE, 800, 400, "overshoot", "by_paragraph"))
for j, (small, big, ms) in enumerate([("Every", "20 min", cue("eyes", 2)), ("Look", "20 ft away", cue("eyes", 2) + 1800), ("For", "20 sec", cue("eyes", 2) + 3800)]):
    x = 620 + j * 196
    stat = add(s, shape(f"e_stat{j + 1}", F(x, 240, 180, 92), [MIST, BLUSH, MIST][j], name=f"Rule part {j + 1}", label=f"{small}\n{big}",
                        lstyle=style(22, DEEP, True, align="center", valign="middle"), adjust=[0.2]))
    ent(stat, POP, 500, ms, "overshoot")
ring = add(s, shape("e_ring", F(620, 364, 180, 180), WHITE, "ellipse", name="Timer ring", outline={"color": SAGE, "width": 6}))
ring["glow"] = {"size": 14, "color": "#A8C3B170"}
ent(ring, wheel(1), 900, cue("eyes", 3))
add(s, ent(obj("e_timer", "Eye timer", {"type": "timer", "duration_ms": 20000, "format": "seconds", "autostart": False,
                                        "style": style(58, DEEP, True, align="center", valign="middle")}, F(620, 404, 180, 90), description="Seconds left"), FADE, 500, cue("eyes", 3) + 300))
add(s, ent(text("e_timer_label", "seconds", F(620, 486, 180, 22), 14, MUTED, True, "Timer label", align="center"), FADE, 500, cue("eyes", 3) + 300))
for oid, label, fr, fill, k in [("e_start", "Start", F(830, 380, 120, 48), CORAL, 0), ("e_pause", "Pause", F(960, 380, 120, 48), DEEP, 1),
                                ("e_reset", "Reset", F(830, 438, 120, 48), DEEP, 2), ("e_why", "Why 20-20-20?", F(830, 496, 250, 42), TEAL, 3)]:
    add(s, ent(button(oid, label, fr, fill), POP, 400, cue("eyes", 3) + 400 + k * 120, "overshoot"))
emph(find(s, "e_start"), "heartbeat", cue("eyes", 3) + 1800, 1000, 2)
trig(s, click("e_start"), {"action": "start_timer", "timer": "e_timer"})
trig(s, click("e_start"), state("e_ring", "running"))
trig(s, click("e_pause"), {"action": "pause_timer", "timer": "e_timer"})
trig(s, click("e_reset"), {"action": "reset_timer", "timer": "e_timer"})
trig(s, click("e_reset"), hide("e_done"))
trig(s, click("e_reset"), state("e_ring", "normal"))
trig(s, click("e_why"), {"action": "lightbox_slide", "slide": "bonus"})
ring["states"] = [{"name": "running", "fill": "#F3F8F5"}]
new_layer(s, "e_done", "Time's up", duration_ms=NARR["eyes_done"]["duration_ms"] + 1200, hide_when_timeline_ends=True)
t2 = add(s, shape("e_toast", F(620, 560, 460, 64), TEAL, name="Done toast", label="Welcome back! Your eyes thank you.",
                  lstyle=style(18, WHITE, True, align="center", valign="middle"), adjust=[0.5]), "e_done")
ent(t2, fly("bottom"), 450, 0, "overshoot")
emph(t2, "color_pulse", 500, 1200)
voice(s, "eyes_done", lid="e_done", x=1100, y=28)
trig(s, {"event": "timer_ends", "timer": "e_timer"}, show("e_done"))
trig(s, {"event": "timer_ends", "timer": "e_timer"}, state("e_ring", "normal"))
next_button(s, 1096, 620, "Next  →", w=130, start=cue("eyes", 3) + 900)
voice(s, "eyes", x=1100, y=28)

# ------------------------------------------------------------------ 7. Myth or fact (flip cards insert)
s = "myth"
setup(s, CREAM, T("push", 650, **{"from": "right"}), "Tap a card to flip it and find out.", duration=NARR["myth"]["duration_ms"] + 2000)
eyebrow(s, "m_eyebrow", "CHECK YOUR UNDERSTANDING", 88, 48, start=0)
add(s, ent(heading("m_title", "Myth or fact? Flip a card to find out", 84, 76, 1000, 64, 44), RISE, 700, 150, "overshoot"))
flips = [("“Sitting up straight all day is best.”", "Myth. Any posture held for hours gets tiring. Your best posture is your next one."),
         ("“Short breaks make you more productive.”", "Fact. A few minutes away from the screen helps you refocus and make fewer mistakes."),
         ("“You need a gym to stay active at work.”", "Myth. Stairs, walking calls and a stretch at your desk all count.")]
for i, (front, back) in enumerate(flips, 1):
    o = find(s, f"flip_card{i}")
    o["frame"] = F(88 + (i - 1) * 376, 180, 352, 360)
    edge, tint = [(TEAL, MIST), (CORAL, BLUSH), (DEEP, "#E3EEF0")][i - 1]
    o["kind"]["fill"] = WHITE
    o["kind"]["outline"] = {"color": edge, "width": 2, "dash": "solid"}
    o["kind"]["text"]["text"] = front
    o["kind"]["text"]["style"].update(style(26, INK, False, align="center", valign="middle", font=SERIF))
    o["kind"]["text"]["autofit"] = "shrink"
    o["shadow"] = CARD
    for stt in o.get("states", []):
        if stt["name"] != "hover":
            stt["text"] = back
            stt["fill"] = tint
    ent(o, flip("vertical"), 800, cue("myth", 1) + (i - 1) * 220, "overshoot")
    emph(o, "teeter", cue("myth", 1) + 1600 + i * 250, 900)
add(s, ent(text("m_hint", "Tap a card again to flip it back.", F(88, 566, 1104, 26), 15, MUTED, False, "Hint", align="center"), FADE, 500, cue("myth", 1) + 1200))
next_button(s, 1020, 620, start=cue("myth", 1) + 1400)
voice(s, "myth", x=1100, y=28)

# ------------------------------------------------------------------ 8. Sort: draggable objects, intersections, dropped_on
s = "sort"
setup(s, SAND, T("push", 650, **{"from": "right"}), "Drag each card onto Helps or Hurts.", duration=NARR["sort"]["duration_ms"] + 1500)
eyebrow(s, "so_eyebrow", "TRY IT", 88, 48, CORAL_DK, start=0)
add(s, ent(heading("so_title", "Helps or hurts? Drag each card to a side", 84, 76, 1000, 64, 44), RISE, 700, 150, "overshoot"))
bins = [("so_helps", "Helps  ✓", 640, MIST, TEAL), ("so_hurts", "Hurts  ×", 924, BLUSH, CORAL_DK)]
for j, (bid, label, x, fill, edge) in enumerate(bins):
    b = add(s, shape(bid, F(x, 170, 268, 430), fill, name=f"{label.split()[0]} area", adjust=[0.07], outline={"color": edge, "width": 2, "dash": "dash"},
                     label=label, lstyle=style(26, edge, False, align="center", valign="top", font=SERIF, insets={"left": 10, "right": 10, "top": 24, "bottom": 10})))
    b["states"] = [{"name": "active", "fill": WHITE}]
    ent(b, wipe("top"), 700, cue("sort", 1) + j * 200)
items = [("so_water", "Refill your water bottle", "so_helps"), ("so_lunch", "Eat lunch at your keyboard", "so_hurts"),
         ("so_stairs", "Take the stairs", "so_helps"), ("so_scroll", "Scroll in bed at midnight", "so_hurts")]
for i, (iid, label, right) in enumerate(items):
    it = add(s, shape(iid, F(88 + (i % 2) * 248, 196 + (i // 2) * 112, 228, 88), WHITE, name=label, adjust=[0.22],
                      label=label, lstyle=style(18, INK, True, align="center", valign="middle")))
    it["shadow"] = CARD
    it["states"] = [{"name": "right", "fill": "#DDEDE3", "text": label + "  ✓"}, {"name": "wrong", "fill": BLUSH, "text": "Try the other side"}]
    it["drag"] = {"targets": ["so_helps", "so_hurts"], "snap": False}
    ent(it, POP, 500, 600 + i * 150, "overshoot")
    wrong = "so_hurts" if right == "so_helps" else "so_helps"
    trig(s, {"event": "dropped_on", "object": iid, "target": right}, setv("sorted", 1.0, "add"), ostate(iid, "right", False))
    trig(s, {"event": "dropped_on", "object": iid, "target": right}, state(iid, "right"))
    trig(s, {"event": "dropped_on", "object": iid, "target": wrong}, state(iid, "wrong"))
    for bid, *_ in bins:
        trig(s, {"event": "objects_intersect", "a": iid, "b": bid}, state(bid, "active"))
        trig(s, {"event": "objects_stop_intersecting", "a": iid, "b": bid}, state(bid, "normal"))
add(s, ent(pill("so_count", "%sorted% of 4 sorted", F(88, 440, 220, 42), WHITE, DEEP, 15, "Sorted counter"), FADE, 400, 1300))
new_layer(s, "so_done", "All sorted", duration_ms=5000)
st2 = add(s, shape("so_toast", F(88, 500, 476, 76), TEAL, name="Done toast", label="All sorted! Small swaps like these add up.",
                   lstyle=style(18, WHITE, True, align="center", valign="middle"), adjust=[0.3]), "so_done")
ent(st2, ZOOM, 450, 0, "overshoot")
emph(st2, "jump", 500, 800)
confetti(s, "so_done", (100, 470, 440, 40), 100, 10, 3)
trig(s, {"event": "variable_changes", "variable": "sorted"}, show("so_done"), cmp("sorted", "ge", 4))
trig(s, {"event": "timeline_starts"}, setv("sorted", 0.0))
next_button(s, 88, 610, w=180)
voice(s, "sort", x=1100, y=28)

# ------------------------------------------------------------------ Breathe opener
opener("breathe_intro", "02", "Breathe", "When the day speeds up, your breath is the quickest way to slow it down.", "breathe.jpg",
       "A woman breathing calmly with her eyes closed", 0.72, "breathe_intro", T("clock", 1000, spokes=1))

# ------------------------------------------------------------------ 9. Breathe: Lottie, timeline_reaches, motion paths, narration
s = "breathe"
B0 = NARR["breathe"]["duration_ms"] + 500   # the circle starts once the instructions end
setup(s, FOREST, T("fade_black", 900), "Follow the circle. Breathe in as it grows, hold for a moment, and breathe out slowly as it shrinks.",
      master=False, duration=B0 + 14500)
slide(s)["revisit"] = "reset"
eyebrow(s, "b_eyebrow", "LESSON 2  ·  BREATHE", 88, 96, SAGE, start=0)
add(s, ent(text("b_title", "Breathe with\nthe circle", F(84, 124, 520, 150), 58, ON_DARK, False, "Title", ls=0.98, font=SERIF), RISE, 900, 200, "overshoot", "by_paragraph"))
add(s, ent(text("b_body", "Watch the circle grow as you breathe in, rest as you hold, and shrink as you breathe out.", F(88, 290, 440, 90), 19, ON_DARK_SOFT, False, "Body", ls=1.4),
           FADE, 600, 700))
lab = add(s, text("b_label", "Get comfortable…", F(88, 420, 480, 56), 36, WHITE, False, "Breathing cue", font=SERIF))
lab["states"] = [{"name": "in", "text": "Breathe in · 4"}, {"name": "hold", "text": "Hold · 4"}, {"name": "out", "text": "Breathe out · 6"}, {"name": "again", "text": "Lovely. Once more?"}]
emph(lab, "transparency", B0 - 1500, 1400)
circ = add(s, obj("b_circle", "Breathing circle", {"type": "lottie", "src": "breathe-circle.json", "loop": False}, F(640, 80, 560, 560), description="A circle that grows and shrinks with your breath"))
ent(circ, ZOOM, 1200, 0, "ease_in_out")
leaf = add(s, obj("b_leaf", "Floating leaf", {"type": "shape", "shape": "ellipse", "fill": BRIGHT}, F(912, 612, 16, 16), decorative=True))
leaf["glow"] = {"size": 8, "color": "#E8775A66"}
leaf["motion_paths"] = [
    {"id": "p_up", "points": [{"x": 0, "y": 0}, {"x": 0, "y": -560}], "duration_ms": 4000, "easing": "ease_in_out", "relative": True},
    {"id": "p_down", "points": [{"x": 0, "y": 0}, {"x": 0, "y": 560}], "duration_ms": 6000, "easing": "ease_in_out", "relative": True}]
add(s, ent(button("b_again", "Breathe again", F(88, 590, 190, 52), CORAL), POP, 450, None, "overshoot"))
find(s, "b_again")["initial_state"] = "hidden"
add(s, ent(text("b_count", "Breaths taken: %breaths%", F(296, 604, 300, 26), 17, SAGE, True, "Breath counter"), FADE, 500, B0))
trig(s, {"event": "timeline_reaches", "ms": B0}, state("b_label", "in"))
trig(s, {"event": "timeline_reaches", "ms": B0}, {"action": "move_on_path", "object": "b_leaf", "path": "p_up"})
trig(s, {"event": "timeline_reaches", "ms": B0 + 4000}, state("b_label", "hold"))
trig(s, {"event": "timeline_reaches", "ms": B0 + 8000}, state("b_label", "out"))
trig(s, {"event": "timeline_reaches", "ms": B0 + 8000}, {"action": "move_on_path", "object": "b_leaf", "path": "p_down"})
trig(s, {"event": "animation_completes", "object": "b_leaf", "path": "p_down"}, setv("breaths", 1.0, "add"))
trig(s, {"event": "timeline_ends"}, state("b_label", "again"))
trig(s, {"event": "timeline_ends"}, state("b_again", "normal"))
trig(s, click("b_again"), jump("breathe"))
voice(s, "breathe", x=88, y=520, dark=True)
add(s, button("b_hold", "Pause", F(234, 520, 96, 34), "#FFFFFF2E", name="Pause timeline button")).pop("shadow")
add(s, button("b_go", "Resume", F(338, 520, 110, 34), "#FFFFFF2E", name="Resume timeline button")).pop("shadow")
trig(s, click("b_hold"), {"action": "pause_timeline"})
trig(s, click("b_go"), {"action": "resume_timeline"})
add(s, button("breathe_next", "Continue  →", F(1020, 620, 180, 52), CORAL))
trig(s, click("breathe_next"), jump("next"))

# ------------------------------------------------------------------ 10. Steps (process steps insert)
s = "steps"
setup(s, CREAM, T("push", 650, **{"from": "right"}), "Use Next and Back to walk through the four steps.", duration=NARR["steps"]["duration_ms"] + 1000)
slide(s)["revisit"] = "reset"
steps4 = [("Sit tall", "Feet flat, hands resting in your lap. Let your shoulders drop away from your ears."),
          ("Breathe in · 4", "Breathe in slowly through your nose while you count to four."),
          ("Hold · 4", "Hold the breath gently, without strain, for another count of four."),
          ("Breathe out · 6", "Let the air out through your mouth for a count of six. Repeat three times.")]
for i, (title, detail) in enumerate(steps4, 1):
    set_label(s, f"step_label{i}", title)
    set_label(s, f"step_panel{i}_title1", title, size=34, font=SERIF, bold=False)
    set_label(s, f"step_panel{i}_detail1", detail, size=22, color=BODY)
    find(s, f"step_panel{i}_detail1")["frame"] = F(116, 304, 620, 200)
    find(s, f"step_panel{i}_title1")["frame"] = F(116, 240, 620, 54)
    ent(find(s, f"step_panel{i}_title1"), RISE, 600, 0, "overshoot")
    ent(find(s, f"step_panel{i}_detail1"), FADE, 500, 250)
    pfr = F(850, 236, 320, 320)
    sp = add(s, image(f"st_pic{i}", "breathe.jpg", "A woman breathing calmly", pfr, crop=crop_for(pfr, 0.72), name=f"Step {i} picture", mask="ellipse"), f"step_layer{i}")
    ent(sp, reveal_shape("circle"), 700, 100)
for i in range(1, 5):
    pnl = find(s, f"step_panel{i}1")
    pnl["kind"]["fill"] = WHITE
    pnl["kind"]["outline"] = {"color": SAGE, "width": 2, "dash": "solid"}
    pnl["shadow"] = CARD
find(s, "step_back1")["frame"] = F(88, 610, 120, 48)
find(s, "step_next1")["frame"] = F(218, 610, 120, 48)
next_button(s, 1020, 610, oid="steps_next")
voice(s, "steps", x=1100, y=28)

# ------------------------------------------------------------------ 11. Check-in: sliders, meters, conditions
s = "checkin"
setup(s, CREAM, T("dissolve", 800), "Move the sliders. The page reacts to your answers. Nothing here is scored.", duration=NARR["checkin"]["duration_ms"] + 1500)
eyebrow(s, "ci_eyebrow", "CHECK IN  ·  NOT SCORED", 88, 48, start=0)
add(s, ent(heading("ci_title", "How are you feeling today?", 84, 76, 900, 64, 44), RISE, 700, 150, "overshoot", "by_letter"))
for j, (cid, q, v, lo, hi, meter_lbl, tip_lo, tip_hi, high_when) in enumerate([
        ("stress", "How stressed do you feel right now?", "stress", "Calm", "Very stressed", "Stress",
         "You're doing fine. Keep taking short breaks to stay that way.", "That's a lot. Try the one-minute breathing reset before your next task.", ("ge", 7)),
        ("energy", "How is your energy?", "energy", "Running on empty", "Full of beans", "Energy",
         "Good energy. A walk after lunch helps it last through the afternoon.", "Running low? Stand up, drink some water and get some daylight.", ("le", 3))]):
    x = 88 + j * 568
    card = add(s, shape(f"ci_card{j + 1}", F(x, 164, 536, 430), WHITE, name=f"{meter_lbl} card", adjust=[0.05]))
    card["shadow"] = CARD
    ent(card, RISE, 700, cue("checkin", 1) + j * 250, "overshoot")
    parts = [text(f"ci_q{j + 1}", q, F(x + 32, 194, 470, 30), 21, INK, True, f"{meter_lbl} question"),
             obj(f"ci_slider{j + 1}" if j else "ci_slider", f"{meter_lbl} slider", {"type": "slider", "variable": v, "min": 0, "max": 10, "step": 1},
                 F(x + 32, 252, 470, 40), description=f"{meter_lbl} from 0 to 10"),
             text(f"ci_lo{j + 1}", lo, F(x + 32, 298, 230, 20), 14, MUTED, False, "Low end"),
             text(f"ci_hi{j + 1}", hi, F(x + 272, 298, 230, 20), 14, MUTED, False, "High end", align="right"),
             obj(f"ci_meter{j + 1}" if j else "ci_meter", f"{meter_lbl} meter", {"type": "progress_meter", "variable": v, "min": 0, "max": 10, "style": "bar"},
                 F(x + 32, 340, 470, 14), description=f"{meter_lbl} level"),
             text(f"ci_value{j + 1}", f"{meter_lbl}: %{v}% out of 10", F(x + 32, 368, 470, 30), 20, DEEP, True, f"{meter_lbl} value")]
    for p in parts:
        add(s, ent(p, FADE, 500, cue("checkin", 1) + 400 + j * 250))
    tip = add(s, shape(f"ci_tip{j + 1}" if j else "ci_tip", F(x + 32, 420, 470, 140), MIST, name=f"{meter_lbl} tip", adjust=[0.1],
                       label=tip_lo, lstyle=style(18, DEEP, False, align="left", valign="middle", insets={"left": 20, "right": 20, "top": 10, "bottom": 10})))
    tip["states"] = [{"name": "high", "fill": BLUSH, "text": tip_hi}]
    ent(tip, wipe("left"), 600, cue("checkin", 2))
    tid = tip["id"]
    op, val = high_when
    trig(s, {"event": "variable_changes", "variable": v}, state(tid, "high"), cmp(v, op, val))
    trig(s, {"event": "variable_changes", "variable": v}, state(tid, "normal"), cmp(v, {"ge": "lt", "le": "gt"}[op], val))
next_button(s, 1020, 616, start=cue("checkin", 2) + 600)
voice(s, "checkin", x=1100, y=28)

# ------------------------------------------------------------------ Connect opener
opener("connect_intro", "03", "Connect", "People who feel connected at work are happier, and they do better work too.", "connect.jpg",
       "Two colleagues laughing on a walking meeting", 0.72, "connect_intro", T("shape", 1000, shape="diamond", inout="out"))

# ------------------------------------------------------------------ 12. Scenario
s = "scenario"
setup(s, CREAM, T("fade", 600), "Choose what you'd do. You can try the other answers too.", duration=NARR["scenario"]["duration_ms"] + 1000)
set_label(s, "scenario_title1", "What would you do?", color=WHITE, font=SERIF, bold=False, size=40)
set_label(s, "scenario_body1", "Your afternoon is packed, and a colleague asks if you'd like a quick walk. You've both been on screens since 9.", color=MIST)
find(s, "scenario_panel1")["kind"]["fill"] = FOREST
find(s, "scenario_panel1")["kind"].pop("outline", None)
find(s, "scenario_panel1")["shadow"] = SOFT
for i, label in enumerate(["Say no: there's too much to do", "Say yes, and walk for ten minutes", "Suggest a coffee at your desks instead"], 1):
    set_label(s, f"scenario_choice{i}", label)
    ent(find(s, f"scenario_choice{i}"), fly("right"), 550, cue("scenario", 2) + i * 180, "overshoot")
set_label(s, "scenario_feedback_text1", "Staying put feels productive, but your focus drops after hours at a screen. Ten minutes outside often pays for itself.")
set_label(s, "scenario_feedback_text2", "Great call. A short walk and a real conversation reset your focus and mood, and the work will still be there.")
set_label(s, "scenario_feedback_text3", "Company helps, but you'd both still be sitting at a screen. Getting up and outside is the bigger win.")
for i, t in enumerate(["Not the best choice", "Great choice!", "Nearly there"], 1):
    set_label(s, f"scenario_feedback_title{i}", t)
for oid, fr in [("scenario_panel1", F(88, 96, 1104, 196)), ("scenario_title1", F(124, 122, 1000, 56)), ("scenario_body1", F(124, 186, 900, 80))]:
    find(s, oid)["frame"] = fr
find(s, "scenario_body1")["kind"]["style"]["size"] = 21
for i in (1, 2, 3):
    find(s, f"scenario_choice{i}")["frame"] = F(88, 316 + (i - 1) * 76, 1104, 60)
    find(s, f"scenario_choice{i}")["kind"]["fill"] = TEAL
    find(s, f"scenario_choice{i}")["states"] = [{"name": "hover", "fill": DEEP}]
ent(find(s, "scenario_panel1"), split("horizontal", "out"), 800, 0)
ent(find(s, "scenario_title1"), RISE, 700, 400, "overshoot")
ent(find(s, "scenario_body1"), FADE, 600, cue("scenario", 1))
bg = add(s, image("sc_pic", "connect.jpg", "Two colleagues walking outside", F(0, 0, 1280, 720), crop=crop_for(F(0, 0, 1280, 720)), name="Background picture"), index=0)
bg["opacity"] = 0.16
bg["decorative"] = True
next_button(s, 1020, 620)
voice(s, "scenario", x=1100, y=28)

# ------------------------------------------------------------------ 13. Day (timeline insert)
s = "day"
setup(s, CREAM, T("wipe", 800, **{"from": "left"}), "Tap each time to see a habit that fits that part of the day.", duration=NARR["day"]["duration_ms"] + 2500)
band = add(s, image("dy_band", "day.jpg", "A landscape from sunrise to sunset", F(0, 150, 1280, 150), crop={"x": 0.0, "y": 0.33, "w": 1.0, "h": 0.28}, name="Day panorama"), index=0)
band["opacity"] = 0.55
band["decorative"] = True
ent(band, wipe("left"), 1400, 300, "ease_in_out")
eyebrow(s, "dy_eyebrow", "LESSON 3  ·  CONNECT", 88, 40, start=0)
add(s, ent(heading("dy_title", "A healthy day, hour by hour", 84, 66, 900, 64, 44), RISE, 700, 150, "overshoot"))
day = [("9:00", "Set up", "Adjust your chair and screen before you open email. Two minutes now saves an aching neck later.", "desk.jpg", 0.45),
       ("11:00", "Refill", "Stand up, refill your water and look out of a window on the way.", "eyes.jpg", 0.4),
       ("13:00", "Walk", "Take lunch away from your desk. A ten-minute walk after eating helps the afternoon dip.", "connect.jpg", 0.7),
       ("15:00", "Breathe", "One minute of paced breathing before the late-afternoon push.", "breathe.jpg", 0.72),
       ("18:00", "Switch off", "Close the laptop, write tomorrow's first task down and let the day end.", "evening.jpg", 0.7)]
for i, (t, title, detail, src, focus) in enumerate(day, 1):
    set_label(s, f"timeline_year{i}", t, color=CORAL, bold=True, size=18)
    find(s, f"timeline_point{i}")["description"] = f"{t}: {title}"
    set_label(s, f"timeline_title{i}", title, color=INK, bold=True, size=18)
    set_label(s, f"timeline_panel{i}_title1", title, size=34, font=SERIF, bold=False)
    set_label(s, f"timeline_panel{i}_detail1", detail, size=21, color=BODY)
    pfr = F(700, 328, 470, 264)
    add(s, image(f"dy_pic{i}", src, title, pfr, crop=crop_for(pfr, focus), name=f"{title} picture"), f"timeline_layer{i}")
    ent(find(s, f"dy_pic{i}"), peek("right"), 600, 100)
for o in slide(s)["objects"]:
    if o["id"].startswith("timeline_"):
        o["frame"]["y"] += 90
for i in range(1, 6):
    L = layer(s, f"timeline_layer{i}")
    for o in L["objects"]:
        if not o["id"].startswith("dy_pic"):
            o["frame"]["y"] += 60
    find(s, f"timeline_panel{i}1")["frame"]["h"] = 340
    find(s, f"timeline_panel{i}1")["kind"]["fill"] = WHITE
    find(s, f"timeline_panel{i}1")["shadow"] = CARD
    find(s, f"timeline_panel{i}_title1")["frame"] = F(112, 274, 520, 48)
    find(s, f"timeline_panel{i}_detail1")["frame"] = F(112, 330, 520, 200)
    ent(find(s, f"timeline_panel{i}_title1"), RISE, 600, 0, "overshoot")
    for oid in (f"timeline_point{i}", f"timeline_year{i}", f"timeline_title{i}"):
        ent(find(s, oid), POP, 450, cue("day", 0) + 600 + i * 300, "overshoot")
hintc = add(s, shape("dy_hint", F(88, 330, 1104, 250), WHITE, name="Hint card", adjust=[0.06]))
hintc["shadow"] = CARD
ent(hintc, RISE, 700, cue("day", 1), "overshoot")
bulb = add(s, shape("dy_bulb", F(150, 400, 96, 96), GOLD, "lightbulb", name="Idea icon"))
bulb["decorative"] = True
ent(bulb, POP, 500, cue("day", 1) + 300, "overshoot")
emph(bulb, "teeter", cue("day", 1) + 1200, 1000, 2)
add(s, ent(text("dy_hint_t", "Tap a time on the timeline", F(280, 392, 820, 50), 34, INK, False, "Hint title", font=SERIF), FADE, 500, cue("day", 1) + 400))
add(s, ent(body("dy_hint_b", "Each one opens a habit that fits that part of the day, from setting up at nine to switching off at six.", 280, 446, 800, 64, 19), FADE, 500, cue("day", 1) + 600))
next_button(s, 1020, 620, start=cue("day", 1) + 500)
voice(s, "day", x=1100, y=28)

# ------------------------------------------------------------------ 14. Tips (reveal grid insert)
s = "tips"
setup(s, SAND, T("random_bars", 700, orientation="vertical"), "Tap each tile to reveal a quick win.", duration=NARR["tips"]["duration_ms"] + 2000)
slide(s)["revisit"] = "reset"
eyebrow(s, "tp_eyebrow", "SIX QUICK WINS", 88, 36, CORAL_DK, start=0)
tips = [("Stand for calls", "Every phone call is a chance to stand and walk."),
        ("Stairs first", "One flight up beats the lift for your heart."),
        ("Bottle on desk", "If you can see it, you'll drink it."),
        ("Walking 1:1s", "Talk side by side instead of face to screen."),
        ("Lunch away", "Eat anywhere but your keyboard."),
        ("Last task first", "Write tomorrow's first task down, then log off.")]
for i, (front, back) in enumerate(tips, 1):
    o = find(s, f"grid_tile_shape{i}")
    o["kind"]["fill"] = WHITE
    o["kind"].pop("outline", None)
    o["kind"]["text"]["text"] = front
    o["kind"]["text"]["style"].update(style(26, DEEP, False, align="center", valign="middle", font=SERIF))
    o["shadow"] = CARD
    o["frame"]["y"] = o["frame"]["y"] + 10
    for stt in o.get("states", []):
        if stt["name"] == "Revealed":
            stt["fill"] = "#DDEDE3"
            stt["text"] = back
    ent(o, strips("top_left"), 700, cue("tips", 1) + (i - 1) * 140)
set_label(s, "grid_counter1", "%grid1_explored% of 6 revealed", color=DEEP, bold=True)
next_button(s, 1060, 26, "Next  →", w=140)
find(s, "tips_next")["frame"] = F(940, 28, 140, 44)
voice(s, "tips", x=1100, y=28)

# ------------------------------------------------------------------ 15. Plan: radio group, checkbox, slider, table
s = "plan"
setup(s, CREAM, T("push", 650, **{"from": "right"}), "Choose your first habit, how long you'll give it, and look at a sample week.", duration=NARR["plan"]["duration_ms"] + 1500)
eyebrow(s, "pl_eyebrow", "MAKE IT STICK", 88, 48, start=0)
add(s, ent(heading("pl_title", "Your plan for next week", 84, 76, 700, 64, 44), RISE, 700, 150, "overshoot", "by_letter"))
pp = add(s, picture_card("pl_pic", "plan.jpg", "An open notebook on a tidy desk", F(88, 168, 560, 312), 0.05, "Planner picture"))
pp["shadow"] = CARD
ent(pp, reveal_shape("box"), 900, 300)
def cell(t, fill=None, bold=False, color=INK):
    d = {"text": t, "style": style(14, color, bold, align="center", valign="middle"), "valign": "middle"}
    if fill:
        d["fill"] = fill
    return d
rows = [["", "Move", "Breathe", "Connect"], ["Mon", "Stand for calls", "Before email", "—"], ["Wed", "Stairs", "After lunch", "Walking 1:1"],
        ["Fri", "Walk at lunch", "End of day", "Thank someone"]]
table = {"type": "table", "columns": [0.16, 0.28, 0.28, 0.28], "header_row": True,
         "rows": [{"height": 1, "cells": [cell(t, TEAL if r == 0 and j > 0 else (MIST if j == 0 else WHITE), r == 0 or j == 0, WHITE if r == 0 and j > 0 else INK) for j, t in enumerate(row)]} for r, row in enumerate(rows)],
         "borders": {"outer": {"color": SAGE, "width": 2}, "inner": {"color": "#D7E5DD", "width": 1}}}
add(s, ent(obj("pl_table", "Sample week", table, F(88, 498, 560, 150), description="A sample week of habits"), FADE, 600, cue("plan", 1)))
p = add(s, shape("pl_card", F(688, 168, 504, 424), WHITE, name="Choice card", adjust=[0.05])); p["shadow"] = CARD
ent(p, RISE, 700, cue("plan", 1), "overshoot")
items = [text("pl_q", "Which habit will you start with?", F(720, 192, 440, 30), 21, INK, True, "Habit question"),
         obj("pl_radio", "First habit", {"type": "radio_group", "variable": "first_habit", "options": [
             {"label": "Move more", "value": "move"}, {"label": "Breathe and reset", "value": "breathe"}, {"label": "Connect with people", "value": "connect"}]},
             F(720, 232, 440, 124), description="Your first habit"),
         text("pl_q2", "Minutes a day: %plan_minutes%", F(720, 372, 440, 28), 19, INK, True, "Minutes question"),
         obj("pl_minutes", "Minutes a day", {"type": "slider", "variable": "plan_minutes", "min": 5, "max": 30, "step": 5}, F(720, 410, 440, 36),
             description="Minutes a day for your habit"),
         obj("pl_check", "Reminders", {"type": "checkbox", "variable": "reminders", "label": "Remind me to take a break every hour"}, F(720, 470, 440, 40))]
for k, it in enumerate(items):
    add(s, ent(it, FADE, 500, cue("plan", 1) + 400 + k * 120))
note = add(s, text("pl_note", "We'll nudge you hourly.", F(720, 516, 440, 26), 16, MUTED, False, "Reminder note"))
note["states"] = [{"name": "off", "text": "No reminders. You've got this."}]
trig(s, {"event": "variable_changes", "variable": "reminders"}, state("pl_note", "off"), cmp("reminders", "eq", False))
trig(s, {"event": "variable_changes", "variable": "reminders"}, state("pl_note", "normal"), cmp("reminders", "eq", True))
add(s, ent(text("pl_pick", "Your pick: %first_habit%, %plan_minutes% minutes a day.", F(720, 556, 440, 28), 17, TEAL, True, "Your pick"), FADE, 400, cue("plan", 1) + 1200))
next_button(s, 1012, 608, start=cue("plan", 1) + 1400)
voice(s, "plan", x=1100, y=28)

# ------------------------------------------------------------------ Quiz and reflections
QSPEC = {
    "q1": dict(prompt="You've been at your desk for two hours. What's best now?", choices=[
        ("a", "Stand, stretch and look at something far away", True), ("b", "Push through: breaks break focus", False),
        ("c", "Check your phone for a few minutes", False), ("d", "Wait until lunch, then rest", False)], attempts=2, shuffle=True),
    "q2": dict(prompt="Which of these help you calm down quickly? Choose all that apply.", choices=[
        ("a", "A minute of slow breathing", True), ("b", "Another cup of strong coffee", False), ("c", "Looking out of a window", True), ("d", "A short walk", True)], scoring="partial"),
    "q3": dict(prompt="True or false: a walking meeting can be as useful as a video call.", choices=[("true", "True", True), ("false", "False", False)]),
    "q4": dict(prompt="Look 20 feet away for how many seconds? Type a number.", accepted=["20", "twenty"]),
    "q5": dict(prompt="Put the paced breathing steps in order.", choices=[
        ("a", "Sit tall and relax your shoulders", False), ("b", "Breathe in for four", False), ("c", "Hold for four", False), ("d", "Breathe out for six", False)]),
    "q6": dict(prompt="Match each habit to the best time of day.", pairs=[("p1", "Set up your desk", "Start of the day"), ("p2", "Walk outside", "Lunch break"), ("p3", "Write tomorrow's first task", "End of the day")]),
    "q7": dict(prompt="Click where the top of the screen should line up with your eyes."),
    "q8": dict(prompt="Drag each item to the habit it belongs to."),
    "q9": dict(prompt="Set the slider to how often you should change position."),
    "r1": dict(prompt="This course gave me ideas I can use tomorrow."),
    "r2": dict(prompt="In a few words: which habit will you try first?"),
    "r3": dict(prompt="Describe one change to your workday, and when you'll make it."),
}
QSIDE = {"q1": ("desk.jpg", 0.45), "q2": ("breathe.jpg", 0.72), "q3": ("connect.jpg", 0.72), "q4": ("eyes.jpg", 0.35),
         "q5": ("breathe.jpg", 0.72), "q6": ("evening.jpg", 0.72), "q8": ("quiz.jpg", 0.72), "q9": ("move.jpg", 0.72),
         "r1": ("hero.jpg", 0.62), "r2": ("plan.jpg", 0.5), "r3": ("evening.jpg", 0.72)}
TRANS = [T("fade", 500), T("push", 600, **{"from": "right"}), T("reveal", 650, **{"from": "right"}), T("wipe", 650, **{"from": "right"})]
for n, (sid, kind) in enumerate([(q[0], q[1]) for q in QUIZ] + REFLECT, 1):
    sp = QSPEC[sid]
    setup(sid, CREAM, TRANS[n % len(TRANS)])
    q = slide(sid)["question"]
    q["prompt"] = sp["prompt"]
    if "choices" in sp:
        q["choices"] = [{"id": i, "text": t, "correct": cr} for i, t, cr in sp["choices"]]
    if "accepted" in sp:
        q["accepted"] = sp["accepted"]
    if "pairs" in sp:
        q["pairs"] = [{"id": i, "left": l, "right": r} for i, l, r in sp["pairs"]]
    for k in ("attempts", "shuffle", "scoring"):
        if k in sp:
            q[k] = sp[k]
    if sid in ("q1", "q2", "q3"):
        label = f"QUICK CHECK  ·  LESSON {sid[1:]}"
    elif sid.startswith("q"):
        label = f"QUESTION {int(sid[1:]) - 3} OF 6"
    else:
        label = f"REFLECTION {sid[1:]} OF 3  ·  NOT SCORED"
    if sid in QSIDE:
        # The player lays out the prompt and answers across the slide, so the picture sits in the free corner.
        src, focus = QSIDE[sid]
        side = add(sid, picture_card(f"{sid}_side", src, "", F(792, 430, 408, 228), 0.06, "Corner picture"), index=0)
        side["decorative"] = True
        side["shadow"] = CARD
        ent(side, reveal_shape("box"), 800, 300, "ease_in_out")
    eyebrow(sid, f"{sid}_eyebrow", label, 88, 32, start=0)
    sub = find(sid, "button2" if sid == "q8" else "button1")
    sub["kind"]["label"] = "Submit" if sid.startswith("q") else "Save answer"
    sub["kind"]["fill"] = CORAL
    sub["frame"] = F(88, 604, 180, 54)
    sub["states"] = [{"name": "hover", "fill": CORAL_DK}]
    sub["shadow"] = LIFT
    ent(sub, POP, 450, 600, "overshoot")
    # feedback layers: a friendly bar with an icon, the message and a Continue button
    for lid, toid, fill, icon in [("Correct1", "feedback_text1", TEAL, "✓"), ("Incorrect1", "feedback_text2", "#A9472E", "!")]:
        if lid not in [l["id"] for l in slide(sid).get("layers", [])]:
            continue
        L = layer(sid, lid)
        L["prevent_base_clicks"] = True
        bar = add(sid, ent(shape(f"{sid}_{lid}_toast", F(288, 590, 700, 84), fill, name="Feedback panel", adjust=[0.25]), fly("bottom"), 450, 0, "overshoot"), lid, 0)
        bar["shadow"] = SOFT
        ic = add(sid, shape(f"{sid}_{lid}_icon", F(306, 610, 44, 44), "#FFFFFF33", "ellipse", name="Feedback icon", label=icon,
                            lstyle=style(22, WHITE, True, align="center", valign="middle")), lid, 1)
        ent(ic, POP, 400, 250, "overshoot")
        emph(ic if icon == "✓" else bar, "jump" if icon == "✓" else "shake", 700, 700)
        t = find(sid, toid)
        t["frame"] = F(364, 604, 470, 56)
        t["kind"]["style"].update(style(18, WHITE, True, valign="middle"))
        ent(t, FADE, 400, 200)
        go = add(sid, button(f"{sid}_{lid}_go", "Continue  →", F(842, 610, 132, 44), FOREST), lid)
        go.pop("shadow")
        trig(sid, click(f"{sid}_{lid}_go"), jump("next"), lid=lid)
    fb = {"q1": ("Correct! A short movement break rests your eyes and your back.", "Not quite. Your body needs a change of position more often than that."),
          "q2": ("Yes! Breathing, looking far away and walking all calm you.", "Look again: more caffeine tends to wind you up."),
          "q3": ("True. Many one-to-ones work better on foot.", "It's true: most catch-ups don't need a screen."),
          "q4": ("Correct: 20 seconds is enough.", "It's 20 seconds: 20-20-20."),
          "q5": ("Perfect order. Breathe easy!", "Not quite: sit, in, hold, out."),
          "q6": ("All matched!", "Some pairs are off. Desk first, walk at lunch, plan at the end."),
          "q7": ("Spot on: the top of the screen at eye level.", "Look higher: the top of the monitor."),
          "q8": ("Nicely sorted!", "A couple are in the wrong place."),
          "q9": ("Yes: every 30 minutes or so.", "Aim for about every 30 minutes."),
          "r1": ("Thanks for your answer!", "Thanks for your answer!"),
          "r2": ("Thanks! Writing it down makes it more likely.", "Thanks!"),
          "r3": ("Thanks! A plan with a time attached is much easier to keep.", "Thanks!")}
    ok, no = fb[sid]
    try:
        set_label(sid, "feedback_text1", ok)
        set_label(sid, "feedback_text2", no)
    except KeyError:
        pass

for sid in ("q1", "q2", "q3"):
    voice(sid, "check", x=844, y=24, oid=f"{sid}_voice")
voice("q4", "quiz", x=844, y=24, oid="q4_voice")
voice("r1", "reflect", x=844, y=24, oid="r1_voice")

# hotspot on the desk picture
s = "q7"
hs = find(s, "image1")
hfr = F(88, 150, 640, 420)
hs["kind"].update({"src": "desk.jpg", "alt": "Side view of a woman sitting at her desk", "crop": crop_for(hfr, 0.45)})
hs["frame"] = hfr
slide(s)["question"]["regions"] = [{"id": "r_screen", "shape": "ellipse", "x": 505, "y": 185, "w": 150, "h": 80, "correct": True}]
add(s, ent(text("q7_hint", "Tip: the screen is on the desk, on the right of the picture.", F(760, 160, 220, 110), 16, MUTED, False, "Hint", ls=1.35), FADE, 500, 800))
ent(hs, EXPAND, 700, 200)

# drag and drop question
s = "q8"
slide(s)["objects"] = [o for o in slide(s)["objects"] if o["id"] not in ("button1", "shape1", "text1")]
dd_items = [("dd_water", "Water bottle", "dd_move"), ("dd_stairs", "Stairs", "dd_move"), ("dd_count", "Count of four", "dd_breathe"), ("dd_walk", "Walking 1:1", "dd_connect")]
for j, (tid, label) in enumerate([("dd_move", "Move"), ("dd_breathe", "Breathe"), ("dd_connect", "Connect")]):
    tg = add(s, shape(tid, F(88 + j * 300, 340, 280, 200), [MIST, BLUSH, "#E3EEF0"][j], name=f"{label} target", adjust=[0.08],
                      outline={"color": [TEAL, CORAL, DEEP][j], "width": 2, "dash": "dash"}, label=label,
                      lstyle=style(26, [TEAL, CORAL_DK, DEEP][j], False, align="center", valign="top", font=SERIF, insets={"left": 10, "right": 10, "top": 18, "bottom": 10})))
    ent(tg, wipe("top"), 600, 300 + j * 150)
for j, (iid, label, tgt) in enumerate(dd_items):
    b = add(s, button(iid, label, F(88 + j * 226, 176, 206, 56), DEEP, name=label))
    ent(b, POP, 450, 700 + j * 120, "overshoot")
q = slide(s)["question"]
find(s, "button2")["frame"] = F(88, 604, 180, 54)
q["drag_items"] = [{"item": iid, "target": tgt} for iid, _, tgt in dd_items]
q["drop_targets"] = ["dd_move", "dd_breathe", "dd_connect"]
q["snap_to_center"] = True
q["one_per_target"] = False

# freeform question graded by a slider
s = "q9"
add(s, ent(obj("q9_slider", "Minutes slider", {"type": "slider", "variable": "change_minutes", "min": 0, "max": 120, "step": 15}, F(88, 230, 840, 44),
               description="Minutes between position changes"), wipe("left"), 600, 300))
add(s, ent(text("q9_value", "Every %change_minutes% minutes", F(88, 290, 840, 50), 34, DEEP, False, "Slider value", font=SERIF), FADE, 500, 500))
add(s, text("q9_lo", "0 min", F(88, 346, 200, 20), 14, MUTED, False, "Low end"))
add(s, text("q9_hi", "120 min", F(728, 346, 200, 20), 14, MUTED, False, "High end", align="right"))
slide(s)["question"]["correct_when"] = {"all": [cmp("change_minutes", "ge", 15), cmp("change_minutes", "le", 30)]}
trig(s, {"event": "timeline_starts"}, setv("change_minutes", 60.0))

# likert scale labels
slide("r1")["question"]["choices"] = [{"id": str(i), "text": t, "correct": False} for i, t in enumerate(["Strongly disagree", "Disagree", "Not sure", "Agree", "Strongly agree"], 1)]

# results slide restyle, with narration and confetti on the passed layer
s = "results1"
setup(s, CREAM, T("zoom", 800), "Your quiz score. Retry or review your answers.", master=True)
slide(s)["title"] = "Your results"
set_label(s, "title1", "Your results", size=46, color=INK, font=SERIF, bold=False)
set_label(s, "score1", "%Course.ScorePercent%%", size=104, color=TEAL, bold=False, font=SERIF)
find(s, "score1")["frame"] = F(100, 160, 520, 130)
ent(find(s, "score1"), POP, 800, 300, "overshoot")
set_label(s, "points1", "Points: %Course.ScorePoints%", color=BODY, size=22)
set_label(s, "passing1", "You need 80%% to pass.", color=MUTED, size=20)
find(s, "points1")["frame"] = F(104, 296, 500, 36)
find(s, "passing1")["frame"] = F(104, 336, 500, 36)
ring = add(s, shape("rs_ring", F(740, 100, 460, 460), "#FFFFFF00", "ellipse", name="Dotted ring", outline={"color": SAGE, "width": 2, "dash": "dot"}))
ring["decorative"] = True
ent(ring, wheel(1), 1200, 200)
emph(ring, "spin", 1400, 30000, 1, "linear")
bdg = add(s, image("rs_badge", "badge.jpg", "An achievement rosette with a leaf", F(780, 140, 380, 380), crop={"x": 0.22, "y": 0.0, "w": 0.558, "h": 1.0}, name="Badge", mask="ellipse"))
ent(bdg, SPIN_GROW, 1100, 400, "overshoot")
for lid in ("passed", "failed"):
    for o in layer(s, lid)["objects"]:
        if o["kind"]["type"] == "button":
            o["frame"]["y"] = 600
            o["kind"]["fill"] = CORAL if ("finish" in o["id"] or "retry" in o["id"]) else TEAL
            o["states"] = [{"name": "hover", "fill": CORAL_DK if o["kind"]["fill"] == CORAL else TEAL_DK}]
set_label(s, "message1", "You passed. Great work, %learner_name%!", size=28, color=DEEP, bold=True)
set_label(s, "message2", "Not quite there yet. Review your answers or try again.", size=26, color=CORAL_DK, bold=True)
for mid in ("message1", "message2"):
    find(s, mid)["frame"] = F(104, 420, 620, 80)
for o in layer(s, "passed")["objects"]:
    if o["id"] == "passed_finish1":
        o["kind"]["label"] = "Continue  →"
for t in layer(s, "passed")["triggers"]:
    if t["when"].get("object") == "passed_finish1":
        t["action"] = jump("next")
voice(s, "passed", lid="passed", x=1100, y=28, oid="rs_voice_pass")
voice(s, "failed", lid="failed", x=1100, y=28, oid="rs_voice_fail")
confetti(s, "passed", (720, 80, 500, 520), 200, 18, 11)
for lid, plan in [("passed", [("message1", 0), ("passed_review1", cue("passed", 1) + 300), ("passed_finish1", cue("passed", 2))]),
                  ("failed", [("message2", 0), ("failed_review1", cue("failed", 1)), ("failed_retry1", cue("failed", 2))])]:
    layer(s, lid)["duration_ms"] = max(layer(s, lid).get("duration_ms", 0), NARR[lid]["duration_ms"] + 800)
    for oid, ms in plan:
        o = next(x for x in layer(s, lid)["objects"] if x["id"] == oid)
        ent(o, FADE if "message" in oid else POP, 450, ms, "ease_out" if "message" in oid else "overshoot")
emph(next(x for x in layer(s, "failed")["objects"] if x["id"] == "message2"), "shake", 500, 600)

# certificate restyle
s = "certificate1"
setup(s, CREAM, T("fade", 700), "Print or save your certificate.", master=False, duration=NARR["certificate"]["duration_ms"] + 1500)
slide(s)["title"] = "Your certificate"
find(s, "border1")["kind"]["outline"] = {"color": SAGE, "width": 6, "dash": "solid"}
ent(find(s, "border1"), reveal_shape("box"), 900, 0)
find(s, "learner1")["kind"]["text"] = "%learner_name%"
for oid, color in [("title1", DEEP), ("certifies1", MUTED), ("learner1", CORAL), ("completed1", MUTED), ("course1", INK), ("score1", BODY), ("date1", MUTED)]:
    find(s, oid)["kind"]["style"]["color"] = color
for oid in ("title1", "learner1", "course1"):
    find(s, oid)["kind"]["style"]["font"] = SERIF
    find(s, oid)["kind"]["style"]["bold"] = False
find(s, "learner1")["kind"]["style"]["size"] = 56
find(s, "learner1")["frame"] = F(128, 256, 1024, 80)
ent(find(s, "learner1"), RISE, 1000, 600, "overshoot")
for k, oid in enumerate(("title1", "certifies1", "completed1", "course1", "score1", "date1")):
    ent(find(s, oid), FADE, 500, 300 + k * 150)
seal = add(s, image("ce_seal", "badge.jpg", "A rosette seal", F(1030, 470, 130, 130), crop={"x": 0.22, "y": 0.0, "w": 0.558, "h": 1.0}, name="Seal", mask="ellipse"))
ent(seal, SPIN_GROW, 900, 1600, "overshoot")
pr = find(s, "print1"); pr["kind"]["fill"] = TEAL; pr["frame"] = F(830, 620, 150, 48)
next_button(s, 1000, 620, "Finish  →", w=160)
voice(s, "certificate", x=130, y=622)

# ------------------------------------------------------------------ Finish
s = "finish"
setup(s, FOREST, T("shape", 1100, shape="circle", inout="out"), "Well done! Small habits, repeated often, make the biggest difference.",
      master=False, duration=NARR["finish"]["duration_ms"] + 1500)
fr = add(s, shape("f_ring", F(720, 90, 500, 500), "#FFFFFF00", "ellipse", name="Ring", outline={"color": "#A8C3B188", "width": 2, "dash": "dot"}))
fr["decorative"] = True
ent(fr, wheel(1), 1400, 0)
emph(fr, "spin", 1500, 30000, 1, "linear")
fpic = add(s, image("f_pic", "evening.jpg", "A woman happily closing her laptop at golden hour", F(760, 130, 420, 420), crop={"x": 0.42, "y": 0.0, "w": 0.558, "h": 1.0},
                    name="Evening picture", mask="ellipse"))
ent(fpic, reveal_shape("circle"), 1100, 300, "ease_in_out")
fb2 = add(s, image("f_badge", "badge.jpg", "An achievement rosette with a leaf", F(700, 420, 150, 150), crop={"x": 0.22, "y": 0.0, "w": 0.558, "h": 1.0}, name="Badge", mask="ellipse"))
ent(fb2, SPIN_GROW, 900, 1200, "overshoot")
emph(fb2, "teeter", 3000, 1000, 2)
confetti(s, None, (680, 60, 560, 560), 1000, 16, 5)
eyebrow(s, "f_eyebrow", "COURSE COMPLETE", 88, 160, SAGE, start=200)
add(s, ent(text("f_title", "Well done,\n%learner_name%!", F(84, 188, 620, 190), 70, ON_DARK, False, "Title", ls=0.98, font=SERIF), RISE, 1000, cue("finish", 0), "overshoot", "by_paragraph"))
fsub = add(s, ent(text("f_sub", "Small habits, repeated often, make the biggest difference.", F(88, 400, 520, 70), 22, ON_DARK_SOFT, False, "Subtitle", ls=1.4), FADE, 600, cue("finish", 1)))
add(s, ent(button("f_finish", "Finish course", F(88, 500, 220, 58), CORAL), POP, 500, cue("finish", 2), "overshoot"))
add(s, ent(button("f_restart", "Start again", F(322, 500, 170, 58), "#FFFFFF2E"), POP, 500, cue("finish", 2) + 150, "overshoot")).pop("shadow")
emph(find(s, "f_finish"), "heartbeat", cue("finish", 2) + 1600, 1000, 2)
trig(s, click("f_finish"), {"action": "exit_course"})
trig(s, click("f_restart"), jump("welcome"))
voice(s, "finish", x=88, y=620, dark=True, oid="f_voice")

# ------------------------------------------------------------------ Bonus (lightbox target)
s = "bonus"
setup(s, WHITE, T_FADE, "Blink more, and the 20-20-20 rule explained.", master=False, duration=NARR["bonus"]["duration_ms"] + 1000)
bfr = F(0, 0, 540, 720)
add(s, ent(image("bn_pic", "eyes.jpg", "A man resting his eyes by looking out of a window", bfr, crop={"x": 0.12, "y": 0.0, "w": 0.5, "h": 1.0}, name="Eyes picture"), peek("left"), 700))
eyebrow(s, "bn_eyebrow", "BONUS TIP", 600, 120, start=100)
add(s, ent(heading("bn_title", "Blink more, and\nlook far away", 596, 148, 620, 130, 46), RISE, 800, 300, "overshoot", "by_paragraph"))
add(s, ent(text("bn_list", "Every 20 minutes\nlook 20 feet (6 m) away\nfor 20 seconds", F(600, 296, 580, 140), 24, DEEP, True, "Rule",
                paras=[{"list": "number", "space_after": 10}, {"list": "number", "space_after": 10}, {"list": "number", "space_after": 10}]), FADE, 600, cue("bonus", 1), seq="by_paragraph"))
add(s, ent(body("bn_body", "At a screen we blink about half as often, which dries the eyes. A few slow blinks keep them comfortable.", 600, 450, 580, 90), FADE, 600, cue("bonus", 2)))
lk = add(s, text("bn_link", "Want more? Read the WHO physical activity fact sheet.", F(600, 548, 580, 26), 16, BODY, False, "Link text"))
lk["kind"]["runs"] = [{"text": "Want more? Read the "}, {"text": "WHO physical activity fact sheet", "link": {"type": "url", "url": "https://www.who.int/news-room/fact-sheets/detail/physical-activity"}}, {"text": "."}]
add(s, button("bn_close", "Back to the lesson", F(600, 600, 230, 52)))
trig(s, click("bn_close"), {"action": "close_lightbox"})
voice(s, "bonus", x=1100, y=28)

# ------------------------------------------------------------------ more features: bank draw, template, player rules
# Question bank with a random draw: three quick true/false questions, one is drawn after Question 9.
base_q = copy.deepcopy(slide("q3"))
bank = []
for n, (prompt, right, wrong) in enumerate([
        ("True or false: a two-minute walk counts as a break.", "Right. Even two minutes on your feet resets your body and mind.", "It's true: a short walk is a real break."),
        ("True or false: drinking water through the day helps you focus.", "Right. Even mild thirst makes it harder to concentrate.", "It's true: small sips through the day keep you sharp."),
        ("True or false: looking far away now and then rests your eyes.", "Right. That's the 20-20-20 rule at work.", "It's true: distant focus lets your eye muscles relax.")], 1):
    q = json.loads(json.dumps(base_q).replace("q3_", "bank_"))
    q["id"], q["title"] = f"bank_q{n}", "Bonus question"
    q["question"]["prompt"] = prompt
    q["question"]["choices"] = [{"id": "true", "text": "True", "correct": True}, {"id": "false", "text": "False", "correct": False}]
    q["question"]["points"] = 10
    q["objects"] = [o for o in q["objects"] if o["kind"]["type"] != "audio" and not o["id"].startswith("bank_voice")]
    q["triggers"] = [t for t in q.get("triggers", []) if "bank_voice" not in json.dumps(t)]
    for o in q["objects"]:
        if o["id"] == "bank_eyebrow":
            o["kind"]["text"] = "BONUS  ·  DRAWN AT RANDOM"
    for L in q.get("layers", []):
        for o in L["objects"]:
            if o["id"] == "feedback_text1":
                o["kind"]["text"] = right
            elif o["id"] == "feedback_text2":
                o["kind"]["text"] = wrong
    q["notes"] = "Drawn at random from the Quick checks bank."
    bank.append(q)
C["banks"] = [{"id": "quick_checks", "title": "Quick checks", "slides": bank}]
quiz = next(sc for sc in C["scenes"] if sc["id"] == "sc_quiz")
at_q9 = next(i for i, x in enumerate(quiz["slides"]) if x["id"] == "q9")
quiz["slides"].insert(at_q9 + 1, {"id": "draw1", "title": "Random bonus question", "duration_ms": 5000,
                                   "draw": {"bank": "quick_checks", "count": 1},
                                   "notes": "This slide is replaced by one question picked at random from the Quick checks bank."})

# Per-slide player rules: no seekbar on graded questions; Next waits for the welcome narration.
for sc in C["scenes"]:
    for x in sc["slides"]:
        if x.get("question") and x["id"].startswith("q"):
            x["player"] = {"show_seekbar": False}
slide("welcome")["player"] = {"next_requires": {"kind": "timeline_ends"}}

# A slide template authors can start new lesson pages from (New slide ▸ Your templates).
tpl = {"id": "tpl_lesson", "title": "Lesson page", "duration_ms": 5000, "background": CREAM, "master": "m_content",
       "transition": T("push", 600, **{"from": "right"}), "objects": []}
C["templates"] = [tpl]
_saved = C["scenes"]
C["scenes"] = [{"id": "_tpl", "title": "", "slides": [tpl]}]
eyebrow("tpl_lesson", "t_eyebrow", "LESSON  ·  TOPIC", 88, 120, start=0)
C["scenes"] = _saved
tfr = F(680, 64, 520, 560)
for o in [ent(heading("t_title", "Lesson title", 84, 148, 520, 120, 44), RISE, 800, 200, "overshoot", "by_paragraph"),
          ent(body("t_body", "One or two short sentences that explain the idea. Keep it under 30 words.", 88, 280, 500, 110), FADE, 600, 500),
          ent(image("t_pic", "hero.jpg", "Replace with a picture for this lesson", tfr, crop=crop_for(tfr, 0.62), name="Picture"), peek("right"), 800, 100),
          ent(button("t_next", "Continue  →", F(88, 560, 190, 54), CORAL), POP, 450, 900, "overshoot")]:
    tpl["objects"].append(o)

# Custom font and background music.
C["fonts"] = [{"family": SERIF, "style": "regular", "src": "fonts/dm-serif-display-regular.ttf"}]
C["background_audio"] = {"src": "music.mp3", "volume": 0.1}

# ------------------------------------------------------------------ global restyle of inserted content
def walk(o):
    if isinstance(o, dict):
        for k, v in list(o.items()):
            if isinstance(v, str):
                if v.upper().startswith("#2F6FEB"):
                    o[k] = TEAL + v[7:]
                elif v.upper() == "#1F1F1F88":
                    o[k] = SAGE
                elif v.upper() == "#1F1F1F":
                    o[k] = INK
                elif v == "Open Sans":
                    o[k] = "Inter"
            else:
                walk(v)
    elif isinstance(o, list):
        for v in o:
            walk(v)
walk(C)

def floor16(o):
    if isinstance(o, dict):
        if o.get("decorative"):
            return
        st = o.get("style")
        if isinstance(st, dict) and isinstance(st.get("size"), (int, float)) and st["size"] < 16:
            st["size"] = 16
        for v in o.values():
            floor16(v)
    elif isinstance(o, list):
        for v in o:
            floor16(v)
floor16(C)
C["start_slide"] = "welcome"
with open(os.path.join(ROOT, "course.json"), "w") as f:
    json.dump(C, f, indent=1, ensure_ascii=False)
    f.write("\n")
print("slides:", sum(len(sc["slides"]) for sc in C["scenes"]))
