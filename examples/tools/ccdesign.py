"""Shared helpers for the build scripts of the example courses and starter templates.

A build script runs engine commands with the CLI (pass 1), loads the result, calls `use(course)`
and then edits the course JSON with these helpers (pass 2). Colours are passed in by each script,
so every course keeps its own palette.
"""
import json, os, subprocess, sys

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "../.."))
TARGET = os.environ.get("CARGO_TARGET_DIR") or os.path.join(REPO, "target")
CLI = os.path.join(TARGET, "debug/coursecraft-cli")

C = None          # the course being edited
FONT = "Inter"


def use(course, font="Inter"):
    global C, FONT
    C, FONT = course, font
    return course


def run_pass1(cmds, title, workdir):
    """Run engine commands on a new course and return the result as a dict."""
    os.makedirs(workdir, exist_ok=True)
    script = os.path.join(workdir, "pass1.script.json")
    json.dump(cmds, open(script, "w"), indent=1)
    base = os.path.join(workdir, "base.json")
    subprocess.run([CLI, "new", base, "--title", title], check=True, capture_output=True)
    out = os.path.join(workdir, "pass1.json")
    r = subprocess.run([CLI, "run", base, script, "-o", out], capture_output=True, text=True)
    if r.returncode != 0:
        print(r.stdout[-3000:], r.stderr[-3000:])
        sys.exit(1)
    return json.load(open(out))


class Script:
    """Collects engine commands for pass 1."""

    def __init__(self):
        self.cmds = []

    def __call__(self, command, **params):
        self.cmds.append({"command": command, "params": params})


# ------------------------------------------------------------------ lookup
def slides():
    return [s for sc in C["scenes"] for s in sc["slides"]]


def slide(sid):
    for s in slides():
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


def objects_of(sid, lid=None):
    return (layer(sid, lid) if lid else slide(sid)).setdefault("objects", [])


def remove(sid, oid):
    s = slide(sid)
    for holder in [s] + s.get("layers", []):
        holder["objects"] = [o for o in holder.get("objects", []) if o["id"] != oid]


# ------------------------------------------------------------------ objects
def F(x, y, w, h):
    return {"x": x, "y": y, "w": w, "h": h}


def style(size=20, color="#1F1F1F", bold=False, **kw):
    s = {"font": FONT, "size": size, "color": color, "bold": bold}
    s.update(kw)
    return s


def add(sid, o, lid=None, index=None):
    lst = objects_of(sid, lid)
    if index is None:
        lst.append(o)
    else:
        lst.insert(index, o)
    return o


def obj(oid, name, kind, frame, **extra):
    o = {"id": oid, "name": name, "kind": kind, "frame": frame}
    o.update(extra)
    return o


def text(oid, s, frame, size=20, color="#1F1F1F", bold=False, name=None, ls=None, ts=None, decorative=False, **st):
    k = {"type": "text", "text": s, "style": style(size, color, bold, **st)}
    if ls:
        k["paragraphs"] = [{"line_spacing": ls} for _ in s.split("\n")]
    if ts:
        k["text_style"] = ts
    o = obj(oid, name or oid, k, frame)
    if decorative:
        o["decorative"] = True
    return o


def shape(oid, frame, fill, kind="rounded_rectangle", name=None, label=None, lstyle=None, adjust=None,
          outline=None, decorative=False):
    k = {"type": "shape", "shape": kind, "fill": fill}
    if label is not None:
        k["text"] = {"text": label, "style": lstyle or style(18, "#1F1F1F", True, align="center", valign="middle")}
    if adjust:
        k["adjust"] = adjust
    if outline:
        k["outline"] = outline
    o = obj(oid, name or oid, k, frame)
    if decorative:
        o["decorative"] = True
    return o


def button(oid, label, frame, fill, hover, color=None, name=None, shadow=None, size=None):
    """A button. Its label colour is picked by the player for contrast with `fill` (color and size are ignored)."""
    k = {"type": "button", "label": label, "fill": fill}
    o = obj(oid, name or f"{label.strip(' →')} button", k, frame)
    o["states"] = [{"name": "hover", "fill": hover}]
    if shadow:
        o["shadow"] = shadow
    return o


def soft_button(oid, label, frame, fill, hover, color, size=17, name=None, radius=0.2, shadow=None):
    """A button drawn as a labelled shape: use it where the label must be dark (button labels are always white)."""
    o = shape(oid, frame, fill, name=name or f"{label.strip(' →')} button", label=label, adjust=[radius],
              lstyle=style(size, color, True, align="center", valign="middle"))
    o["states"] = [{"name": "hover", "fill": hover}]
    o["description"] = label.strip(" →")
    if shadow:
        o["shadow"] = shadow
    return o


def image(oid, src, alt, frame, crop=None, name=None, mask=None):
    k = {"type": "image", "src": src, "alt": alt}
    if mask:
        k["shape"] = mask
    if crop:
        k["crop"] = crop
    return obj(oid, name or oid, k, frame)


def crop_for(frame, x=0.5, y=0.5, src_ratio=1376 / 768):
    """A crop that fills `frame` from a picture of `src_ratio` without stretching, centred on (x, y)."""
    r = frame["w"] / frame["h"]
    if r < src_ratio:
        w = r / src_ratio
        return {"x": round(min(max(x - w / 2, 0), 1 - w), 4), "y": 0.0, "w": round(w, 4), "h": 1.0}
    h = src_ratio / r
    return {"x": 0.0, "y": round(min(max(y - h / 2, 0), 1 - h), 4), "w": 1.0, "h": round(h, 4)}


