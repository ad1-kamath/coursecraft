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
mod mesh_post;
pub mod offscreen;
pub mod paint;
pub mod path_geometry;
pub mod player_frame;
pub mod sequence;
pub mod shape_fill;
pub mod shape_geometry;
pub mod snapshot;
pub mod text_layout;
pub mod ticker;
pub mod view;

#[cfg(test)]
mod bughunt_m26_render;
#[cfg(test)]
mod justify_tests;
#[cfg(test)]
mod m29_state_extras_tests;
#[cfg(test)]
mod m29_state_resolver_tests;
#[cfg(test)]
mod m39_t_tests;
#[cfg(test)]
mod m46_anim_render_tests;
#[cfg(test)]
mod m48_nofill_effects_tests;
#[cfg(test)]
mod m49_group_anim_tests;
#[cfg(test)]
mod m49_master_anim_tests;
#[cfg(test)]
mod neon_render_tests;
#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod question_frame_tests;
#[cfg(test)]
mod review_tests;
#[cfg(test)]
mod shape_tests;
#[cfg(test)]
mod shaped_picture_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod u19_state_hover_tests;
#[cfg(test)]
mod u23_hand_pointer_tests;
#[cfg(test)]
mod uiux_player_tests;

pub use access::{
    ANSWER_REQUIRED_ANNOUNCEMENT, AccessNode, AccessRole, AccessTree, DomKeyAction, announcement,
    build_access_tree, dom_key_action, drag_is_held, map_dom_key,
};
pub use chrome::{
    DEFAULT_BAR_COLOR, DEFAULT_BAR_TEXT_COLOR, format_ms, nav_limits, progress_fraction,
};
pub use color::parse_color;
pub use compose::{
    AnimLook, DEFAULT_BUTTON_COLOR, DEFAULT_TEXT_COLOR, DrawItem, GroupAnim, Hit, PlacedMask,
    ResolvedCellText, ResolvedLinkRect, ResolvedOutline, ResolvedPath, ResolvedShapeText,
    ResolvedTable, ResolvedTableCell, ResolvedTableRow, ResolvedTextRun, ReviewMark, Visual,
    apply_sample_to_items, compose, compose_authoring, compose_authoring_layers,
    compose_authoring_layers_opts, compose_object, compose_object_items, compose_object_with_theme,
    compose_object_with_value, compose_with_pointer, flip_point, item_bounds, item_corners,
    item_hit_test, item_hit_test_scaled, item_link_at, item_local_point, overlay_screen_rect,
    question_prompt_rect, question_prompt_rect_in, resolve_text_runs, rotate_point,
};
pub use design::{
    CONTROL_COMPACT, CONTROL_PRIMARY, CONTROL_TOOLBAR, ICON_SM, ICON_TOOLBAR, PANEL_MARGIN,
    PLAYER_GROUND, PaintQuality, Palette, PerfProfile, PerformanceMode, RADIUS_LG, RADIUS_MD,
    RADIUS_SM, RADIUS_XL, SIZE_BODY, SIZE_BODY_STRONG, SIZE_CAPTION, SIZE_DISPLAY, SIZE_HEADING,
    SIZE_MONO, SIZE_SMALL, SIZE_TITLE, SPACE_6, SPACE_20, SPACE_24, SPACE_32, SPACE_48, SPACE_LG,
    SPACE_MD, SPACE_SM, SPACE_XL, SPACE_XS, ThemeMode, ThemePref, apply_style, body, body_strong,
    caption, contrast_ratio, dialog_shadow, display, heading, paint_quality, palette,
    popover_shadow, relative_luminance, set_render_profile, slide_shadow, small, space, theme_mode,
    title,
};
pub use drawer::{DrawerTab, DrawerView, available_tabs, visible_glossary};
pub use focus::{
    ChromeButton, ExternalFocus, FocusCandidate, FocusManager, FocusTarget, KeyboardDragVisual,
    build_list, fill_in_id, focus_order_candidates, input_text_id,
};
#[cfg(not(target_arch = "wasm32"))]
pub use font::IndexFontProvider;
pub use font::{
    SystemFontData, SystemFontProvider, UndrawableChar, course_face_family, font_epoch,
    install_course_fonts, install_fonts, set_system_font_provider, slide_font, slide_font_named,
    system_face_family, undrawable_chars,
};
pub use icons::{Icon, icon, icon_fill, icon_text};
pub use layout::SlideTransform;
pub use lottie::{LottieInfo, MAX_LOTTIE_BYTES, frame_at, info_of, parse_composition, parse_info};
#[cfg(debug_assertions)]
pub use media::test_counts;
pub use media::{LottieView, MediaCache, MediaEntry, MediaFetch, MediaSource, MediaView};
pub use media_driver::{
    CaptionFrame, CaptionStore, MediaDriver, PlaybackMediaSource, collect_video_ids,
    find_media_target, slide_has_captions,
};
pub use offscreen::{
    DEFAULT_RENDER_WIDTH, MAX_RENDER_WIDTH, MIN_RENDER_WIDTH, RenderError, RenderMode,
    RenderOptions, RgbaImage, decode_png, encode_png, render_player_rgba, render_slide_rgba,
    render_ui,
};
pub use paint::{
    DEFAULT_BACKGROUND_COLOR, DEFAULT_SHAPE_MESH_CACHE_MAX, Highlight, clear_shape_mesh_cache,
    dial_point, paint, paint_outline, paint_shape, set_shape_mesh_cache_limit, slider_fill_rect,
    slider_thumb_center, slider_track_rect, value_fraction,
};
pub use path_geometry::{
    CONNECTOR_MIN_LEAD, PathMeshSpec, arc_points, connector_site, connector_site_direction,
    flatten as flatten_path, line_end_size, path_closest, path_contains, path_distance, path_mesh,
    route_connector,
};
pub use player_frame::{FrameOutput, PlayerFrame, show_loading, show_loading_text};
pub use shape_fill::{
    StrokeJoin, even_odd_area, fill_contours, stroke_contours, stroke_contours_pattern,
    stroke_contours_with_join, stroke_path, stroke_path_pattern, stroke_path_with_join,
};
pub use shape_geometry::{
    Pt as ShapePoint, adjust_from_point, adjust_handle_name, adjust_handles, arc, regular_polygon,
    rounded_polygon, scale, shape_contours, shape_contours_adjusted, shape_text_rect, star,
    stroke_polyline, translate,
};
pub use snapshot::{PlayerSnapshotSelect, snapshot_player};
pub use text_layout::{
    AUTOFIT_MIN_SCALE, DEFAULT_TEXT_LAYOUT_CACHE_MAX, INDENT_STEP, LaidLink, LaidMarker,
    LaidParagraph, LaidText, clear_text_layout_cache, fill_item_links, inline_edit_layout_job,
    inline_edit_layout_job_with_wrap, justify_galley, justify_galley_snapped, layout_text,
    layout_text_with_wrap, set_text_layout_cache_limit,
};
pub use ticker::Ticker;
pub use view::{
    LIGHTBOX_BACKDROP, LIGHTBOX_FADE_SECONDS, LIGHTBOX_SCALE, PointerState, SlideView, ValueDrag,
    hit_test, lightbox_box_rect, lightbox_close_rect,
};
