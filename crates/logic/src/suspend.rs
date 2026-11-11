//! Suspend data encoding and decoding.
//!
//! SCORM 1.2 and modern LMSs persist learner progress across sessions in
//! `cmi.suspend_data`. Because SCORM 1.2 caps this field at 4096 characters,
//! CourseCraft uses a compact binary format encoded as unpadded base64url
//! with a `cc1:` prefix.
//!
//! # Binary format specification (`cc1:`)
//!
//! The body following the [`SUSPEND_PREFIX`] is unpadded base64url (RFC 4648 §5).
//! The binary payload uses unsigned LEB128 varints for integer fields and counts.
//!
//! Field order (stable storage format):
//!
//! 0. `draw_seed` (only when `course.banks` is not empty): varint (`u32`).
//!    All slide indices below are play-order indices of the expanded order
//!    produced by `play_order(course, draw_seed)`. A course without banks
//!    writes no seed and encodes byte-for-byte as before.
//!
//! 0b. `quiz_attempts` (only when `course.quiz` is set or `course.banks` is
//!    not empty): varint (`u32`), directly after the draw-seed position.
//!    A course with neither writes no attempts and encodes byte-for-byte as
//!    before.
//!
//! 1. `slide`: varint
//!    Play-order index of the current slide.
//!
//! 2. `visited_slides`:
//!    - `count`: varint
//!    - for each visited slide:
//!      - `slide_index`: varint (play-order index)
//!
//! 3. `modified_object_states`:
//!    - `slide_count`: varint (number of slides with modified objects)
//!    - for each slide:
//!      - `slide_index`: varint (play-order index)
//!      - `object_count`: varint (number of modified objects on this slide)
//!      - for each modified object:
//!        - `object_index`: varint (index in base objects, then layer objects in order)
//!        - `state_index`: varint (0..6 for [`BUILTIN_STATES`], then 7+ for object's
//!          non-builtin states in model order)
//!
//! 4. `variables`:
//!    - `var_count`: varint
//!    - for each modified variable:
//!      - `var_index`: varint (index in `course.variables`)
//!      - `value_tag`: u8 (0 = Bool, 1 = Number, 2 = Text)
//!      - value payload:
//!        - Bool: 1 byte (0 = false, 1 = true)
//!        - Number: 8 bytes little-endian (`f64`)
//!        - Text: varint length followed by UTF-8 bytes
//!
//! 5. `answers`:
//!    - `count`: varint
//!    - for each answer:
//!      - `slide_index`: varint (play-order index)
//!      - `correct`: 1 byte (0 = false, 1 = true)
//!      - `attempts`: varint (`u32`)
//!      - `points`: varint (`u32`)

use std::collections::BTreeMap;
use std::fmt;

use coursecraft_course::{BUILTIN_STATES, Course, Object, Slide, Value};

use crate::{Answer, PlayerState};

/// Prefix required for CourseCraft v1 suspend data strings.
pub const SUSPEND_PREFIX: &str = "cc1:";

const BASE64URL_TABLE: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Errors that can occur when decoding suspend data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuspendError {
    /// String does not start with [`SUSPEND_PREFIX`].
    InvalidPrefix,
    /// Base64url data is malformed, invalid length, or contains invalid characters.
    InvalidBase64,
    /// Data ended unexpectedly.
    Truncated,
    /// Data contains trailing bytes after all expected fields.
    TrailingBytes,
    /// LEB128 varint is malformed or overflows 64 bits.
    InvalidVarint,
    /// Slide index is out of range for the course.
    InvalidSlideIndex,
    /// Object index is out of range for the slide.
    InvalidObjectIndex,
    /// State index is out of range for the object.
    InvalidStateIndex,
    /// Variable index is out of range for the course.
    InvalidVariableIndex,
    /// Variable type does not match course definition.
    VariableTypeMismatch,
    /// Value exceeds the supported integer or float range.
    ValueOutOfRange,
    /// Corrupt data encountered (e.g. invalid bool or float byte sequence).
    CorruptData,
}

