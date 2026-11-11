//! Slide layout, scaling and coordinate transformation.

use coursecraft_course::Size;

/// Coordinate transform between slide coordinates and screen coordinates.
///
/// Holds the uniform scale factor and the centred bounding box of the slide on
/// the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlideTransform {
    scale: f32,
    slide_rect: egui::Rect,
}

impl SlideTransform {
    /// Compute a transform that uniformly scales and centres a slide inside `available`.
    ///
    /// Returns `None` if `slide` has zero width or height, or if `available`
    /// has non-positive or non-finite dimensions.
    #[must_use]
    pub fn fit(available: egui::Rect, slide: Size) -> Option<Self> {
        if slide.w == 0 || slide.h == 0 {
            return None;
        }
        if !available.min.x.is_finite()
            || !available.min.y.is_finite()
            || !available.max.x.is_finite()
            || !available.max.y.is_finite()
        {
            return None;
        }

        let avail_w = available.width();
        let avail_h = available.height();
        if !avail_w.is_finite() || !avail_h.is_finite() || avail_w <= 0.0 || avail_h <= 0.0 {
            return None;
        }

        let slide_w = slide.w as f32;
        let slide_h = slide.h as f32;
        let scale_x = avail_w / slide_w;
        let scale_y = avail_h / slide_h;
        let scale = scale_x.min(scale_y);

        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }

        let scaled_w = slide_w * scale;
        let scaled_h = slide_h * scale;
        let offset_x = available.min.x + (avail_w - scaled_w) * 0.5;
        let offset_y = available.min.y + (avail_h - scaled_h) * 0.5;
        let slide_rect = egui::Rect::from_min_size(
            egui::pos2(offset_x, offset_y),
            egui::vec2(scaled_w, scaled_h),
        );

        Some(Self { scale, slide_rect })
    }

    /// The uniform scale factor from slide coordinates to screen coordinates.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The bounding rectangle of the slide on the screen.
    #[must_use]
    pub fn slide_rect(&self) -> egui::Rect {
        self.slide_rect
    }

    /// Map a point from slide coordinates to screen coordinates.
    #[must_use]
    pub fn to_screen_pos(&self, slide_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            self.slide_rect.min.x + slide_pos.x * self.scale,
            self.slide_rect.min.y + slide_pos.y * self.scale,
        )
    }

    /// Map a rectangle from slide coordinates to screen coordinates.
    #[must_use]
    pub fn to_screen_rect(&self, slide_rect: egui::Rect) -> egui::Rect {
        let min = self.to_screen_pos(slide_rect.min);
        let max = self.to_screen_pos(slide_rect.max);
        egui::Rect::from_min_max(min, max)
    }

    /// Map a point from screen coordinates to slide coordinates.
    #[must_use]
    pub fn to_slide_pos(&self, screen_pos: egui::Pos2) -> egui::Pos2 {
        egui::pos2(
            (screen_pos.x - self.slide_rect.min.x) / self.scale,
            (screen_pos.y - self.slide_rect.min.y) / self.scale,
        )
    }
}