def picture(oid, src, alt, frame, x=0.5, y=0.5, mask="rounded_rectangle", name=None, src_ratio=1376 / 768):
    """A picture cropped to its frame (never stretched), in a rounded or other shape mask."""
    return image(oid, src, alt, frame, crop=crop_for(frame, x, y, src_ratio), name=name or "Picture", mask=mask)


def set_label(sid, oid, s, **st):
    o = find(sid, oid)
    k = o["kind"]
    if k["type"] == "text":
        k["text"] = s
        k.pop("runs", None)
        k["style"].update(st)
    elif k["type"] == "shape":
        k.setdefault("text", {"text": "", "style": style(18)})
        k["text"]["text"] = s
        k["text"].pop("runs", None)
        k["text"]["style"].update(st)
    elif k["type"] == "button":
        k["label"] = s
    return o


# ------------------------------------------------------------------ motion
def ent(o, effect, ms=600, start=None, easing="ease_out", seq=None):
    o["entrance"] = {"effect": effect, "duration_ms": ms, "easing": easing}
    if seq:
        o["entrance"]["sequence"] = seq
    if start is not None:
        o.setdefault("timing", {})["start_ms"] = start
    return o


def emph(o, effect, start=0, ms=1000, repeat=1, easing=None):
    e = {"effect": {"type": effect}, "start_ms": start, "duration_ms": ms}
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
BOUNCE = {"type": "bounce"}
SPIN_GROW = {"type": "spin_grow"}
def fly(d="left"): return {"type": "fly_in", "from": d}
def wipe(d="left"): return {"type": "wipe", "from": d}
def floaty(d="bottom"): return {"type": "float", "from": d}
def reveal_shape(s="box", io="out"): return {"type": "shape", "shape": s, "inout": io}
def flip(o="vertical"): return {"type": "flip", "orientation": o}
def wheel(n=1): return {"type": "wheel", "spokes": n}


def T(kind, ms=600, **opts):
    t = {"type": kind, "duration_ms": ms}
    t.update(opts)
    return t


# ------------------------------------------------------------------ triggers and layers
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


def new_layer(sid, lid, name, **opts):
    l = {"id": lid, "name": name, "duration_ms": opts.pop("duration_ms", 5000), "objects": [],
         "hide_other_layers": opts.pop("hide_other_layers", True),
         "pause_base_timeline": opts.pop("pause_base_timeline", False),
         "prevent_base_clicks": opts.pop("prevent_base_clicks", False)}
    l.update(opts)
    slide(sid).setdefault("layers", []).append(l)
    return l


def var(name, value):
    C.setdefault("variables", [])
    C["variables"] = [v for v in C["variables"] if v["name"] != name] + [{"name": name, "value": value}]


# ------------------------------------------------------------------ finishing
def fit_emphasis():
    """Fit every emphasis into its object's time window (the validator warns when one is cut off)."""
    def fit(objects, timeline):
        for o in objects:
            t = o.get("timing", {})
            start = t.get("start_ms", 0)
            end = min(t.get("end_ms", timeline), timeline)
            keep = []
            for e in o.get("emphasis", []):
                e["start_ms"] = max(e.get("start_ms", 0), start)
                avail = end - e["start_ms"]
                dur = e.get("duration_ms", 1000)
                if avail < 200:
                    continue
                passes = min(e.get("repeat", 1), avail // dur)
                if passes >= 1:
                    e["repeat"] = passes
                else:
                    e["repeat"], e["duration_ms"] = 1, avail
                if e["repeat"] == 1:
                    e.pop("repeat")
                keep.append(e)
            if "emphasis" in o:
                o["emphasis"] = keep
    for s in slides():
        dur = s.get("duration_ms", 5000)
        fit(s.get("objects", []), dur)
        for l in s.get("layers", []):
            fit(l.get("objects", []), l.get("duration_ms", dur))


def fit_entrances():
    """Make each slide long enough for its last entrance to finish (plus a short hold)."""
    for s in slides():
        last = 0
        for o in s.get("objects", []):
            if o.get("entrance"):
                last = max(last, o.get("timing", {}).get("start_ms", 0) + o["entrance"]["duration_ms"])
        s["duration_ms"] = max(s.get("duration_ms", 5000), last + 400)


def save(path):
    fit_entrances()
    fit_emphasis()
    with open(path, "w") as f:
        json.dump(C, f, indent=1, ensure_ascii=False)
        f.write("\n")


def check(path, inputs=None):
    """Validate (and simulate) the written course; print a one-line summary of each."""
    v = subprocess.run([CLI, "validate", path], capture_output=True, text=True)
    lines = (v.stdout + v.stderr).strip().splitlines()
    for line in lines:
        if "warning" in line.lower() or "error" in line.lower():
            print(line)
    if inputs:
        r = subprocess.run([CLI, "simulate", path, inputs], capture_output=True, text=True)
        tail = [line for line in r.stdout.splitlines() if '"completion"' in line]
        print(tail[-1] if tail else "simulate: no completion\n" + r.stderr[-500:])
