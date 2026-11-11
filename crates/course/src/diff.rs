//! A plain-words difference between two versions of a course.
//!
//! Pure and wasm-safe: no file access and no hashing. Slides, scenes, layers,
//! objects, variables, banks and resources are matched by id, never by
//! position, so inserting a slide does not make every later slide look moved.
//! Media content is compared through hashes the caller supplies.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use serde::Serialize;
use serde_json::Value as Json;

use crate::{Course, Layer, Object, Scene, Slide, Trigger};

/// Text longer than this is reported as "changed" without showing it.
const SHORT: usize = 80;
/// Longest `before` or `after` text kept in a [`Change`].
const KEEP: usize = 2000;
/// Longest name shown in a path or sentence.
const NAME: usize = 60;

/// What happened to the thing a [`Change`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    Renamed,
    Moved,
}

/// One difference between two courses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    /// Where it is, such as `scene "Intro" › slide "Welcome" › object "Title"`.
    pub path: String,
    /// The group a person reads it under: `Course`, `Scene "Intro"`, `Slide "Welcome"`.
    pub scope: String,
    /// The change in plain words, without the scope.
    pub summary: String,
    /// The slide's id when the change is inside a slide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slide: Option<String>,
    /// The old value when there is a single one to show.
    pub before: Option<String>,
    /// The new value when there is a single one to show.
    pub after: Option<String>,
}

impl Change {
    /// The line the CLI prints: `Slide "Welcome": text of "Title" changed ...`.
    #[must_use]
    pub fn sentence(&self) -> String {
        format!("{}: {}", self.scope, self.summary)
    }
}

/// Media file hashes keyed by project-relative path, one map per side.
pub type MediaHashes = BTreeMap<String, u64>;

/// Compare two courses, ignoring media content.
#[must_use]
pub fn diff(a: &Course, b: &Course) -> Vec<Change> {
    diff_with_media(a, b, None)
}

/// Compare two courses and, when both media maps are given, the files too:
/// added, removed, or present on both sides with a different hash.
#[must_use]
pub fn diff_with_media(
    a: &Course,
    b: &Course,
    media: Option<(&MediaHashes, &MediaHashes)>,
) -> Vec<Change> {
    let mut d = Differ::default();
    d.settings(a, b);
    d.keyed(
        "variable",
        &a.variables,
        &b.variables,
        |v| v.name.as_str(),
        |v| v.name.clone(),
    );
    d.keyed(
        "resource",
        &a.resources,
        &b.resources,
        |r| r.id.as_str(),
        |r| r.title.clone(),
    );
    d.keyed(
        "glossary entry",
        &a.glossary,
        &b.glossary,
        |g| g.id.as_str(),
        |g| g.term.clone(),
    );
    d.scenes(a, b);
    d.slides(a, b);
    d.banks(a, b);
    let root = Cx::new("", "Course", None);
    d.slide_group(&root, "master", &a.masters, &b.masters);
    d.slide_group(&root, "template", &a.templates, &b.templates);
    if let Some((ma, mb)) = media {
        d.media(ma, mb);
    }
    d.changes
}

// ------------------------------------------------------------------ helpers

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

fn name_or_id(name: &str, id: &str) -> String {
    let name = name.trim();
    clip(if name.is_empty() { id } else { name }, NAME)
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", clip(text, NAME))
}

fn to_json<T: Serialize>(value: &T) -> Json {
    serde_json::to_value(value).unwrap_or(Json::Null)
}

/// A number the way a person writes it: `80`, not `80.0`.
fn number(n: &serde_json::Number) -> String {
    match n.as_f64() {
        Some(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 => format!("{f:.0}"),
        _ => n.to_string(),
    }
}

fn raw(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Number(n) => number(n),
        other => other.to_string(),
    }
}

fn disp(value: &Json) -> String {
    match value {
        Json::String(s) => format!("\"{s}\""),
        other => raw(other),
    }
}

fn is_short(text: &str) -> bool {
    text.chars().count() <= SHORT
}

fn humanize(key: &str) -> String {
    key.replace(['_', '.'], " ")
}

