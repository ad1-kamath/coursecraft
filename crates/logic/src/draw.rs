//! Random draw from question banks at play time.
//!
//! A scene slide carrying `draw` is replaced by `count` slides from its bank.
//! The expansion is a pure function of the course and a draw seed, so preview,
//! published packages and resume all show the same questions.

use std::collections::BTreeMap;

use coursecraft_course::{Course, Slide};

use crate::run;

/// Play order with every draw slide expanded.
///
/// Slides without `draw` are kept in course order. Each bank is shuffled once
/// with a seed mixed from `seed` and the bank id; draws take from the front of
/// that shuffled list in course order, so two draws from one bank never pick
/// the same question. A missing or short bank draws what exists and never panics.
pub(crate) fn play_order(course: &Course, seed: u32) -> Vec<&Slide> {
    let mut out: Vec<&Slide> = Vec::new();
    // Per bank: (shuffled ids, how many already taken from the front).
    let mut per_bank: BTreeMap<String, (Vec<String>, usize)> = BTreeMap::new();

    for slide in course.slides() {
        let Some(draw) = &slide.draw else {
            out.push(slide);
            continue;
        };
        let Some(bank) = course.bank(&draw.bank) else {
            continue;
        };
        let bank_id = bank.id.clone();
        let (shuffled, taken) = per_bank.entry(bank_id.clone()).or_insert_with(|| {
            let ids: Vec<String> = bank.slides.iter().map(|s| s.id.clone()).collect();
            let mixed = run::seed_for(&bank_id, seed);
            (run::shuffled_ids(&ids, mixed), 0)
        });
        let want = draw.count as usize;
        let start = *taken;
        let end = start.saturating_add(want).min(shuffled.len());
        let picks: Vec<String> = shuffled
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect();
        *taken = end;
        for id in picks {
            if let Some(picked) = course.bank_slide(&id) {
                out.push(picked);
            }
        }
    }
    out
}

/// Position of `id` in the expanded order: in `slides` if present, else the
/// first drawn question of the draw slide `id`, else `None`.
pub(crate) fn position_in_expanded(course: &Course, slides: &[&Slide], id: &str) -> Option<usize> {
    if let Some(pos) = slides.iter().position(|s| s.id == id) {
        return Some(pos);
    }
    // Not in the expanded order. Only a draw slide has an indirect position.
    let mut cursor: usize = 0;
    let mut per_bank_taken: BTreeMap<String, usize> = BTreeMap::new();
    for slide in course.slides() {
        let Some(draw) = &slide.draw else {
            if slide.id == id {
                return None;
            }
            cursor = cursor.saturating_add(1);
            continue;
        };
        let bank_size = course.bank(&draw.bank).map_or(0, |b| b.slides.len());
        let already = per_bank_taken.get(&draw.bank).copied().unwrap_or(0);
        let remaining = bank_size.saturating_sub(already);
        let taken_now = (draw.count as usize).min(remaining);
        if slide.id == id {
            if taken_now == 0 {
                return None;
            }
            return Some(cursor);
        }
        cursor = cursor.saturating_add(taken_now);
        per_bank_taken.insert(draw.bank.clone(), already.saturating_add(taken_now));
    }
    None
}

/// Expanded start position: `start_slide` mapped through the expansion,
/// otherwise the first slide (0).
pub(crate) fn expanded_start(course: &Course, slides: &[&Slide]) -> usize {
    match &course.start_slide {
        Some(id) => position_in_expanded(course, slides, id).unwrap_or(0),
        None => 0,
    }
}