impl fmt::Display for SuspendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPrefix => write!(f, "suspend data does not start with {SUSPEND_PREFIX}"),
            Self::InvalidBase64 => write!(f, "invalid base64url data in suspend string"),
            Self::Truncated => write!(f, "unexpected end of suspend data"),
            Self::TrailingBytes => write!(f, "trailing bytes in suspend data"),
            Self::InvalidVarint => write!(f, "invalid varint in suspend data"),
            Self::InvalidSlideIndex => write!(f, "slide index out of range for this course"),
            Self::InvalidObjectIndex => write!(f, "object index out of range for this slide"),
            Self::InvalidStateIndex => write!(f, "state index out of range for this object"),
            Self::InvalidVariableIndex => {
                write!(f, "variable index out of range for this course")
            }
            Self::VariableTypeMismatch => {
                write!(f, "variable type does not match course definition")
            }
            Self::ValueOutOfRange => write!(f, "value exceeds supported range"),
            Self::CorruptData => write!(f, "corrupted suspend data payload"),
        }
    }
}

impl std::error::Error for SuspendError {}

fn base64_char(val: u8) -> char {
    let idx = (val & 0x3f) as usize;
    BASE64URL_TABLE
        .get(idx)
        .copied()
        .map(|b| b as char)
        .unwrap_or('A')
}

fn encode_base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(4).div_ceil(3));
    let (chunks, remainder) = bytes.as_chunks::<3>();
    for &[b0, b1, b2] in chunks {
        out.push(base64_char(b0 >> 2));
        out.push(base64_char(((b0 & 0x03) << 4) | (b1 >> 4)));
        out.push(base64_char(((b1 & 0x0f) << 2) | (b2 >> 6)));
        out.push(base64_char(b2 & 0x3f));
    }
    match remainder.len() {
        1 => {
            let b0 = *remainder.first().unwrap_or(&0);
            out.push(base64_char(b0 >> 2));
            out.push(base64_char((b0 & 0x03) << 4));
        }
        2 => {
            let b0 = *remainder.first().unwrap_or(&0);
            let b1 = *remainder.get(1).unwrap_or(&0);
            out.push(base64_char(b0 >> 2));
            out.push(base64_char(((b0 & 0x03) << 4) | (b1 >> 4)));
            out.push(base64_char((b1 & 0x0f) << 2));
        }
        _ => {}
    }
    out
}

fn base64_val(b: u8) -> Result<u8, SuspendError> {
    match b {
        b'A'..=b'Z' => Ok(b - b'A'),
        b'a'..=b'z' => Ok(b - b'a' + 26),
        b'0'..=b'9' => Ok(b - b'0' + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err(SuspendError::InvalidBase64),
    }
}

fn decode_base64url(s: &str) -> Result<Vec<u8>, SuspendError> {
    if s.len() % 4 == 1 {
        return Err(SuspendError::InvalidBase64);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity((bytes.len().saturating_mul(3)) / 4);
    let (chunks, rem) = bytes.as_chunks::<4>();
    for &[b0, b1, b2, b3] in chunks {
        let c0 = base64_val(b0)?;
        let c1 = base64_val(b1)?;
        let c2 = base64_val(b2)?;
        let c3 = base64_val(b3)?;
        out.push((c0 << 2) | (c1 >> 4));
        out.push(((c1 & 0x0f) << 4) | (c2 >> 2));
        out.push(((c2 & 0x03) << 6) | c3);
    }
    match rem.len() {
        2 => {
            let c0 = base64_val(*rem.first().unwrap_or(&0))?;
            let c1 = base64_val(*rem.get(1).unwrap_or(&0))?;
            out.push((c0 << 2) | (c1 >> 4));
        }
        3 => {
            let c0 = base64_val(*rem.first().unwrap_or(&0))?;
            let c1 = base64_val(*rem.get(1).unwrap_or(&0))?;
            let c2 = base64_val(*rem.get(2).unwrap_or(&0))?;
            out.push((c0 << 2) | (c1 >> 4));
            out.push(((c1 & 0x0f) << 4) | (c2 >> 2));
        }
        _ => {}
    }
    Ok(out)
}

fn encode_varint(mut val: u64, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7f) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
            buf.push(byte);
        } else {
            buf.push(byte);
            break;
        }
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn is_empty(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn read_byte(&mut self) -> Result<u8, SuspendError> {
        let byte = self
            .bytes
            .get(self.pos)
            .copied()
            .ok_or(SuspendError::Truncated)?;
        self.pos = self.pos.saturating_add(1);
        Ok(byte)
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8], SuspendError> {
        if self.remaining() < len {
            return Err(SuspendError::Truncated);
        }
        let end = self.pos.saturating_add(len);
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(SuspendError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    fn read_varint(&mut self) -> Result<u64, SuspendError> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            let byte = self.read_byte()?;
            let val = (byte & 0x7f) as u64;
            if shift >= 63 && (shift > 63 || val > 1 || (byte & 0x80) != 0) {
                return Err(SuspendError::InvalidVarint);
            }
            let shifted = val.checked_shl(shift).ok_or(SuspendError::InvalidVarint)?;
            result = result
                .checked_add(shifted)
                .ok_or(SuspendError::InvalidVarint)?;
            if (byte & 0x80) == 0 {
                return Ok(result);
            }
            shift = shift.saturating_add(7);
        }
    }

    fn read_bounded_count(&mut self) -> Result<usize, SuspendError> {
        let count = self.read_varint()?;
        let rem = self.remaining() as u64;
        if count > rem {
            return Err(SuspendError::Truncated);
        }
        usize::try_from(count).map_err(|_| SuspendError::ValueOutOfRange)
    }
}

fn slide_objects(slide: &Slide) -> impl Iterator<Item = &Object> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
}