fn join(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_owned()
    } else {
        format!("{path} › {segment}")
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// "what changed from X to Y", showing the values only when they are short.
fn words_s(what: &str, before: Option<&str>, after: Option<&str>) -> String {
    match (before, after) {
        (Some(b), Some(a)) if is_short(b) && is_short(a) => {
            format!("{what} changed from {b} to {a}")
        }
        (None, Some(a)) if is_short(a) => format!("{what} set to {a}"),
        (None, Some(_)) => format!("{what} added"),
        (Some(b), None) if is_short(b) => format!("{what} cleared (was {b})"),
        (Some(_), None) => format!("{what} removed"),
        _ => format!("{what} changed"),
    }
}

fn words(what: &str, before: Option<&Json>, after: Option<&Json>) -> String {
    let b = before.map(disp);
    let a = after.map(disp);
    words_s(what, b.as_deref(), a.as_deref())
}

type Leaf = (String, Option<Json>, Option<Json>);

fn present(value: Option<&Json>) -> Option<&Json> {
    value.filter(|v| !v.is_null())
}

/// Every differing leaf between two JSON values, as dotted keys. Arrays are
/// compared whole. A null counts as absent.
fn walk(prefix: &str, a: Option<&Json>, b: Option<&Json>, out: &mut Vec<Leaf>) {
    let (a, b) = (present(a), present(b));
    if a == b {
        return;
    }
    if let (Some(Json::Object(x)), Some(Json::Object(y))) = (a, b) {
        let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
        for key in keys {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            walk(&path, x.get(key), y.get(key), out);
        }
        return;
    }
    out.push((prefix.to_owned(), a.cloned(), b.cloned()));
}

fn leaves(a: &Json, b: &Json, skip: &[&str]) -> Vec<Leaf> {
    let strip = |v: &Json| {
        let mut v = v.clone();
        if let Json::Object(map) = &mut v {
            for key in skip {
                map.remove(*key);
            }
        }
        v
    };
    let mut out = Vec::new();
    walk("", Some(&strip(a)), Some(&strip(b)), &mut out);
    out
}

/// Ids whose place in `b` is out of order relative to `a`: everything that is
/// not part of the longest run that kept its relative order. Ids missing from
/// either side are ignored.
fn out_of_order<'a>(a: &[&'a str], b: &[&'a str]) -> HashSet<&'a str> {
    let b_pos: HashMap<&str, usize> = b.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let seq: Vec<(usize, &str)> = a
        .iter()
        .filter_map(|id| b_pos.get(id).map(|p| (*p, *id)))
        .collect();
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, &(pos, _)) in seq.iter().enumerate() {
        let k = tails.partition_point(|&t| seq.get(t).is_some_and(|s| s.0 < pos));
        if let (Some(slot), Some(before)) = (prev.get_mut(i), k.checked_sub(1)) {
            *slot = tails.get(before).copied();
        }
        match tails.get_mut(k) {
            Some(tail) => *tail = i,
            None => tails.push(i),
        }
    }
    let mut keep = HashSet::new();
    let mut cursor = tails.last().copied();
    while let Some(i) = cursor {
        keep.insert(i);
        cursor = prev.get(i).copied().flatten();
    }
    seq.iter()
        .enumerate()
        .filter(|(i, _)| !keep.contains(i))
        .map(|(_, &(_, id))| id)
        .collect()
}

// ------------------------------------------------------------------ context

#[derive(Clone)]
struct Cx {
    path: String,
    scope: String,
    slide: Option<String>,
}

impl Cx {
    fn new(path: &str, scope: &str, slide: Option<&str>) -> Self {
        Self {
            path: path.to_owned(),
            scope: scope.to_owned(),
            slide: slide.map(str::to_owned),
        }
    }

    fn child(&self, segment: &str) -> Self {
        Self {
            path: join(&self.path, segment),
            ..self.clone()
        }
    }
}

#[derive(Default)]
struct Differ {
    changes: Vec<Change>,
}

impl Differ {
    fn add(
        &mut self,
        cx: &Cx,
        kind: ChangeKind,
        summary: String,
        before: Option<String>,
        after: Option<String>,
    ) {
        self.changes.push(Change {
            kind,
            path: cx.path.clone(),
            scope: cx.scope.clone(),
            summary,
            slide: cx.slide.clone(),
            before: before.map(|s| clip(&s, KEEP)),
            after: after.map(|s| clip(&s, KEEP)),
        });
    }

    /// One change per differing leaf, worded as "`<prefix> <key>` changed ...".
    fn leaf_changes(&mut self, cx: &Cx, prefix: &str, found: &[Leaf]) {
        for (key, before, after) in found {
            let what = if prefix.is_empty() {
                humanize(key)
            } else {
                format!("{prefix} {}", humanize(key))
            };
            let at = cx.child(&what);
            let summary = words(&what, before.as_ref(), after.as_ref());
            self.add(
                &at,
                ChangeKind::Changed,
                summary,
                before.as_ref().map(raw),
                after.as_ref().map(raw),
            );
        }
    }

    // -------------------------------------------------------------- course

    fn settings(&mut self, a: &Course, b: &Course) {
        let cx = Cx::new("course", "Course", None);
        let found = leaves(&settings_json(a), &settings_json(b), &[]);
        self.leaf_changes(&cx, "", &found);
    }

