//! Pure maths for the canvas crop mode.
//!
//! Cropping an image keeps the kept pixels where they are on the slide and at
//! the same scale: the frame shrinks (or grows) exactly as the kept window of
//! the source changes. Everything here is plain arithmetic on slide
//! coordinates, so it can be unit-tested without a running editor.
//!
//! The laws, per axis, with `frame` measured in slide pixels and `crop` in
//! fractions of the source of natural size `source_px`:
//!
//! - scale `s = frame / (crop * source_px)`
//! - crop -> frame: `frame' = crop' * source_px * s` (anchored on the edge
//!   opposite the one that moved, so the kept picture does not jump)
//! - frame -> crop: `crop' = crop + d / (source_px * s)`

use coursecraft_course::Rect;
use egui::{Rect as ERect, vec2};

/// Smallest kept frame edge, in slide pixels.
pub const MIN_KEPT_PX: f32 = 8.0;

/// The eight crop handles, named by the edge of the kept area they move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CropHandle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl CropHandle {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::Top,
        Self::TopRight,
        Self::Right,
        Self::BottomRight,
        Self::Bottom,
        Self::BottomLeft,
        Self::Left,
    ];

    /// Whether dragging the handle moves the kept area's left edge.
    #[must_use]
    pub fn moves_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::Left | Self::BottomLeft)
    }

    /// Whether dragging the handle moves the kept area's right edge.
    #[must_use]
    pub fn moves_right(self) -> bool {
        matches!(self, Self::TopRight | Self::Right | Self::BottomRight)
    }

    /// Whether dragging the handle moves the kept area's top edge.
    #[must_use]
    pub fn moves_top(self) -> bool {
        matches!(self, Self::TopLeft | Self::Top | Self::TopRight)
    }

    /// Whether dragging the handle moves the kept area's bottom edge.
    #[must_use]
    pub fn moves_bottom(self) -> bool {
        matches!(self, Self::BottomLeft | Self::Bottom | Self::BottomRight)
    }

    /// Whether the handle sits on a corner.
    #[must_use]
    pub fn is_corner(self) -> bool {
        (self.moves_left() || self.moves_right()) && (self.moves_top() || self.moves_bottom())
    }

    /// The eight handle centres of `rect`, in the order of [`Self::ALL`].
    #[must_use]
    pub fn positions(rect: ERect) -> [(Self, egui::Pos2); 8] {
        [
            (Self::TopLeft, rect.left_top()),
            (Self::Top, egui::pos2(rect.center().x, rect.top())),
            (Self::TopRight, rect.right_top()),
            (Self::Right, egui::pos2(rect.right(), rect.center().y)),
            (Self::BottomRight, rect.right_bottom()),
            (Self::Bottom, egui::pos2(rect.center().x, rect.bottom())),
            (Self::BottomLeft, rect.left_bottom()),
            (Self::Left, egui::pos2(rect.left(), rect.center().y)),
        ]
    }

    /// The handle within `tolerance` pixels of `pos`, if any.
    #[must_use]
    pub fn at(rect: ERect, pos: egui::Pos2, tolerance: f32) -> Option<Self> {
        Self::positions(rect)
            .into_iter()
            .find(|(_, center)| {
                (center.x - pos.x).abs() <= tolerance && (center.y - pos.y).abs() <= tolerance
            })
            .map(|(handle, _)| handle)
    }
}

/// A crop being edited: the working window of the source and the working frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropEdit {
    /// Kept window of the source, in fractions of the source (0..=1).
    pub crop: Rect,
    /// The object's frame in slide coordinates.
    pub frame: ERect,
    /// Natural size of the loaded source, in pixels.
    pub source_px: (f32, f32),
}

fn finite_rect(rect: ERect) -> bool {
    rect.min.x.is_finite()
        && rect.min.y.is_finite()
        && rect.max.x.is_finite()
        && rect.max.y.is_finite()
}

