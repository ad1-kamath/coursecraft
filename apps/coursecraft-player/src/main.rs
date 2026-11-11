//! `coursecraft-player`: plays a course on the desktop (eframe/wgpu) or in the
//! browser (the same code compiled to wasm and served by trunk).

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

#[cfg(target_arch = "wasm32")]
mod a11y;
mod app;
#[cfg(target_arch = "wasm32")]
mod lms;
mod load;
#[cfg(target_arch = "wasm32")]
mod lrs;
#[cfg(target_arch = "wasm32")]
mod lti;
pub mod media;
#[cfg(target_arch = "wasm32")]
mod print;
#[cfg(target_arch = "wasm32")]
mod web_object;

#[cfg(test)]
mod tests;

pub use app::{PlayerApp, Playing, State};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CourseCraft player")
            .with_inner_size([1280.0, 768.0]),
        ..Default::default()
    };
    eframe::run_native(
        "CourseCraft player",
        options,
        Box::new(|cc| {
            coursecraft_render::install_fonts(&cc.egui_ctx);
            Ok(Box::new(PlayerApp::new_desktop(std::env::args().skip(1))))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("coursecraft_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else {
            web_sys::console::error_1(&"coursecraft: canvas #coursecraft_canvas not found".into());
            return;
        };
        if let Err(e) = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(PlayerApp::new_web(cc)))),
            )
            .await
        {
            web_sys::console::error_1(&e);
        }
    });
}