    /// Items matched by id (variables use their name): added, removed, changed.
    fn keyed<T: Serialize>(
        &mut self,
        noun: &str,
        a: &[T],
        b: &[T],
        id: impl Fn(&T) -> &str,
        name: impl Fn(&T) -> String,
    ) {
        let root = Cx::new("", "Course", None);
        let a_map: HashMap<&str, &T> = a.iter().map(|x| (id(x), x)).collect();
        let b_ids: HashSet<&str> = b.iter().map(&id).collect();
        for item in b {
            let label = format!("{noun} {}", quoted(&name(item)));
            let cx = root.child(&label);
            match a_map.get(id(item)) {
                None => self.add(&cx, ChangeKind::Added, format!("{label} added"), None, None),
                Some(old) => {
                    let found = leaves(&to_json(old), &to_json(item), &[]);
                    self.leaf_changes(&cx, &label, &found);
                }
            }
        }
        for item in a.iter().filter(|x| !b_ids.contains(id(x))) {
            let label = format!("{noun} {}", quoted(&name(item)));
            let cx = root.child(&label);
            self.add(
                &cx,
                ChangeKind::Removed,
                format!("{label} removed"),
                None,
                None,
            );
        }
    }

    fn media(&mut self, a: &MediaHashes, b: &MediaHashes) {
        let root = Cx::new("", "Media", None);
        for (path, hash) in b {
            let label = format!("media file {}", quoted(path));
            let cx = root.child(&label);
            match a.get(path) {
                None => self.add(&cx, ChangeKind::Added, format!("{label} added"), None, None),
                Some(old) if old != hash => {
                    self.add(
                        &cx,
                        ChangeKind::Changed,
                        format!("{label} has different content"),
                        None,
                        None,
                    );
                }
                Some(_) => {}
            }
        }
        for path in a.keys().filter(|p| !b.contains_key(*p)) {
            let label = format!("media file {}", quoted(path));
            let cx = root.child(&label);
            self.add(
                &cx,
                ChangeKind::Removed,
                format!("{label} removed"),
                None,
                None,
            );
        }
    }

    // -------------------------------------------------------------- scenes

    fn scenes(&mut self, a: &Course, b: &Course) {
        let a_map: HashMap<&str, &Scene> = a.scenes.iter().map(|s| (s.id.as_str(), s)).collect();
        let b_ids: HashSet<&str> = b.scenes.iter().map(|s| s.id.as_str()).collect();
        let a_order: Vec<&str> = a.scenes.iter().map(|s| s.id.as_str()).collect();
        let b_order: Vec<&str> = b.scenes.iter().map(|s| s.id.as_str()).collect();
        let moved = out_of_order(&a_order, &b_order);
        for (index, scene) in b.scenes.iter().enumerate() {
            let cx = scene_cx(scene);
            match a_map.get(scene.id.as_str()) {
                None => {
                    let text = format!("scene {} added", quoted(&scene.title));
                    self.add(&cx, ChangeKind::Added, text, None, None);
                }
                Some(old) => {
                    if old.title != scene.title {
                        let text = format!(
                            "renamed from {} to {}",
                            quoted(&old.title),
                            quoted(&scene.title)
                        );
                        self.add(
                            &cx,
                            ChangeKind::Renamed,
                            text,
                            Some(old.title.clone()),
                            Some(scene.title.clone()),
                        );
                    }
                    if moved.contains(scene.id.as_str()) {
                        let text = format!("moved to position {}", index + 1);
                        self.add(&cx, ChangeKind::Moved, text, None, None);
                    }
                }
            }
        }
        for scene in a.scenes.iter().filter(|s| !b_ids.contains(s.id.as_str())) {
            let text = format!(
                "scene {} removed ({} slides)",
                quoted(&scene.title),
                scene.slides.len()
            );
            self.add(&scene_cx(scene), ChangeKind::Removed, text, None, None);
        }
    }

    // -------------------------------------------------------------- slides

    fn slides(&mut self, a: &Course, b: &Course) {
        let a_at = index_slides(a);
        let b_ids: HashSet<&str> = b.slides().map(|s| s.id.as_str()).collect();
        let reordered = reordered_slides(a, b);
        for scene in &b.scenes {
            for (index, slide) in scene.slides.iter().enumerate() {
                let cx = slide_cx(Some(scene), slide);
                let Some(&(old_scene, old)) = a_at.get(slide.id.as_str()) else {
                    let text = format!(
                        "slide {} added to scene {}",
                        quoted(&slide.title),
                        quoted(&scene.title)
                    );
                    self.add(&cx, ChangeKind::Added, text, None, None);
                    continue;
                };
                if old_scene.id != scene.id {
                    let text = format!(
                        "moved from scene {} to scene {}",
                        quoted(&old_scene.title),
                        quoted(&scene.title)
                    );
                    self.add(&cx, ChangeKind::Moved, text, None, None);
                } else if reordered.contains(slide.id.as_str()) {
                    let text = format!(
                        "moved to position {} in scene {}",
                        index + 1,
                        quoted(&scene.title)
                    );
                    self.add(&cx, ChangeKind::Moved, text, None, None);
                }
                if old != slide {
                    self.slide_body(old, slide, &cx);
                }
            }
        }
        for scene in &a.scenes {
            for slide in scene
                .slides
                .iter()
                .filter(|s| !b_ids.contains(s.id.as_str()))
            {
                let cx = slide_cx(Some(scene), slide);
                let text = format!(
                    "slide {} removed from scene {}",
                    quoted(&slide.title),
                    quoted(&scene.title)
                );
                self.add(&cx, ChangeKind::Removed, text, None, None);
            }
        }
    }

