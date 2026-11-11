//! Editor selection model and repair.

use coursecraft_course::Course;

/// Current selection in the editor.
///
/// `object` is the primary object. `also` holds any further selected objects on
/// the same slide and layer, in selection order. Everything that edits one
/// object keeps working on the primary.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Selected slide id.
    pub slide: String,
    /// Focused layer id (`None` means the base layer).
    pub layer: Option<String>,
    /// Primary selected object id (`None` means no object is selected).
    pub object: Option<String>,
    /// Extra selected objects beyond the primary, same slide and layer.
    pub also: Vec<String>,
    /// Slide/layer seen by the last `repair`, so a scope change clears extras.
    pub(crate) scope: Option<(String, Option<String>)>,
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        self.slide == other.slide
            && self.layer == other.layer
            && self.object == other.object
            && self.also == other.also
    }
}

impl Eq for Selection {}

impl Selection {
    /// Create a new selection on the given slide.
    #[must_use]
    pub fn new(slide: impl Into<String>) -> Self {
        Self {
            slide: slide.into(),
            layer: None,
            object: None,
            also: Vec::new(),
            scope: None,
        }
    }

    /// Every selected object id, primary first, without duplicates.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(ref primary) = self.object {
            out.push(primary.clone());
        }
        for id in &self.also {
            if !out.contains(id) {
                out.push(id.clone());
            }
        }
        out
    }

    /// Whether `id` is part of the selection.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.object.as_deref() == Some(id) || self.also.iter().any(|other| other == id)
    }

    /// Whether more than one object is selected.
    #[must_use]
    pub fn is_multi(&self) -> bool {
        self.object.is_some() && !self.also.is_empty()
    }

    /// Select exactly `ids`: the first becomes primary, the rest are extras.
    pub fn set_many<I, S>(&mut self, ids: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut unique: Vec<String> = Vec::new();
        for id in ids {
            let id = id.as_ref().to_owned();
            if !unique.contains(&id) {
                unique.push(id);
            }
        }
        let mut iter = unique.into_iter();
        self.object = iter.next();
        self.also = iter.collect();
        self.scope = Some((self.slide.clone(), self.layer.clone()));
    }

    /// Add `id` to the selection, or remove it when already selected.
    pub fn toggle(&mut self, id: &str) {
        self.scope = Some((self.slide.clone(), self.layer.clone()));
        if self.object.as_deref() == Some(id) {
            if self.also.is_empty() {
                self.object = None;
            } else {
                self.object = Some(self.also.remove(0));
            }
            return;
        }
        if let Some(pos) = self.also.iter().position(|other| other == id) {
            self.also.remove(pos);
            return;
        }
        if self.object.is_none() {
            self.object = Some(id.to_owned());
        } else {
            self.also.push(id.to_owned());
        }
    }

    /// Objects on the focused layer that share `id`'s group, primary first.
    /// Returns just `id` when it is not grouped.
    #[must_use]
    pub fn group_members(&self, course: &Course, id: &str) -> Vec<String> {
        let Some(slide) = course.slide(&self.slide) else {
            return vec![id.to_owned()];
        };
        let objects: Vec<&coursecraft_course::Object> = match self.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| layer.objects.iter().collect())
                .unwrap_or_default(),
            None => slide.objects.iter().collect(),
        };
        let Some(group) = objects
            .iter()
            .find(|object| object.id == id)
            .and_then(|object| object.group.clone())
        else {
            return vec![id.to_owned()];
        };
        let mut out = vec![id.to_owned()];
        for object in objects {
            if object.id != id && object.group.as_deref() == Some(group.as_str()) {
                out.push(object.id.clone());
            }
        }
        out
    }

    /// Grow the selection so every member of a selected group is included.
    pub fn expand_groups(&mut self, course: &Course) {
        let mut expanded: Vec<String> = Vec::new();
        for id in self.ids() {
            for member in self.group_members(course, &id) {
                if !expanded.contains(&member) {
                    expanded.push(member);
                }
            }
        }
        if expanded != self.ids() {
            self.set_many(expanded);
        }
    }

    /// True when the selection is exactly one complete group.
    #[must_use]
    pub fn is_one_group(&self, course: &Course) -> bool {
        let Some(primary) = self.object.as_deref() else {
            return false;
        };
        let members = self.group_members(course, primary);
        if members.len() < 2 {
            return false;
        }
        let ids = self.ids();
        members.len() == ids.len() && ids.iter().all(|id| members.contains(id))
    }

    /// Clear every selected object, keeping the slide and layer.
    pub fn clear_objects(&mut self) {
        self.object = None;
        self.also.clear();
        self.scope = Some((self.slide.clone(), self.layer.clone()));
    }

    /// Repair selection after a course revision change.
    ///
    /// Rules:
    /// 1. A deleted slide selects the first slide (and clears layer and objects).
    /// 2. A deleted layer goes back to the base layer.
    /// 3. Objects that no longer exist in the active layer are dropped. If the
    ///    primary goes, the first remaining extra becomes primary.
    /// 4. Changing slide or layer clears the extras.
    pub fn repair(&mut self, course: &Course) {
        // 1. Check if the selected slide still exists.
        let slide_exists = course.slide(&self.slide).is_some();
        if !slide_exists {
            if let Some(first) = course.slides().next() {
                self.slide = first.id.clone();
            } else {
                self.slide = String::new();
            }
            self.layer = None;
            self.object = None;
            self.also.clear();
            self.scope = Some((self.slide.clone(), None));
            return;
        }

        let Some(slide) = course.slide(&self.slide) else {
            return;
        };

        // 4a. A different slide or layer since the last repair drops the extras.
        if let Some((ref scope_slide, ref scope_layer)) = self.scope
            && (scope_slide != &self.slide || scope_layer != &self.layer)
        {
            self.also.clear();
        }

        // 2. Check if the focused layer still exists.
        if let Some(ref lid) = self.layer
            && slide.layer(lid).is_none()
        {
            self.layer = None;
            self.also.clear();
        }

        // 3. Keep only objects on the current layer.
        let valid: Vec<String> = match self.layer.as_deref() {
            Some(lid) => slide
                .layer(lid)
                .map(|layer| layer.objects.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default(),
            None => slide.objects.iter().map(|o| o.id.clone()).collect(),
        };

        let mut primary = self
            .object
            .clone()
            .filter(|id| valid.iter().any(|candidate| candidate == id));

        let mut extras: Vec<String> = Vec::new();
        for id in std::mem::take(&mut self.also) {
            if primary.as_deref() == Some(id.as_str()) {
                continue;
            }
            if valid.iter().any(|candidate| candidate == &id) && !extras.contains(&id) {
                extras.push(id);
            }
        }

        if primary.is_none()
            && let Some(first) = extras.first().cloned()
        {
            primary = Some(first);
            extras.remove(0);
        }

        self.object = primary;
        self.also = extras;
        self.scope = Some((self.slide.clone(), self.layer.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coursecraft_course::{Object, Rect, Timing};
    use coursecraft_engine::Session;

    fn object(id: &str) -> Object {
        Object {
            id: id.into(),
            name: id.into(),
            kind: coursecraft_course::ObjectKind::Button {
                label: id.into(),
                fill: coursecraft_course::Color("#2F6FEB".into()),
                fill_style: None,
            },
            frame: Rect {
                x: 10.0,
                y: 10.0,
                w: 100.0,
                h: 50.0,
            },
            timing: Timing::default(),
            entrance: None,
            exit: None,
            motion_paths: Vec::new(),
            states: Vec::new(),
            initial_state: "normal".into(),
            focus_order: None,
            decorative: false,
            description: None,
            group: None,
            panel: None,
            rotation: 0.0,
            flip_h: false,
            flip_v: false,
            opacity: 1.0,
            shadow: None,
            glow: None,
            drag: None,
        }
    }

    #[test]
    fn repair_selects_first_slide_when_current_slide_deleted() {
        let mut session = Session::blank("Test Course");
        // Add a second slide
        let res = session
            .execute(
                "slide.add",
                &serde_json::json!({ "id": "slide2", "after": "slide1" }),
            )
            .expect("add slide");
        assert_eq!(res, serde_json::json!({ "id": "slide2" }));

        let mut sel = Selection {
            slide: "slide2".into(),
            layer: None,
            object: None,
            ..Default::default()
        };

        // Delete slide2
        session
            .execute("slide.delete", &serde_json::json!({ "slide": "slide2" }))
            .expect("delete slide2");

        sel.repair(&session.course);
        assert_eq!(sel.slide, "slide1");
        assert_eq!(sel.layer, None);
        assert_eq!(sel.object, None);
        assert!(sel.also.is_empty());
    }

    #[test]
    fn repair_resets_layer_to_base_when_layer_deleted() {
        let mut session = Session::blank("Test Course");
        session
            .execute(
                "layer.add",
                &serde_json::json!({ "slide": "slide1", "id": "layer1" }),
            )
            .expect("add layer");

        let mut sel = Selection {
            slide: "slide1".into(),
            layer: Some("layer1".into()),
            object: None,
            ..Default::default()
        };

        session
            .execute(
                "layer.delete",
                &serde_json::json!({ "slide": "slide1", "layer": "layer1" }),
            )
            .expect("delete layer");

        sel.repair(&session.course);
        assert_eq!(sel.slide, "slide1");
        assert_eq!(sel.layer, None);
    }

    #[test]
    fn repair_clears_object_when_object_deleted() {
        let mut session = Session::blank("Test Course");
        session
            .course
            .slide_mut("slide1")
            .expect("slide1")
            .objects
            .push(object("btn1"));

        let mut sel = Selection {
            slide: "slide1".into(),
            layer: None,
            object: Some("btn1".into()),
            ..Default::default()
        };

        session
            .execute(
                "object.delete",
                &serde_json::json!({ "slide": "slide1", "object": "btn1" }),
            )
            .expect("delete object");

        sel.repair(&session.course);
        assert_eq!(sel.object, None);
    }

    #[test]
    fn ids_are_primary_first_and_unique() {
        let mut sel = Selection::new("s");
        sel.set_many(["a", "b", "a", "c"]);
        assert_eq!(sel.ids(), vec!["a", "b", "c"]);
        assert!(sel.is_multi());
        assert!(sel.contains("b"));
        assert!(!sel.contains("z"));
    }

    #[test]
    fn toggle_adds_and_removes_and_promotes() {
        let mut sel = Selection::new("s");
        sel.toggle("a");
        assert_eq!(sel.object.as_deref(), Some("a"));
        sel.toggle("b");
        assert_eq!(sel.ids(), vec!["a", "b"]);
        sel.toggle("a");
        assert_eq!(sel.object.as_deref(), Some("b"));
        assert!(sel.also.is_empty());
        sel.toggle("b");
        assert_eq!(sel.object, None);
        assert!(sel.ids().is_empty());
    }

    #[test]
    fn repair_drops_extras_and_promotes_when_primary_goes() {
        let mut session = Session::blank("Test Course");
        let slide = session.course.slide_mut("slide1").expect("slide1");
        slide.objects.push(object("a"));
        slide.objects.push(object("b"));
        slide.objects.push(object("c"));

        let mut sel = Selection::new("slide1");
        sel.set_many(["a", "b", "c"]);
        sel.repair(&session.course);
        assert_eq!(sel.ids(), vec!["a", "b", "c"]);

        // Delete the primary; the first remaining extra becomes primary.
        session
            .execute(
                "object.delete",
                &serde_json::json!({ "slide": "slide1", "object": "a" }),
            )
            .expect("delete a");
        sel.repair(&session.course);
        assert_eq!(sel.ids(), vec!["b", "c"]);
        assert_eq!(sel.object.as_deref(), Some("b"));
    }

    #[test]
    fn repair_drops_ids_from_another_layer_and_clears_on_scope_change() {
        let mut session = Session::blank("Test Course");
        session
            .execute(
                "layer.add",
                &serde_json::json!({ "slide": "slide1", "id": "layer1" }),
            )
            .expect("add layer");
        session
            .course
            .slide_mut("slide1")
            .expect("slide1")
            .objects
            .push(object("base"));
        session
            .course
            .slide_mut("slide1")
            .and_then(|slide| slide.layer_mut("layer1"))
            .expect("layer1")
            .objects
            .push(object("layered"));

        // With the layer focused, base ids are dropped.
        let mut sel = Selection::new("slide1");
        sel.layer = Some("layer1".into());
        sel.set_many(["base", "layered"]);
        sel.repair(&session.course);
        assert_eq!(sel.ids(), vec!["layered"]);

        // Changing slide clears the extras even when the ids exist there.
        session
            .course
            .slide_mut("slide1")
            .expect("slide1")
            .objects
            .push(object("base2"));
        let mut sel = Selection::new("slide1");
        sel.set_many(["base", "base2"]);
        sel.repair(&session.course);
        assert_eq!(sel.ids(), vec!["base", "base2"]);

        session
            .execute("slide.add", &serde_json::json!({ "id": "slide2" }))
            .expect("add slide2");
        let slide2 = session.course.slide_mut("slide2").expect("slide2");
        slide2.objects.push(object("base"));
        slide2.objects.push(object("base2"));
        sel.slide = "slide2".into();
        sel.layer = None;
        sel.repair(&session.course);
        assert_eq!(sel.ids(), vec!["base"]);
    }

    #[test]
    fn clear_objects_keeps_slide_and_layer() {
        let mut sel = Selection::new("slide1");
        sel.layer = Some("layer1".into());
        sel.set_many(["a", "b"]);
        sel.clear_objects();
        assert_eq!(sel.slide, "slide1");
        assert_eq!(sel.layer.as_deref(), Some("layer1"));
        assert!(sel.ids().is_empty());
    }
}