/// Whether `crop` is a positive, finite window inside the source.
#[must_use]
pub fn valid_crop(crop: Rect) -> bool {
    let (x, y, w, h) = (crop.x, crop.y, crop.w, crop.h);
    [x, y, w, h].iter().all(|v| v.is_finite())
        && w > 0.0
        && h > 0.0
        && x >= 0.0
        && y >= 0.0
        && x + w <= 1.000_001
        && y + h <= 1.000_001
}

fn crop_rect(crop: Rect) -> ERect {
    ERect::from_min_size(
        egui::pos2(crop.x, crop.y),
        vec2(crop.w.max(0.0), crop.h.max(0.0)),
    )
}

fn from_crop_rect(rect: ERect) -> Rect {
    Rect {
        x: rect.min.x,
        y: rect.min.y,
        w: rect.width(),
        h: rect.height(),
    }
}

/// The crop a Shift-locked corner drag settles on.
///
/// `taken` is the window before the drag, `raw` what the plain drag produced.
/// The axis the pointer changed more leads; the other follows from the
/// pre-drag pixel aspect. The result is fitted to the source and the minimum
/// kept size, and falls back to `raw` when that is impossible.
fn aspect_locked(
    taken: Rect,
    raw: Rect,
    handle: CropHandle,
    source_px: (f32, f32),
    min_w: f32,
    min_h: f32,
) -> Rect {
    let (sw, sh) = source_px;
    let aspect = (taken.w * sw) / (taken.h * sh);
    if !(aspect.is_finite() && aspect > 0.0) {
        return raw;
    }
    let mut w = raw.w;
    let mut h = raw.h;
    let change_x = (w - taken.w).abs() / taken.w.max(f32::EPSILON);
    let change_y = (h - taken.h).abs() / taken.h.max(f32::EPSILON);
    if change_x >= change_y {
        h = w * sw / (aspect * sh);
    } else {
        w = h * sh * aspect / sw;
    }
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return raw;
    }

    // Never more than the source between the moving edge and its anchor.
    let available_w = if handle.moves_left() {
        taken.x + taken.w
    } else {
        1.0 - taken.x
    };
    let available_h = if handle.moves_top() {
        taken.y + taken.h
    } else {
        1.0 - taken.y
    };
    if w > available_w {
        w = available_w;
        h = w * sw / (aspect * sh);
    }
    if h > available_h {
        h = available_h;
        w = h * sh * aspect / sw;
    }
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return raw;
    }

    // Meeting the minimum may need both axes to grow.
    if w < min_w || h < min_h {
        let grow = (min_w / w).max(min_h / h);
        if grow.is_finite() && grow > 0.0 {
            w *= grow;
            h *= grow;
        }
        if !(w.is_finite() && h.is_finite()) || w > available_w || h > available_h {
            return raw;
        }
    }

    let x = if handle.moves_left() {
        taken.x + taken.w - w
    } else {
        taken.x
    };
    let y = if handle.moves_top() {
        taken.y + taken.h - h
    } else {
        taken.y
    };
    let crop = Rect { x, y, w, h };
    if valid_crop(crop) { crop } else { raw }
}