    /// Slides of a bank, the masters or the templates, matched by id.
    fn slide_group(&mut self, parent: &Cx, noun: &str, a: &[Slide], b: &[Slide]) {
        let a_map: HashMap<&str, &Slide> = a.iter().map(|s| (s.id.as_str(), s)).collect();
        let b_ids: HashSet<&str> = b.iter().map(|s| s.id.as_str()).collect();
        let a_order: Vec<&str> = a.iter().map(|s| s.id.as_str()).collect();
        let b_order: Vec<&str> = b.iter().map(|s| s.id.as_str()).collect();
        let moved = out_of_order(&a_order, &b_order);
        let cx_for = |slide: &Slide| {
            let scope = format!("{} {}", capitalised(noun), quoted(&slide.title));
            let mut cx = Cx::new(&parent.path, &scope, Some(&slide.id));
            cx.path = join(&parent.path, &format!("{noun} {}", quoted(&slide.title)));
            cx
        };
        for (index, slide) in b.iter().enumerate() {
            let cx = cx_for(slide);
            let Some(old) = a_map.get(slide.id.as_str()) else {
                let text = format!("{noun} {} added", quoted(&slide.title));
                self.add(&cx, ChangeKind::Added, text, None, None);
                continue;
            };
            if moved.contains(slide.id.as_str()) {
                let text = format!("moved to position {}", index + 1);
                self.add(&cx, ChangeKind::Moved, text, None, None);
            }
            if *old != slide {
                self.slide_body(old, slide, &cx);
            }
        }
        for slide in a.iter().filter(|s| !b_ids.contains(s.id.as_str())) {
            let text = format!("{noun} {} removed", quoted(&slide.title));
            self.add(&cx_for(slide), ChangeKind::Removed, text, None, None);
        }
    }

    fn banks(&mut self, a: &Course, b: &Course) {
        let a_map: HashMap<&str, &crate::QuestionBank> =
            a.banks.iter().map(|x| (x.id.as_str(), x)).collect();
        let b_ids: HashSet<&str> = b.banks.iter().map(|x| x.id.as_str()).collect();
        for bank in &b.banks {
            let cx = Cx::new(
                &format!("bank {}", quoted(&bank.title)),
                &format!("Bank {}", quoted(&bank.title)),
                None,
            );
            match a_map.get(bank.id.as_str()) {
                None => {
                    let text = format!("question bank {} added", quoted(&bank.title));
                    self.add(&cx, ChangeKind::Added, text, None, None);
                }
                Some(old) => {
                    if old.title != bank.title {
                        let text = format!(
                            "renamed from {} to {}",
                            quoted(&old.title),
                            quoted(&bank.title)
                        );
                        self.add(
                            &cx,
                            ChangeKind::Renamed,
                            text,
                            Some(old.title.clone()),
                            Some(bank.title.clone()),
                        );
                    }
                    self.slide_group(&cx, "slide", &old.slides, &bank.slides);
                }
            }
        }
        for bank in a.banks.iter().filter(|x| !b_ids.contains(x.id.as_str())) {
            let cx = Cx::new(
                &format!("bank {}", quoted(&bank.title)),
                &format!("Bank {}", quoted(&bank.title)),
                None,
            );
            let text = format!("question bank {} removed", quoted(&bank.title));
            self.add(&cx, ChangeKind::Removed, text, None, None);
        }
    }

    /// What differs inside a slide that exists on both sides.
    fn slide_body(&mut self, a: &Slide, b: &Slide, cx: &Cx) {
        if a.title != b.title {
            let text = format!("renamed from {} to {}", quoted(&a.title), quoted(&b.title));
            self.add(
                cx,
                ChangeKind::Renamed,
                text,
                Some(a.title.clone()),
                Some(b.title.clone()),
            );
        }
        let skip = ["id", "title", "objects", "layers", "triggers", "question"];
        let found = leaves(&to_json(a), &to_json(b), &skip);
        self.leaf_changes(cx, "", &found);
        self.question(a, b, cx);
        self.triggers(&a.triggers, &b.triggers, cx, None);
        self.layers(a, b, cx);
        self.objects(a, b, cx);
    }