fn state_to_index(obj: &Object, state: &str) -> Option<u64> {
    if let Some(pos) = BUILTIN_STATES.iter().position(|&b| b == state) {
        return Some(pos as u64);
    }
    let custom_pos = obj
        .states
        .iter()
        .filter(|s| !BUILTIN_STATES.contains(&s.name.as_str()))
        .position(|s| s.name == state)?;
    let idx = 7_u64.checked_add(custom_pos as u64)?;
    Some(idx)
}

fn index_to_state(obj: &Object, idx: u64) -> Result<&str, SuspendError> {
    if idx < 7 {
        let idx_usize = usize::try_from(idx).map_err(|_| SuspendError::InvalidStateIndex)?;
        return BUILTIN_STATES
            .get(idx_usize)
            .copied()
            .ok_or(SuspendError::InvalidStateIndex);
    }
    let custom_idx =
        usize::try_from(idx.saturating_sub(7)).map_err(|_| SuspendError::InvalidStateIndex)?;
    obj.states
        .iter()
        .map(|s| s.name.as_str())
        .filter(|name| !BUILTIN_STATES.contains(name))
        .nth(custom_idx)
        .ok_or(SuspendError::InvalidStateIndex)
}

fn rebuild_initial_states(slide: &Slide) -> BTreeMap<String, String> {
    slide_objects(slide)
        .map(|o| (o.id.clone(), o.initial_state.clone()))
        .collect()
}