impl CropEdit {
    /// Start editing `crop` (or the whole source when there is no crop yet) on
    /// an object frame, given the natural size of the loaded source.
    ///
    /// Returns `None` when the frame or the source size cannot be worked with,
    /// or when the stored crop is not a usable window (the file is hostile).
    #[must_use]
    pub fn new(crop: Option<Rect>, frame: ERect, source_px: (f32, f32)) -> Option<Self> {
        let (sw, sh) = source_px;
        if !(sw.is_finite() && sh.is_finite() && sw > 0.0 && sh > 0.0) || !finite_rect(frame) {
            return None;
        }
        if frame.width() <= 0.0 || frame.height() <= 0.0 {
            return None;
        }
        let crop = crop.unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        });
        if !valid_crop(crop) {
            return None;
        }
        Some(Self {
            crop,
            frame,
            source_px,
        })
    }

    /// Start from the whole source, keeping the frame as it is on the slide.
    #[must_use]
    pub fn whole_source(frame: ERect, source_px: (f32, f32)) -> Option<Self> {
        Self::new(None, frame, source_px)
    }

    /// The working crop as a model rect.
    #[must_use]
    pub fn crop_model(&self) -> Rect {
        from_crop_rect(crop_rect(self.crop))
    }

    /// A stored [`CropRect`](coursecraft_course::CropRect) as the plain rect
    /// this module works with.
    #[must_use]
    pub fn model_rect(crop: coursecraft_course::CropRect) -> Rect {
        Rect {
            x: crop.x,
            y: crop.y,
            w: crop.w,
            h: crop.h,
        }
    }

    /// Slide pixels per source pixel along x.
    #[must_use]
    pub fn scale_x(&self) -> f32 {
        self.frame.width() / (self.crop.w * self.source_px.0)
    }

    /// Slide pixels per source pixel along y.
    #[must_use]
    pub fn scale_y(&self) -> f32 {
        self.frame.height() / (self.crop.h * self.source_px.1)
    }

    /// Where the whole source would sit on the slide at the current scale: the
    /// kept frame grown back out to the full picture.
    #[must_use]
    pub fn full_rect(&self) -> ERect {
        ERect::from_min_size(
            egui::pos2(
                self.frame.min.x - self.crop.x * self.source_px.0 * self.scale_x(),
                self.frame.min.y - self.crop.y * self.source_px.1 * self.scale_y(),
            ),
            vec2(
                self.source_px.0 * self.scale_x(),
                self.source_px.1 * self.scale_y(),
            ),
        )
    }

    /// Move a source point (in fractions of the source) to slide coordinates.
    #[must_use]
    pub fn source_to_slide(&self, u: f32, v: f32) -> egui::Pos2 {
        let full = self.full_rect();
        egui::pos2(
            full.min.x + u * full.width(),
            full.min.y + v * full.height(),
        )
    }

    /// The whole-source rect that keeps the kept frame exactly where it is.
    #[must_use]
    pub fn source_overlay(&self) -> ERect {
        self.full_rect()
    }

    /// New state after moving a handle by `delta` slide pixels, keeping the
    /// opposite edge of the kept picture fixed on the slide.
    ///
    /// The kept area is clamped to the source, each edge keeps at least
    /// [`MIN_KEPT_PX`] of slide pixels (less only when the whole source is
    /// smaller than that), and `keep_aspect` (Shift on a corner) holds the
    /// kept window's aspect. Non-finite or non-positive input returns the state
    /// unchanged.
    #[must_use]
    pub fn drag_handle(&self, handle: CropHandle, delta: egui::Vec2, keep_aspect: bool) -> Self {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return *self;
        }
        let (sw, sh) = self.source_px;
        let (scale_x, scale_y) = (self.scale_x(), self.scale_y());
        if !(scale_x.is_finite() && scale_y.is_finite() && scale_x > 0.0 && scale_y > 0.0) {
            return *self;
        }
        // Slide size of the whole source, fixed for the drag: the scale must be
        // the pre-drag one so the kept pixels do not move.
        let (px_x, px_y) = (sw * scale_x, sh * scale_y);
        let min_w = (MIN_KEPT_PX / px_x).min(1.0);
        let min_h = (MIN_KEPT_PX / px_y).min(1.0);

        // The edges the handle does not move stay where they are.
        let taken = self.crop;
        let anchor_left = taken.x;
        let anchor_top = taken.y;
        let anchor_right = taken.x + taken.w;
        let anchor_bottom = taken.y + taken.h;

        // Dragging past an edge stops at the neighbouring edge rather than
        // crossing it, so a single drag never flips the kept window.
        let mut left = anchor_left;
        let mut top = anchor_top;
        let mut right = anchor_right;
        let mut bottom = anchor_bottom;
        if handle.moves_left() {
            left = (left + delta.x / px_x).clamp(0.0, (anchor_right - min_w).max(0.0));
        }
        if handle.moves_right() {
            right = (right + delta.x / px_x).clamp((anchor_left + min_w).min(1.0), 1.0);
        }
        if handle.moves_top() {
            top = (top + delta.y / px_y).clamp(0.0, (anchor_bottom - min_h).max(0.0));
        }
        if handle.moves_bottom() {
            bottom = (bottom + delta.y / px_y).clamp((anchor_top + min_h).min(1.0), 1.0);
        }

        let raw = Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        };
        let crop = if keep_aspect && handle.is_corner() {
            aspect_locked(taken, raw, handle, self.source_px, min_w, min_h)
        } else {
            raw
        };
        if !valid_crop(crop) {
            return *self;
        }

        // The kept pixels keep their pre-drag scale; the edge opposite the
        // moving one stays on the slide.
        let new_w = crop.w * px_x;
        let new_h = crop.h * px_y;
        if !(new_w.is_finite() && new_h.is_finite()) || new_w <= 0.0 || new_h <= 0.0 {
            return *self;
        }
        let mut min = self.frame.min;
        let mut max = self.frame.max;
        if handle.moves_left() {
            min.x = self.frame.max.x - new_w;
        }
        if handle.moves_right() {
            max.x = self.frame.min.x + new_w;
        }
        if handle.moves_top() {
            min.y = self.frame.max.y - new_h;
        }
        if handle.moves_bottom() {
            max.y = self.frame.min.y + new_h;
        }
        let frame = ERect::from_min_max(min, max);
        if finite_rect(frame) && frame.width() > 0.0 && frame.height() > 0.0 {
            Self {
                crop,
                frame,
                source_px: self.source_px,
            }
        } else {
            *self
        }
    }

    /// Move the source under the fixed frame by `delta` slide pixels.
    ///
    /// The frame does not move; the window slides within the source, clamped so
    /// the frame always stays covered by the picture.
    #[must_use]
    pub fn drag_source(&self, delta: egui::Vec2) -> Self {
        if !delta.x.is_finite() || !delta.y.is_finite() {
            return *self;
        }
        let (sw, sh) = self.source_px;
        let (scale_x, scale_y) = (self.scale_x(), self.scale_y());
        if !(scale_x.is_finite() && scale_y.is_finite() && scale_x > 0.0 && scale_y > 0.0) {
            return *self;
        }
        let max_x = 1.0 - self.crop.w;
        let max_y = 1.0 - self.crop.h;
        let x = (self.crop.x - delta.x / (sw * scale_x)).clamp(0.0, max_x.max(0.0));
        let y = (self.crop.y - delta.y / (sh * scale_y)).clamp(0.0, max_y.max(0.0));
        Self {
            crop: Rect {
                x,
                y,
                w: self.crop.w,
                h: self.crop.h,
            },
            ..*self
        }
    }

    /// The frame that shows the whole source at the current scale, grown out
    /// from the kept frame so every kept pixel stays exactly where it is.
    ///
    /// Used by the inspector's Reset: clear the crop and let the frame take the
    /// space the hidden pixels need.
    #[must_use]
    pub fn reset_frame(&self) -> ERect {
        let (sw, sh) = self.source_px;
        let full = self.full_rect();
        let usable = sw.is_finite()
            && sh.is_finite()
            && sw > 0.0
            && sh > 0.0
            && finite_rect(full)
            && full.width() > 0.0
            && full.height() > 0.0;
        if usable { full } else { self.frame }
    }

    /// The crop that shows the whole source.
    #[must_use]
    pub fn full_crop() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn edit() -> CropEdit {
        // A 400x200 source shown whole in a 400x200 frame at (100, 100).
        CropEdit::new(
            None,
            ERect::from_min_size(pos2(100.0, 100.0), vec2(400.0, 200.0)),
            (400.0, 200.0),
        )
        .expect("valid edit")
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// A drag that moves every edge a handle owns inward by 20 x / 10 y px.
    fn inward(handle: CropHandle) -> egui::Vec2 {
        vec2(
            if handle.moves_left() {
                20.0
            } else if handle.moves_right() {
                -20.0
            } else {
                0.0
            },
            if handle.moves_top() {
                10.0
            } else if handle.moves_bottom() {
                -10.0
            } else {
                0.0
            },
        )
    }

    #[test]
    fn whole_source_has_unit_scale_and_frame() {
        let e = edit();
        assert!(close(e.scale_x(), 1.0));
        assert!(close(e.scale_y(), 1.0));
        assert_eq!(e.full_rect(), e.frame);
        assert_eq!(e.crop_model(), CropEdit::full_crop());
    }

    #[test]
    fn each_handle_moves_only_its_edges() {
        let e = edit();
        for handle in CropHandle::ALL {
            let next = e.drag_handle(handle, inward(handle), false);
            let (l, t, r, b) = (
                next.crop.x,
                next.crop.y,
                next.crop.x + next.crop.w,
                next.crop.y + next.crop.h,
            );
            assert!(
                (handle.moves_left() && close(l, 0.05) || !handle.moves_left() && close(l, 0.0)),
                "{handle:?} left"
            );
            assert!(
                (handle.moves_right() && close(r, 0.95) || !handle.moves_right() && close(r, 1.0)),
                "{handle:?} right"
            );
            assert!(
                (handle.moves_top() && close(t, 0.05) || !handle.moves_top() && close(t, 0.0)),
                "{handle:?} top"
            );
            assert!(
                (handle.moves_bottom() && close(b, 0.95)
                    || !handle.moves_bottom() && close(b, 1.0)),
                "{handle:?} bottom"
            );
            // The edge that moved follows the pointer; its opposite stays put.
            if handle.moves_left() {
                assert!(close(next.frame.max.x, e.frame.max.x));
                assert!(close(next.frame.min.x, e.frame.min.x + 20.0));
            }
            if handle.moves_right() {
                assert!(close(next.frame.min.x, e.frame.min.x));
                assert!(close(next.frame.max.x, e.frame.max.x - 20.0));
            }
            if handle.moves_top() {
                assert!(close(next.frame.max.y, e.frame.max.y));
                assert!(close(next.frame.min.y, e.frame.min.y + 10.0));
            }
            if handle.moves_bottom() {
                assert!(close(next.frame.min.y, e.frame.min.y));
                assert!(close(next.frame.max.y, e.frame.max.y - 10.0));
            }
        }
    }

    #[test]
    fn a_kept_pixel_stays_put_after_any_handle_drag() {
        let e = edit();
        let kept_pixel = e.source_to_slide(0.5, 0.5);
        for handle in CropHandle::ALL {
            let next = e.drag_handle(handle, inward(handle), false);
            let same_pixel = next.source_to_slide(0.5, 0.5);
            assert!(
                close(same_pixel.x, kept_pixel.x) && close(same_pixel.y, kept_pixel.y),
                "{handle:?}: {same_pixel:?} != {kept_pixel:?}"
            );
        }
    }

    #[test]
    fn kept_pixels_stay_put_under_the_pointer() {
        let e = edit();
        // Grab the right edge and pull it left by 40 slide pixels: the kept
        // pixel under the pointer at the end is the new right edge, and the
        // left edge did not move.
        let next = e.drag_handle(CropHandle::Right, vec2(-40.0, 0.0), false);
        let left_pixel = e.source_to_slide(0.0, 0.5);
        assert!(close(next.source_to_slide(0.0, 0.5).x, left_pixel.x));
        assert!(close(next.crop.w, 0.9));
        assert!(close(next.frame.width(), 360.0));
        assert!(close(next.frame.min.x, e.frame.min.x));
        assert!(close(next.frame.max.x, e.frame.max.x - 40.0));
    }

    #[test]
    fn every_edge_clamps_to_the_source() {
        let e = edit();
        for handle in CropHandle::ALL {
            // Way out one way…
            let out = e.drag_handle(handle, vec2(-10_000.0, -10_000.0), false);
            assert!(valid_crop(out.crop_model()), "{handle:?} far negative");
            // …and the other.
            let out = e.drag_handle(handle, vec2(10_000.0, 10_000.0), false);
            let crop = out.crop_model();
            assert!(valid_crop(crop), "{handle:?} far positive");
            assert!(crop.x >= 0.0 && crop.y >= 0.0);
            assert!(crop.x + crop.w <= 1.0 && crop.y + crop.h <= 1.0);
            assert!(out.frame.width() >= MIN_KEPT_PX - 1e-3);
            assert!(out.frame.height() >= MIN_KEPT_PX - 1e-3);
        }
    }

    #[test]
    fn minimum_kept_size_is_eight_slide_pixels() {
        let e = edit();
        // Squeeze the left edge all the way past the right one.
        let next = e.drag_handle(CropHandle::Left, vec2(10_000.0, 0.0), false);
        assert!(close(next.frame.width(), MIN_KEPT_PX));
        assert!(close(next.frame.max.x, e.frame.max.x));
        let next = e.drag_handle(CropHandle::Top, vec2(0.0, 10_000.0), false);
        assert!(close(next.frame.height(), MIN_KEPT_PX));
        let next = e.drag_handle(CropHandle::BottomRight, vec2(-10_000.0, -10_000.0), false);
        assert!(close(next.frame.width(), MIN_KEPT_PX));
        assert!(close(next.frame.height(), MIN_KEPT_PX));
    }

    #[test]
    fn minimum_size_never_exceeds_a_source_shown_at_under_eight_pixels() {
        // A 4x4 source in a 4x4 frame: the smallest kept area is the whole
        // picture, because 8 slide px is more than the source has.
        let e = CropEdit::new(
            None,
            ERect::from_min_size(pos2(0.0, 0.0), vec2(4.0, 4.0)),
            (4.0, 4.0),
        )
        .expect("valid edit");
        let next = e.drag_handle(CropHandle::Left, vec2(10_000.0, 0.0), false);
        assert!(close(next.crop.w, 1.0));
        assert!(close(next.frame.width(), 4.0));
    }

    #[test]
    fn drag_inside_moves_the_source_and_clamps_to_coverage() {
        let e = edit();
        // Not cropped yet: nothing to slide.
        let same = e.drag_source(vec2(50.0, 50.0));
        assert!(close(same.crop.x, 0.0) && close(same.crop.y, 0.0));

        let cropped = e.drag_handle(CropHandle::BottomRight, vec2(-100.0, -50.0), false);
        assert!(close(cropped.crop.w, 0.75) && close(cropped.crop.h, 0.75));
        // Moving the pointer left shows more of the right of the source.
        let moved = cropped.drag_source(vec2(-50.0, 0.0));
        assert!(close(
            moved.crop.x,
            cropped.crop.x + 50.0 / (400.0 * cropped.scale_x())
        ));
        assert!(close(moved.crop.y, cropped.crop.y));
        // The frame never moves while the source slides, and neither does the
        // scale.
        assert_eq!(moved.frame, cropped.frame);
        assert!(close(moved.scale_x(), cropped.scale_x()));
        // Clamped so the picture keeps covering the frame.
        let far = cropped.drag_source(vec2(10_000.0, 10_000.0));
        assert!(close(far.crop.x, 0.0));
        assert!(close(far.crop.y, 0.0));
        let far = cropped.drag_source(vec2(-10_000.0, -10_000.0));
        assert!(close(far.crop.x, 0.25));
        assert!(close(far.crop.y, 0.25));
    }

    #[test]
    fn shift_on_a_corner_keeps_the_crop_aspect() {
        let e = edit();
        let next = e.drag_handle(CropHandle::BottomRight, vec2(-100.0, -10.0), true);
        let aspect = (next.crop.w * 400.0) / (next.crop.h * 200.0);
        assert!(close(aspect, 2.0), "aspect {aspect}");
        assert!(close(next.crop.w, 0.75));
        assert!(close(next.crop.h, 0.75));
        assert_eq!(next.frame.min, e.frame.min);
    }

    #[test]
    fn shift_on_a_side_leaves_the_other_axis_alone() {
        let e = edit();
        let next = e.drag_handle(CropHandle::Right, vec2(-100.0, 0.0), true);
        assert!(close(next.crop.w, 0.75));
        assert!(close(next.crop.h, 1.0));
    }

    #[test]
    fn non_finite_input_is_ignored() {
        let e = edit();
        for delta in [
            vec2(f32::NAN, 0.0),
            vec2(0.0, f32::INFINITY),
            vec2(f32::NEG_INFINITY, f32::NAN),
        ] {
            assert_eq!(e.drag_handle(CropHandle::BottomRight, delta, false), e);
            assert_eq!(e.drag_handle(CropHandle::TopLeft, delta, true), e);
            assert_eq!(e.drag_source(delta), e);
        }
        assert!(CropEdit::new(None, e.frame, (f32::NAN, 200.0)).is_none());
        assert!(CropEdit::new(None, e.frame, (0.0, 200.0)).is_none());
        assert!(
            CropEdit::new(
                None,
                ERect::from_min_size(pos2(f32::NAN, 0.0), vec2(10.0, 10.0)),
                (400.0, 200.0)
            )
            .is_none()
        );
    }

    #[test]
    fn a_hostile_stored_crop_is_refused() {
        let frame = ERect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
        let bad = [
            Rect {
                x: -0.5,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
            Rect {
                x: 0.5,
                y: 0.0,
                w: 0.75,
                h: 1.0,
            },
            Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 1.0,
            },
            Rect {
                x: f32::NAN,
                y: 0.0,
                w: 0.5,
                h: 0.5,
            },
        ];
        for crop in bad {
            assert!(CropEdit::new(Some(crop), frame, (100.0, 100.0)).is_none());
        }
    }

    #[test]
    fn opening_a_cropped_image_keeps_the_kept_pixels_where_they_were() {
        // 400x200 source, half of it kept in a 200x200 frame: one source pixel
        // is 1 slide pixel on each axis.
        let crop = Rect {
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let frame = ERect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 200.0));
        let e = CropEdit::new(Some(crop), frame, (400.0, 200.0)).expect("valid edit");
        assert!(close(e.scale_x(), 1.0) && close(e.scale_y(), 1.0));
        assert_eq!(e.frame, frame);
        // The hidden half sits to the right of the kept frame.
        let full = e.source_overlay();
        assert!(close(full.min.x, 10.0) && close(full.max.x, 410.0));
        assert!(close(full.min.y, 20.0) && close(full.max.y, 220.0));
    }

    #[test]
    fn reset_frame_shows_the_whole_source_at_the_current_scale_without_moving_kept_pixels() {
        // A 400x200 source with its middle half kept in a 200x200 frame: the
        // left quarter of the source is hidden to the left of the frame.
        let crop = Rect {
            x: 0.25,
            y: 0.0,
            w: 0.5,
            h: 1.0,
        };
        let frame = ERect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 200.0));
        let e = CropEdit::new(Some(crop), frame, (400.0, 200.0)).expect("valid edit");
        let reset = e.reset_frame();
        assert_eq!(reset, e.full_rect());
        assert!(close(reset.width(), 400.0));
        assert!(close(reset.height(), 200.0));
        // The hidden left quarter appears, so the frame grows left from the
        // kept frame rather than moving the kept picture.
        assert!(close(reset.min.x, 0.0));
        assert!(close(reset.min.y, frame.min.y));

        // Kept pixels stay exactly where they were on the slide.
        for (u, v) in [(0.25, 0.0), (0.5, 0.5), (0.75, 1.0)] {
            let kept = e.source_to_slide(u, v);
            let after = reset.min + vec2(reset.width() * u, reset.height() * v);
            assert!(
                close(after.x, kept.x) && close(after.y, kept.y),
                "({u}, {v}): {after:?} != {kept:?}"
            );
        }
    }

    #[test]
    fn handle_hit_testing_finds_corners_and_edges() {
        let rect = ERect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        assert_eq!(
            CropHandle::at(rect, pos2(0.0, 0.0), 5.0),
            Some(CropHandle::TopLeft)
        );
        assert_eq!(
            CropHandle::at(rect, pos2(100.0, 25.0), 5.0),
            Some(CropHandle::Right)
        );
        assert_eq!(CropHandle::at(rect, pos2(50.0, 25.0), 5.0), None);
    }
}