    fn question(&mut self, a: &Slide, b: &Slide, cx: &Cx) {
        match (&a.question, &b.question) {
            (None, Some(_)) => {
                let at = cx.child("question");
                self.add(
                    &at,
                    ChangeKind::Added,
                    "question added".to_owned(),
                    None,
                    None,
                );
            }
            (Some(_), None) => {
                let at = cx.child("question");
                self.add(
                    &at,
                    ChangeKind::Removed,
                    "question removed".to_owned(),
                    None,
                    None,
                );
            }
            (Some(x), Some(y)) if x != y => {
                let found = leaves(&to_json(x), &to_json(y), &[]);
                self.leaf_changes(cx, "question", &found);
            }
            _ => {}
        }
    }

    // -------------------------------------------------------------- layers

    fn layers(&mut self, a: &Slide, b: &Slide, cx: &Cx) {
        let a_map: HashMap<&str, &Layer> = a.layers.iter().map(|l| (l.id.as_str(), l)).collect();
        let b_ids: HashSet<&str> = b.layers.iter().map(|l| l.id.as_str()).collect();
        let a_order: Vec<&str> = a.layers.iter().map(|l| l.id.as_str()).collect();
        let b_order: Vec<&str> = b.layers.iter().map(|l| l.id.as_str()).collect();
        let moved = out_of_order(&a_order, &b_order);
        for layer in &b.layers {
            let label = format!("layer {}", quoted(&name_or_id(&layer.name, &layer.id)));
            let at = cx.child(&label);
            let Some(old) = a_map.get(layer.id.as_str()) else {
                self.add(&at, ChangeKind::Added, format!("{label} added"), None, None);
                continue;
            };
            if old.name != layer.name {
                let text = format!(
                    "{label} renamed from {}",
                    quoted(&name_or_id(&old.name, &old.id))
                );
                self.add(
                    &at,
                    ChangeKind::Renamed,
                    text,
                    Some(old.name.clone()),
                    Some(layer.name.clone()),
                );
            }
            if moved.contains(layer.id.as_str()) {
                let text = format!("{label} moved in stacking order");
                self.add(&at, ChangeKind::Moved, text, None, None);
            }
            let found = leaves(
                &to_json(*old),
                &to_json(layer),
                &["id", "name", "objects", "triggers"],
            );
            self.leaf_changes(&at, &label, &found);
            self.triggers(&old.triggers, &layer.triggers, cx, Some(&label));
        }
        for layer in a.layers.iter().filter(|l| !b_ids.contains(l.id.as_str())) {
            let label = format!("layer {}", quoted(&name_or_id(&layer.name, &layer.id)));
            let at = cx.child(&label);
            self.add(
                &at,
                ChangeKind::Removed,
                format!("{label} removed"),
                None,
                None,
            );
        }
    }

    // ------------------------------------------------------------ triggers

