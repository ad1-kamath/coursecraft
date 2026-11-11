//! Browser printing for certificate slides.
//!
//! Renders the current slide without the clicked Print button, turns it into a
//! PNG, and opens a minimal print window. Falls back to a notice when the
//! browser blocks the pop-up.

use coursecraft_logic::Player;
use coursecraft_render::{MediaSource, PlayerFrame, encode_png, render_player_rgba};
use eframe::wasm_bindgen::{JsCast, JsValue};

/// Escape text placed in the print document's `<title>`.
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Call `Document.write` through reflection: web-sys 0.3 has no binding for it.
fn write_document(doc: &web_sys::Document, html: &str) -> bool {
    let Ok(write) = js_sys::Reflect::get(doc.as_ref(), &JsValue::from_str("write")) else {
        return false;
    };
    let Ok(function) = write.dyn_into::<js_sys::Function>() else {
        return false;
    };
    function
        .call1(doc.as_ref(), &JsValue::from_str(html))
        .is_ok()
}

fn show_blocked(player: &Player<'_>, frame: &mut PlayerFrame) {
    crate::lms::log_warn("print window was blocked");
    frame.notice = Some(player.course().chrome_string("printBlocked").to_owned());
}

/// Render `player`'s current slide and open the browser's print dialog.
pub fn print_slide(
    player: &Player<'_>,
    media: &dyn MediaSource,
    frame: &mut PlayerFrame,
    exclude: &[String],
) {
    let image = match render_player_rgba(player, 1600, exclude, media, None) {
        Ok(image) => image,
        Err(error) => {
            crate::lms::log_warn(&format!("could not render the print page: {error}"));
            return;
        }
    };
    let png = match encode_png(&image) {
        Ok(png) => png,
        Err(error) => {
            crate::lms::log_warn(&format!("could not encode the print page: {error}"));
            return;
        }
    };

    let mut binary = String::with_capacity(png.len());
    for byte in png {
        binary.push(char::from(byte));
    }

    let Some(window) = web_sys::window() else {
        show_blocked(player, frame);
        return;
    };
    let Ok(base64) = window.btoa(&binary) else {
        crate::lms::log_warn("could not base64-encode the print page");
        return;
    };

    let Ok(Some(print_window)) = window.open_with_url_and_target("", "_blank") else {
        show_blocked(player, frame);
        return;
    };
    let Some(document) = print_window.document() else {
        show_blocked(player, frame);
        return;
    };

    let title = escape_html(player.course().chrome_string("printTitle"));
    let html = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>@media print {{ img {{ width: 100% }} }}</style></head>\
         <body style=\"margin:0\"><img alt=\"\" src=\"data:image/png;base64,{base64}\" \
         onload=\"window.print()\"></body></html>"
    );
    if !write_document(&document, &html) {
        show_blocked(player, frame);
    }
}
