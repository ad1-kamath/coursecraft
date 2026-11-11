//! CourseCraft slide compositor (L3).
//!
//! Draws the current slide of a running [`coursecraft_logic::Player`] with egui
//! and turns pointer input into runtime [`coursecraft_logic::Input`]s. It holds
//! only presentational state (hover, pressed); every play-time decision belongs
//! to `coursecraft-logic`. Rules are in `docs/spec/triggers.md`.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod access;
pub mod chrome;
pub mod color;
pub mod compose;
pub mod design;
pub mod drawer;
pub mod focus;
pub mod font;
pub mod icons;
pub mod layout;
pub mod lottie;
pub mod media;
pub mod media_driver;
pub mod offscreen;
pub mod paint;
pub mod path_geometry;
pub mod player_frame;
pub mod shape_fill;
pub mod shape_geometry;
pub mod snapshot;
pub mod text_layout;
pub mod ticker;
pub mod view;

#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod tests;

pub use access::{
    ANSWER_REQUIRED_ANNOUNCEMENT, AccessNode, AccessRole, AccessTree, announcement,
    build_access_tree,
};
pub use chrome::{
    DEFAULT_BAR_COLOR, DEFAULT_BAR_TEXT_COLOR, format_ms, nav_limits, progress_fraction,
};
pub use color::parse_color;
pub use compose::{
    DEFAULT_BUTTON_COLOR, DEFAULT_TEXT_COLOR, DrawItem, Hit, ResolvedCellText, ResolvedLinkRect,
    ResolvedOutline, ResolvedPath, ResolvedShapeText, ResolvedTable, ResolvedTableCell,
    ResolvedTableRow, ResolvedTextRun, ReviewMark, Visual, compose, compose_authoring,
    compose_authoring_layers, compose_object, compose_object_with_theme, compose_object_with_value,
    flip_point, item_bounds, item_corners, item_hit_test, item_link_at, item_local_point,
    question_prompt_rect, rotate_point,
};
pub use design::{
    CONTROL_COMPACT, CONTROL_PRIMARY, CONTROL_TOOLBAR, ICON_SM, ICON_TOOLBAR, PANEL_MARGIN,
    PLAYER_GROUND, Palette, RADIUS_LG, RADIUS_MD, RADIUS_SM, RADIUS_XL, SIZE_BODY,
    SIZE_BODY_STRONG, SIZE_CAPTION, SIZE_DISPLAY, SIZE_HEADING, SIZE_MONO, SIZE_SMALL, SIZE_TITLE,
    SPACE_6, SPACE_20, SPACE_24, SPACE_32, SPACE_48, SPACE_LG, SPACE_MD, SPACE_SM, SPACE_XL,
    SPACE_XS, ThemeMode, ThemePref, apply_style, body, body_strong, caption, contrast_ratio,
    dialog_shadow, display, heading, palette, popover_shadow, relative_luminance, slide_shadow,
    small, space, theme_mode, title,
};
pub use drawer::{DrawerTab, DrawerView, available_tabs, visible_glossary};
pub use focus::{
    ChromeButton, ExternalFocus, FocusManager, FocusTarget, KeyboardDragVisual, build_list,
    fill_in_id, input_text_id,
};
pub use font::{
    course_face_family, install_course_fonts, install_fonts, slide_font, slide_font_named,
};
pub use icons::{Icon, icon, icon_fill, icon_text};
pub use layout::SlideTransform;
pub use lottie::{LottieInfo, MAX_LOTTIE_BYTES, frame_at, info_of, parse_composition, parse_info};
pub use media::{LottieView, MediaCache, MediaEntry, MediaFetch, MediaSource, MediaView};
pub use media_driver::{
    CaptionFrame, CaptionStore, MediaDriver, PlaybackMediaSource, collect_video_ids,
    find_media_target, slide_has_captions,
};
pub use offscreen::{
    DEFAULT_RENDER_WIDTH, MAX_RENDER_WIDTH, MIN_RENDER_WIDTH, RenderError, RenderMode,
    RenderOptions, RgbaImage, encode_png, render_player_rgba, render_slide_rgba, render_ui,
};
pub use paint::{
    DEFAULT_BACKGROUND_COLOR, Highlight, dial_point, paint, paint_outline, paint_shape,
    slider_fill_rect, slider_thumb_center, slider_track_rect, value_fraction,
};
pub use path_geometry::{
    CONNECTOR_MIN_LEAD, PathMeshSpec, arc_points, connector_site, connector_site_direction,
    flatten as flatten_path, line_end_size, path_closest, path_contains, path_distance, path_mesh,
    route_connector,
};
pub use player_frame::{FrameOutput, PlayerFrame, show_loading, show_loading_text};
pub use shape_fill::{even_odd_area, fill_contours, stroke_contours, stroke_path};
pub use shape_geometry::{
    Pt as ShapePoint, adjust_from_point, adjust_handle_name, adjust_handles, arc, regular_polygon,
    rounded_polygon, scale, shape_contours, shape_contours_adjusted, shape_text_rect, star,
    stroke_polyline, translate,
};
pub use snapshot::{PlayerSnapshotSelect, snapshot_player};
pub use text_layout::{
    AUTOFIT_MIN_SCALE, INDENT_STEP, LaidLink, LaidMarker, LaidParagraph, LaidText, fill_item_links,
    layout_text,
};
pub use ticker::Ticker;
pub use view::{
    LIGHTBOX_BACKDROP, LIGHTBOX_FADE_SECONDS, LIGHTBOX_SCALE, PointerState, SlideView, ValueDrag,
    hit_test, lightbox_box_rect, lightbox_close_rect,
};