    /// Triggers have no id, so identical ones are matched first, then ones
    /// with the same "when" are taken as an edit of that trigger.
    fn triggers(&mut self, a: &[Trigger], b: &[Trigger], cx: &Cx, layer: Option<&str>) {
        let ka: Vec<String> = a.iter().map(|t| to_json(t).to_string()).collect();
        let mut exact: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, k) in ka.iter().enumerate() {
            exact.entry(k.as_str()).or_default().push(i);
        }
        let mut a_used = vec![false; a.len()];
        let mut b_left: Vec<&Trigger> = Vec::new();
        for t in b {
            let key = to_json(t).to_string();
            match exact.get_mut(key.as_str()).and_then(Vec::pop) {
                Some(i) => {
                    if let Some(slot) = a_used.get_mut(i) {
                        *slot = true;
                    }
                }
                None => b_left.push(t),
            }
        }
        let mut by_when: HashMap<String, VecDeque<usize>> = HashMap::new();
        for (i, (t, used)) in a.iter().zip(&a_used).enumerate() {
            if !used {
                by_when
                    .entry(to_json(&t.when).to_string())
                    .or_default()
                    .push_back(i);
            }
        }
        for new in b_left {
            let key = to_json(&new.when).to_string();
            let taken = by_when.get_mut(&key).and_then(VecDeque::pop_front);
            match taken.and_then(|i| a.get(i).map(|old| (i, old))) {
                Some((i, old)) => {
                    if let Some(slot) = a_used.get_mut(i) {
                        *slot = true;
                    }
                    self.trigger_edit(old, new, cx, layer);
                }
                None => self.trigger_note(new, cx, layer, true),
            }
        }
        for (old, used) in a.iter().zip(&a_used) {
            if !used {
                self.trigger_note(old, cx, layer, false);
            }
        }
    }

    fn trigger_note(&mut self, t: &Trigger, cx: &Cx, layer: Option<&str>, added: bool) {
        let label = trigger_label(t);
        let at = cx.child(&label);
        let place = layer.map_or(String::new(), |l| {
            format!(" {} {l}", if added { "in" } else { "from" })
        });
        let (kind, verb) = if added {
            (ChangeKind::Added, "added")
        } else {
            (ChangeKind::Removed, "removed")
        };
        self.add(&at, kind, format!("{label} {verb}{place}"), None, None);
    }

    fn trigger_edit(&mut self, old: &Trigger, new: &Trigger, cx: &Cx, layer: Option<&str>) {
        let label = trigger_label(new);
        let at = cx.child(&label);
        let place = layer.map_or(String::new(), |l| format!(" in {l}"));
        let mut parts: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        if old.action != new.action {
            let (x, y) = (to_json(&old.action), to_json(&new.action));
            parts.push((
                words_s(
                    "action",
                    Some(&clip_q(&describe(&x))),
                    Some(&clip_q(&describe(&y))),
                ),
                Some(describe(&x)),
                Some(describe(&y)),
            ));
        }
        if old.condition != new.condition {
            parts.push(("condition changed".to_owned(), None, None));
        }
        if old.disabled != new.disabled {
            let text = if new.disabled {
                "turned off"
            } else {
                "turned on"
            };
            parts.push((text.to_owned(), None, None));
        }
        for (text, before, after) in parts {
            self.add(
                &at,
                ChangeKind::Changed,
                format!("{label}{place}: {text}"),
                before,
                after,
            );
        }
    }

    // ------------------------------------------------------------- objects

    fn objects(&mut self, a: &Slide, b: &Slide, cx: &Cx) {
        let (a_all, b_all) = (placed(a), placed(b));
        let a_map: HashMap<&str, &Placed> = a_all.iter().map(|p| (p.obj.id.as_str(), p)).collect();
        let b_ids: HashSet<&str> = b_all.iter().map(|p| p.obj.id.as_str()).collect();
        let stacked = restacked(&a_all, &b_all);
        for new in &b_all {
            let at = cx.child(&new.path_segment());
            let label = new.label();
            let Some(old) = a_map.get(new.obj.id.as_str()) else {
                let text = format!(
                    "{label} ({}) added{}",
                    object_type(new.obj),
                    new.place("to")
                );
                self.add(&at, ChangeKind::Added, text, None, None);
                continue;
            };
            if old.layer_id() != new.layer_id() {
                let text = format!(
                    "{label} moved from {} to {}",
                    old.where_text(),
                    new.where_text()
                );
                self.add(&at, ChangeKind::Moved, text, None, None);
            } else if stacked.contains(new.obj.id.as_str()) {
                let text = format!("{label} moved in stacking order");
                self.add(&at, ChangeKind::Moved, text, None, None);
            }
            if old.obj != new.obj {
                self.object_body(old.obj, new.obj, &at);
            }
        }
        for old in a_all.iter().filter(|p| !b_ids.contains(p.obj.id.as_str())) {
            let at = cx.child(&old.path_segment());
            let text = format!(
                "{} ({}) removed{}",
                old.label(),
                object_type(old.obj),
                old.place("from")
            );
            self.add(&at, ChangeKind::Removed, text, None, None);
        }
    }

    fn object_body(&mut self, a: &Object, b: &Object, at: &Cx) {
        let label = format!("object {}", quoted(&name_or_id(&b.name, &b.id)));
        if a.name != b.name {
            let text = format!(
                "{label} renamed from {}",
                quoted(&name_or_id(&a.name, &a.id))
            );
            self.add(
                at,
                ChangeKind::Renamed,
                text,
                Some(a.name.clone()),
                Some(b.name.clone()),
            );
        }
        let (ja, jb) = (to_json(a), to_json(b));
        for (part, found) in parts(&leaves(&ja, &jb, &["id", "name"])) {
            let part = single_key_name(part, &found).unwrap_or(part.to_owned());
            let what = format!("{part} of {}", quoted(&name_or_id(&b.name, &b.id)));
            let (before, after) = part_values(&part, &found, &ja, &jb);
            let text = words_s(&what, before.as_deref(), after.as_deref());
            let shown = |v: &Option<String>| v.as_deref().map(unquote);
            self.add(at, ChangeKind::Changed, text, shown(&before), shown(&after));
        }
    }
}

// ------------------------------------------------------------ course pieces

fn settings_json(course: &Course) -> Json {
    let mut map = serde_json::Map::new();
    let mut put = |key: &str, value: Json| {
        map.insert(key.to_owned(), value);
    };
    put("id", Json::String(course.id.clone()));
    put("title", Json::String(course.title.clone()));
    put("schema_version", Json::from(course.schema_version));
    put("slide_size", to_json(&course.slide_size));
    put("start_slide", to_json(&course.start_slide));
    put("quiz", to_json(&course.quiz));
    put("player", to_json(&course.player));
    put("theme", to_json(&course.theme));
    put("language", to_json(&course.language));
    put("strings", to_json(&course.strings));
    put("fonts", to_json(&course.fonts));
    put("background_audio", to_json(&course.background_audio));
    Json::Object(map)
}