/// Encode durable player state into a compact suspend string.
#[must_use]
pub fn encode_suspend(course: &Course, st: &PlayerState) -> String {
    let mut buf = Vec::new();

    let slides = crate::draw::play_order(course, st.draw_seed);

    // 0. Draw seed (courses with banks only).
    if !course.banks.is_empty() {
        encode_varint(u64::from(st.draw_seed), &mut buf);
    }

    // 0b. Quiz attempts (courses with a quiz or with banks only).
    if course.quiz.is_some() || !course.banks.is_empty() {
        encode_varint(u64::from(st.quiz_attempts), &mut buf);
    }

    // 1. Current slide index. A lightbox is not saved: resume shows the base.
    let slide = st.lightbox.as_ref().map_or(st.slide, |lb| lb.base_slide);
    encode_varint(slide as u64, &mut buf);

    // 2. Visited slides (sorted play-order indices)
    let mut visited: Vec<u64> = slides
        .iter()
        .enumerate()
        .filter(|(_, s)| st.visited.contains(&s.id))
        .map(|(i, _)| i as u64)
        .collect();
    visited.sort_unstable();
    encode_varint(visited.len() as u64, &mut buf);
    for idx in visited {
        encode_varint(idx, &mut buf);
    }

    // 3. Modified object states
    let mut modified_slides = Vec::new();
    for (slide_idx, slide) in slides.iter().enumerate() {
        if let Some(slide_states) = st.object_states.get(&slide.id) {
            let mut diffs = Vec::new();
            for (obj_idx, obj) in slide_objects(slide).enumerate() {
                if let Some(cur_state) = slide_states.get(&obj.id)
                    && cur_state != &obj.initial_state
                    && let Some(state_idx) = state_to_index(obj, cur_state)
                {
                    diffs.push((obj_idx as u64, state_idx));
                }
            }
            if !diffs.is_empty() {
                modified_slides.push((slide_idx as u64, diffs));
            }
        }
    }
    encode_varint(modified_slides.len() as u64, &mut buf);
    for (s_idx, diffs) in modified_slides {
        encode_varint(s_idx, &mut buf);
        encode_varint(diffs.len() as u64, &mut buf);
        for (o_idx, st_idx) in diffs {
            encode_varint(o_idx, &mut buf);
            encode_varint(st_idx, &mut buf);
        }
    }

    // 4. Variables (only those differing from course initial definitions)
    let mut modified_vars = Vec::new();
    for (var_idx, var_def) in course.variables.iter().enumerate() {
        if let Some(cur_val) = st.variables.get(&var_def.name)
            && cur_val != &var_def.value
        {
            modified_vars.push((var_idx as u64, cur_val));
        }
    }
    encode_varint(modified_vars.len() as u64, &mut buf);
    for (v_idx, val) in modified_vars {
        encode_varint(v_idx, &mut buf);
        match val {
            Value::Bool(b) => {
                buf.push(0);
                buf.push(if *b { 1 } else { 0 });
            }
            Value::Number(n) => {
                buf.push(1);
                buf.extend_from_slice(&n.to_le_bytes());
            }
            Value::Text(s) => {
                buf.push(2);
                encode_varint(s.len() as u64, &mut buf);
                buf.extend_from_slice(s.as_bytes());
            }
        }
    }

    // 5. Answers
    let mut answers = Vec::new();
    for (slide_idx, slide) in slides.iter().enumerate() {
        if let Some(ans) = st.answers.get(&slide.id) {
            answers.push((slide_idx as u64, ans));
        }
    }
    encode_varint(answers.len() as u64, &mut buf);
    for (s_idx, ans) in answers {
        encode_varint(s_idx, &mut buf);
        buf.push(if ans.correct { 1 } else { 0 });
        encode_varint(u64::from(ans.attempts), &mut buf);
        encode_varint(u64::from(ans.points), &mut buf);
    }

    let b64 = encode_base64url(&buf);
    format!("{SUSPEND_PREFIX}{b64}")
}

