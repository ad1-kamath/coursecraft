//! Slide-content regions, for the headless layout check.
//!
//! Slide content (the canvas, thumbnails, the preview, the course map) may overlap itself by
//! design, so the layout check must not judge text painted inside it. Every site that draws
//! slide content records the rect it draws into here. The record lives in the egui context for
//! one pass and costs one small push per site.

use egui::{Context, Id, LayerId, Rect};

fn regions_id() -> Id {
    Id::unique("coursecraft_content_regions")
}

/// Record that slide content is painted into `rect` on `layer` during this pass.
pub(crate) fn mark_content_region(ctx: &Context, layer: LayerId, rect: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let slot =
            d.get_temp_mut_or_insert_with(regions_id(), || (pass, Vec::<(LayerId, Rect)>::new()));
        if slot.0 != pass {
            slot.0 = pass;
            slot.1.clear();
        }
        slot.1.push((layer, rect));
    });
}

/// The regions recorded during the current pass.
#[cfg(test)]
pub(crate) fn content_regions(ctx: &Context) -> Vec<(LayerId, Rect)> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| {
        d.get_temp::<(u64, Vec<(LayerId, Rect)>)>(regions_id())
            .filter(|(p, _)| *p == pass)
            .map(|(_, v)| v)
            .unwrap_or_default()
    })
}