fn scene_cx(scene: &Scene) -> Cx {
    Cx::new(
        &format!("scene {}", quoted(&scene.title)),
        &format!("Scene {}", quoted(&scene.title)),
        None,
    )
}

fn slide_cx(scene: Option<&Scene>, slide: &Slide) -> Cx {
    let path = format!("slide {}", quoted(&slide.title));
    let path = match scene {
        Some(s) => join(&format!("scene {}", quoted(&s.title)), &path),
        None => path,
    };
    Cx::new(
        &path,
        &format!("Slide {}", quoted(&slide.title)),
        Some(&slide.id),
    )
}

fn index_slides(course: &Course) -> HashMap<&str, (&Scene, &Slide)> {
    let mut map = HashMap::new();
    for scene in &course.scenes {
        for slide in &scene.slides {
            map.entry(slide.id.as_str()).or_insert((scene, slide));
        }
    }
    map
}

/// Slides that stayed in their scene but whose place in it changed.
fn reordered_slides<'a>(a: &'a Course, b: &'a Course) -> HashSet<&'a str> {
    let mut out = HashSet::new();
    for new in &b.scenes {
        let Some(old) = a.scenes.iter().find(|s| s.id == new.id) else {
            continue;
        };
        let in_new: HashSet<&str> = new.slides.iter().map(|s| s.id.as_str()).collect();
        let in_old: HashSet<&str> = old.slides.iter().map(|s| s.id.as_str()).collect();
        let a_ids: Vec<&str> = old
            .slides
            .iter()
            .map(|s| s.id.as_str())
            .filter(|id| in_new.contains(id))
            .collect();
        let b_ids: Vec<&str> = new
            .slides
            .iter()
            .map(|s| s.id.as_str())
            .filter(|id| in_old.contains(id))
            .collect();
        out.extend(out_of_order(&a_ids, &b_ids));
    }
    out
}

// ----------------------------------------------------------------- triggers

fn describe(value: &Json) -> String {
    let Json::Object(map) = value else {
        return disp(value);
    };
    let tag = map
        .get("event")
        .or_else(|| map.get("action"))
        .and_then(Json::as_str)
        .unwrap_or("?")
        .replace('_', " ");
    let mut text = tag.clone();
    for (key, val) in map {
        let key = key.replace('_', " ");
        if key == "event" || key == "action" || val.is_null() {
            continue;
        }
        if !tag.ends_with(&key) {
            text.push_str(&format!(" {key}"));
        }
        text.push_str(&format!(" {}", raw(val)));
    }
    text
}

fn clip_q(text: &str) -> String {
    format!("\"{}\"", clip(text, SHORT))
}

fn trigger_label(t: &Trigger) -> String {
    let text = format!(
        "when {}, {}",
        describe(&to_json(&t.when)),
        describe(&to_json(&t.action))
    );
    format!("trigger \"{}\"", clip(&text, 100))
}

// ------------------------------------------------------------------ objects

struct Placed<'a> {
    obj: &'a Object,
    layer: Option<&'a Layer>,
}

impl Placed<'_> {
    fn layer_id(&self) -> Option<&str> {
        self.layer.map(|l| l.id.as_str())
    }

    fn label(&self) -> String {
        format!(
            "object {}",
            quoted(&name_or_id(&self.obj.name, &self.obj.id))
        )
    }

    fn path_segment(&self) -> String {
        let object = self.label();
        match self.layer {
            Some(l) => format!("layer {} › {object}", quoted(&name_or_id(&l.name, &l.id))),
            None => object,
        }
    }

    fn where_text(&self) -> String {
        match self.layer {
            Some(l) => format!("layer {}", quoted(&name_or_id(&l.name, &l.id))),
            None => "the base layer".to_owned(),
        }
    }

    fn place(&self, preposition: &str) -> String {
        self.layer.map_or(String::new(), |l| {
            format!(
                " {preposition} layer {}",
                quoted(&name_or_id(&l.name, &l.id))
            )
        })
    }
}

fn placed(slide: &Slide) -> Vec<Placed<'_>> {
    let base = slide.objects.iter().map(|obj| Placed { obj, layer: None });
    let layers = slide.layers.iter().flat_map(|layer| {
        layer.objects.iter().map(move |obj| Placed {
            obj,
            layer: Some(layer),
        })
    });
    base.chain(layers).collect()
}