/// Decode a suspend string into a player state ready for resume.
///
/// Starts from defaults with variables initialized from the course, then applies
/// decoded slide index, visited slides, object state differences, variables and answers.
/// Returns the state with `started: false`.
///
/// # Errors
/// Returns [`SuspendError`] if the data has an invalid prefix, malformed
/// base64, corrupted varints or values, out-of-range indices, or trailing bytes.
pub fn decode_suspend(course: &Course, text: &str) -> Result<PlayerState, SuspendError> {
    if !text.starts_with(SUSPEND_PREFIX) {
        return Err(SuspendError::InvalidPrefix);
    }
    let payload = text
        .get(SUSPEND_PREFIX.len()..)
        .ok_or(SuspendError::InvalidPrefix)?;
    let raw = decode_base64url(payload)?;
    let mut cursor = Cursor::new(&raw);

    // 0. Draw seed (courses with banks only).
    let draw_seed: u32 = if course.banks.is_empty() {
        0
    } else {
        let seed = cursor.read_varint()?;
        u32::try_from(seed).map_err(|_| SuspendError::ValueOutOfRange)?
    };

    // 0b. Quiz attempts (courses with a quiz or with banks only).
    let quiz_attempts: u32 = if course.quiz.is_some() || !course.banks.is_empty() {
        let attempts = cursor.read_varint()?;
        u32::try_from(attempts).map_err(|_| SuspendError::ValueOutOfRange)?
    } else {
        0
    };

    let slides = crate::draw::play_order(course, draw_seed);

    // 1. Current slide index
    let slide_idx = cursor.read_varint()?;
    let slide_usize = usize::try_from(slide_idx).map_err(|_| SuspendError::InvalidSlideIndex)?;
    if slide_usize >= slides.len() {
        return Err(SuspendError::InvalidSlideIndex);
    }

    let mut st = PlayerState {
        slide: slide_usize,
        draw_seed,
        quiz_attempts,
        variables: course
            .variables
            .iter()
            .map(|v| (v.name.clone(), v.value.clone()))
            .collect(),
        ..PlayerState::default()
    };

    // 2. Visited slides
    let visited_count = cursor.read_bounded_count()?;
    for _ in 0..visited_count {
        let v_idx = cursor.read_varint()?;
        let v_usize = usize::try_from(v_idx).map_err(|_| SuspendError::InvalidSlideIndex)?;
        let v_slide = slides.get(v_usize).ok_or(SuspendError::InvalidSlideIndex)?;
        st.visited.insert(v_slide.id.clone());
    }

    // Rebuild full initial states for every visited slide
    for slide_id in &st.visited {
        if let Some(slide) = course.play_slide(slide_id) {
            st.object_states
                .insert(slide.id.clone(), rebuild_initial_states(slide));
        }
    }

    // 3. Modified object states
    let mod_slide_count = cursor.read_bounded_count()?;
    for _ in 0..mod_slide_count {
        let s_idx = cursor.read_varint()?;
        let s_usize = usize::try_from(s_idx).map_err(|_| SuspendError::InvalidSlideIndex)?;
        let slide = slides.get(s_usize).ok_or(SuspendError::InvalidSlideIndex)?;
        let all_objects: Vec<&Object> = slide_objects(slide).collect();

        let obj_count = cursor.read_bounded_count()?;
        let slide_map = st
            .object_states
            .entry(slide.id.clone())
            .or_insert_with(|| rebuild_initial_states(slide));

        for _ in 0..obj_count {
            let o_idx = cursor.read_varint()?;
            let o_usize = usize::try_from(o_idx).map_err(|_| SuspendError::InvalidObjectIndex)?;
            let obj = all_objects
                .get(o_usize)
                .ok_or(SuspendError::InvalidObjectIndex)?;
            let st_idx = cursor.read_varint()?;
            let state_name = index_to_state(obj, st_idx)?;
            slide_map.insert(obj.id.clone(), state_name.to_owned());
        }
    }

    // 4. Variables
    let var_count = cursor.read_bounded_count()?;
    for _ in 0..var_count {
        let v_idx = cursor.read_varint()?;
        let v_usize = usize::try_from(v_idx).map_err(|_| SuspendError::InvalidVariableIndex)?;
        let var_def = course
            .variables
            .get(v_usize)
            .ok_or(SuspendError::InvalidVariableIndex)?;
        let tag = cursor.read_byte()?;
        let val = match tag {
            0 => {
                let b = cursor.read_byte()?;
                match b {
                    0 => Value::Bool(false),
                    1 => Value::Bool(true),
                    _ => return Err(SuspendError::CorruptData),
                }
            }
            1 => {
                let bytes = cursor.read_exact(8)?;
                let arr: [u8; 8] = match bytes.try_into() {
                    Ok(a) => a,
                    Err(_) => return Err(SuspendError::CorruptData),
                };
                let n = f64::from_le_bytes(arr);
                if !n.is_finite() {
                    return Err(SuspendError::CorruptData);
                }
                Value::Number(n)
            }
            2 => {
                let len = cursor.read_bounded_count()?;
                let bytes = cursor.read_exact(len)?;
                let s = std::str::from_utf8(bytes).map_err(|_| SuspendError::CorruptData)?;
                Value::Text(s.to_owned())
            }
            _ => return Err(SuspendError::CorruptData),
        };

        if val.kind() != var_def.value.kind() {
            return Err(SuspendError::VariableTypeMismatch);
        }
        st.variables.insert(var_def.name.clone(), val);
    }

    // 5. Answers
    let ans_count = cursor.read_bounded_count()?;
    for _ in 0..ans_count {
        let s_idx = cursor.read_varint()?;
        let s_usize = usize::try_from(s_idx).map_err(|_| SuspendError::InvalidSlideIndex)?;
        let slide = slides.get(s_usize).ok_or(SuspendError::InvalidSlideIndex)?;
        let correct_byte = cursor.read_byte()?;
        let correct = match correct_byte {
            0 => false,
            1 => true,
            _ => return Err(SuspendError::CorruptData),
        };
        let attempts = cursor.read_varint()?;
        let attempts_u32 = u32::try_from(attempts).map_err(|_| SuspendError::ValueOutOfRange)?;
        let points = cursor.read_varint()?;
        let points_u32 = u32::try_from(points).map_err(|_| SuspendError::ValueOutOfRange)?;

        st.answers.insert(
            slide.id.clone(),
            Answer {
                correct,
                attempts: attempts_u32,
                points: points_u32,
            },
        );
    }

    if !cursor.is_empty() {
        return Err(SuspendError::TrailingBytes);
    }

    Ok(st)
}