/// Objects that stayed on their layer but changed their place in its stack.
fn restacked<'a>(a: &[Placed<'a>], b: &[Placed<'a>]) -> HashSet<&'a str> {
    let layers: BTreeSet<Option<&str>> = b.iter().map(Placed::layer_id).collect();
    let mut out = HashSet::new();
    for layer in layers {
        let in_a = ids_on(a, layer, None);
        let in_b = ids_on(b, layer, None);
        let a_set: HashSet<&str> = in_a.iter().copied().collect();
        let b_set: HashSet<&str> = in_b.iter().copied().collect();
        let a_ids = ids_on(a, layer, Some(&b_set));
        let b_ids = ids_on(b, layer, Some(&a_set));
        out.extend(out_of_order(&a_ids, &b_ids));
    }
    out
}

fn ids_on<'a>(
    all: &[Placed<'a>],
    layer: Option<&str>,
    keep: Option<&HashSet<&str>>,
) -> Vec<&'a str> {
    all.iter()
        .filter(|p| p.layer_id() == layer)
        .map(|p| p.obj.id.as_str())
        .filter(|id| keep.is_none_or(|k| k.contains(id)))
        .collect()
}

fn object_type(obj: &Object) -> String {
    to_json(&obj.kind)
        .get("type")
        .and_then(Json::as_str)
        .unwrap_or("object")
        .replace('_', " ")
}

/// The part of an object a dotted key belongs to, in plain words.
fn part_of(key: &str) -> &'static str {
    let mut segments = key.split('.');
    let first = segments.next().unwrap_or("");
    let second = segments.next().unwrap_or("");
    match first {
        "frame" => match second {
            "x" | "y" => "position",
            "w" | "h" => "size",
            _ => "position or size",
        },
        "rotation" | "flip_h" | "flip_v" => "rotation or flip",
        "opacity" | "shadow" | "glow" => "style",
        "states" | "initial_state" => "states",
        "description" | "decorative" | "focus_order" => "accessibility text",
        "timing" | "entrance" | "exit" | "motion_paths" => "timing or animation",
        "kind" => kind_part(second),
        _ => "settings",
    }
}

fn kind_part(key: &str) -> &'static str {
    match key {
        "type" => "type",
        "text" | "runs" | "paragraphs" | "label" | "placeholder" | "body" | "title" | "options"
        | "rows" => "text",
        "fill" | "fill_style" => "fill",
        "alt" => "accessibility text",
        "src" | "captions" | "trim" | "cues" | "volume" | "autoplay" | "crop" | "source"
        | "loop" | "speed" => "media",
        "style" | "autofit" | "outline" | "shape" | "adjust" | "columns" | "borders"
        | "header_row" | "color" | "icon" | "format" | "bar" => "style",
        _ => "settings",
    }
}

/// Group differing leaves by part, in the order the parts first appear. When
/// the object's type changed, that is the only thing worth saying about it.
fn parts(found: &[Leaf]) -> Vec<(&'static str, Vec<&Leaf>)> {
    let type_changed = found.iter().any(|(key, _, _)| key == "kind.type");
    let mut out: Vec<(&'static str, Vec<&Leaf>)> = Vec::new();
    for leaf in found {
        let part = part_of(&leaf.0);
        if type_changed && leaf.0.starts_with("kind.") && leaf.0 != "kind.type" {
            continue;
        }
        match out.iter_mut().find(|(p, _)| *p == part) {
            Some((_, list)) => list.push(leaf),
            None => out.push((part, vec![leaf])),
        }
    }
    out
}

/// For the loose parts, a lone differing key says more than the part's name.
fn single_key_name(part: &str, found: &[&Leaf]) -> Option<String> {
    let loose = ["style", "settings", "timing or animation", "states"];
    match found {
        [(key, _, _)] if loose.contains(&part) => {
            Some(humanize(key.strip_prefix("kind.").unwrap_or(key)))
        }
        _ => None,
    }
}

fn unquote(text: &str) -> String {
    let inner = text.strip_prefix('"').and_then(|t| t.strip_suffix('"'));
    inner.unwrap_or(text).to_owned()
}

/// Old and new values worth showing for a part, already formatted.
fn part_values(
    part: &str,
    found: &[&Leaf],
    a: &Json,
    b: &Json,
) -> (Option<String>, Option<String>) {
    let pair = |json: &Json, keys: [&str; 2], sep: &str| {
        let frame = json.get("frame")?;
        let get = |k: &str| frame.get(k).map(raw);
        Some(format!("{}{sep}{}", get(keys[0])?, get(keys[1])?))
    };
    match part {
        "position" => (
            pair(a, ["x", "y"], ", ").map(|t| format!("({t})")),
            pair(b, ["x", "y"], ", ").map(|t| format!("({t})")),
        ),
        "size" => (pair(a, ["w", "h"], " × "), pair(b, ["w", "h"], " × ")),
        _ => match found {
            [(_, before, after)] => (before.as_ref().map(disp), after.as_ref().map(disp)),
            _ => (None, None),
        },
    }
}

#[cfg(test)]
mod tests;
