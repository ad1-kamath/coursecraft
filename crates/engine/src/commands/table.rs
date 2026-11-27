//! Table cell, row/column, merge/split, and border commands.

use coursecraft_course::{
    Color, ObjectKind, Paragraph, TableBorders, TableCell, TableRow, TextRun, VAlign,
    align_paragraphs, apply_text_edit, normalize_table_fractions, table_cell_owners,
};
use serde_json::{Value as Json, json};

use super::{bad, find_object_mut, keep_valid, obj, opt, req, slide_mut, text, validate_color};
use crate::{EngineError, Result, Session};

type TableRefs<'a> = (
    &'a mut Vec<f32>,
    &'a mut Vec<TableRow>,
    &'a mut TableBorders,
    &'a mut bool,
);

fn table_mut<'a>(s: &'a mut Session, sid: &str, oid: &str, cmd: &str) -> Result<TableRefs<'a>> {
    let slide = slide_mut(s, sid)?;
    let object = find_object_mut(slide, oid)?;
    match &mut object.kind {
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row,
        } => Ok((columns, rows, borders, header_row)),
        _ => Err(bad(cmd, format!("object {oid:?} is not a table"))),
    }
}

fn normalize_row_heights(rows: &mut [TableRow]) {
    let mut heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    normalize_table_fractions(&mut heights);
    for (row, height) in rows.iter_mut().zip(heights) {
        row.height = height;
    }
}

pub(super) fn table_set_cell(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_cell";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let new_text: Option<String> = opt(C, p, "text")?;
    let runs: Option<Vec<TextRun>> = opt(C, p, "runs")?;
    let paragraphs: Option<Vec<Paragraph>> = opt(C, p, "paragraphs")?;
    let valign: Option<VAlign> = opt(C, p, "valign")?;

    let (_, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let cell = rows
        .get_mut(row)
        .and_then(|line| line.cells.get_mut(column))
        .ok_or_else(|| EngineError::NotFound(format!("no table cell {row},{column}")))?;

    let text_was_set = new_text.is_some();
    if let Some(value) = new_text {
        let value = text(C, value, "text")?;
        let old_plain = if cell.runs.is_empty() {
            cell.text.clone()
        } else {
            cell.runs.iter().map(|run| run.text.as_str()).collect()
        };
        if runs.is_none() {
            cell.runs = apply_text_edit(&cell.runs, &old_plain, &value);
            cell.paragraphs = align_paragraphs(&old_plain, &cell.paragraphs, &value);
        }
        cell.text = value;
    }
    if let Some(runs) = runs {
        if runs.len() > 1000 {
            return Err(bad(C, "exceeded 1000 text runs"));
        }
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        if !text_was_set {
            cell.text = joined;
        }
        cell.runs = runs;
    }
    if let Some(paragraphs) = paragraphs {
        cell.paragraphs = paragraphs;
    }
    if let Some(valign) = valign {
        cell.valign = valign;
    }
    if let Some(value) = obj(C, p)?.get("fill") {
        cell.fill = match value {
            Json::Null => None,
            _ => {
                let fill: Color = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("`fill`: {e}")))?;
                validate_color(C, &fill)?;
                Some(fill)
            }
        };
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_insert_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.insert_row";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if rows.len() >= coursecraft_course::TABLE_MAX_AXIS {
        return Err(bad(C, "a table may have at most 50 rows"));
    }
    let at = index.unwrap_or(rows.len());
    if at > rows.len() {
        return Err(bad(C, "row index is past the end of the table"));
    }
    // Extend spans that cross the insertion line before adding the row.
    for (row_index, row) in rows.iter_mut().enumerate() {
        for cell in &mut row.cells {
            let span = usize::from(cell.row_span.max(1));
            if row_index < at && at < row_index.saturating_add(span) {
                cell.row_span = u8::try_from(span.saturating_add(1)).unwrap_or(u8::MAX);
            }
        }
    }
    let mut heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    normalize_table_fractions(&mut heights);
    let old_count = heights.len() as f32;
    let new_height = 1.0 / (old_count + 1.0);
    for (row, height) in rows.iter_mut().zip(heights) {
        row.height = height * old_count / (old_count + 1.0);
    }
    let cells = vec![TableCell::default(); columns.len()];
    rows.insert(
        at.min(rows.len()),
        TableRow {
            height: new_height,
            cells,
        },
    );
    normalize_row_heights(rows);
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": at }))
}

pub(super) fn table_delete_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.delete_row";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let (_, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if rows.len() <= 1 {
        return Err(bad(C, "a table needs at least one row"));
    }
    if index >= rows.len() {
        return Err(EngineError::NotFound(format!("no table row {index}")));
    }
    for (row_index, row) in rows.iter_mut().enumerate() {
        for cell in &mut row.cells {
            let span = usize::from(cell.row_span.max(1));
            if row_index < index && index < row_index.saturating_add(span) {
                cell.row_span = u8::try_from(span.saturating_sub(1)).unwrap_or(1).max(1);
            }
        }
    }
    rows.remove(index);
    let max_span = rows.len().max(1);
    for row in rows.iter_mut() {
        for cell in &mut row.cells {
            cell.row_span = cell
                .row_span
                .min(u8::try_from(max_span).unwrap_or(u8::MAX))
                .max(1);
        }
    }
    normalize_row_heights(rows);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_insert_column(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.insert_column";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if columns.len() >= coursecraft_course::TABLE_MAX_AXIS {
        return Err(bad(C, "a table may have at most 50 columns"));
    }
    let at = index.unwrap_or(columns.len());
    if at > columns.len() {
        return Err(bad(C, "column index is past the end of the table"));
    }
    for row in rows.iter_mut() {
        for (column, cell) in row.cells.iter_mut().enumerate() {
            let span = usize::from(cell.col_span.max(1));
            if column < at && at < column.saturating_add(span) {
                cell.col_span = u8::try_from(span.saturating_add(1)).unwrap_or(u8::MAX);
            }
        }
        row.cells
            .insert(at.min(row.cells.len()), TableCell::default());
    }
    let mut widths = std::mem::take(columns);
    normalize_table_fractions(&mut widths);
    let old_count = widths.len() as f32;
    let new_width = 1.0 / (old_count + 1.0);
    for width in &mut widths {
        *width *= old_count / (old_count + 1.0);
    }
    let at = at.min(widths.len());
    widths.insert(at, new_width);
    normalize_table_fractions(&mut widths);
    *columns = widths;
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": at }))
}

pub(super) fn table_delete_column(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.delete_column";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if columns.len() <= 1 {
        return Err(bad(C, "a table needs at least one column"));
    }
    if index >= columns.len() {
        return Err(EngineError::NotFound(format!("no table column {index}")));
    }
    for row in rows.iter_mut() {
        for (column, cell) in row.cells.iter_mut().enumerate() {
            let span = usize::from(cell.col_span.max(1));
            if column < index && index < column.saturating_add(span) {
                cell.col_span = u8::try_from(span.saturating_sub(1)).unwrap_or(1).max(1);
            }
        }
        if index < row.cells.len() {
            row.cells.remove(index);
        }
        let max_span = row.cells.len().max(1);
        for cell in &mut row.cells {
            cell.col_span = cell
                .col_span
                .min(u8::try_from(max_span).unwrap_or(u8::MAX))
                .max(1);
        }
    }
    columns.remove(index);
    normalize_table_fractions(columns);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_resize(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.resize";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let new_columns: Option<Vec<f32>> = opt(C, p, "columns")?;
    let new_rows: Option<Vec<f32>> = opt(C, p, "rows")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if let Some(mut values) = new_columns {
        if values.len() != columns.len() || values.is_empty() {
            return Err(bad(C, "columns must have one fraction per column"));
        }
        if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
            return Err(bad(C, "column fractions must be finite and above 0"));
        }
        normalize_table_fractions(&mut values);
        *columns = values;
    }
    if let Some(mut values) = new_rows {
        if values.len() != rows.len() || values.is_empty() {
            return Err(bad(C, "rows must have one fraction per row"));
        }
        if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
            return Err(bad(C, "row fractions must be finite and above 0"));
        }
        normalize_table_fractions(&mut values);
        for (row, height) in rows.iter_mut().zip(values) {
            row.height = height;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_merge(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.merge";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let row_count: usize = req(C, p, "rows")?;
    let column_count: usize = req(C, p, "columns")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let row_end = row.saturating_add(row_count);
    let column_end = column.saturating_add(column_count);
    if row_count == 0 || column_count == 0 || row_end > rows.len() || column_end > columns.len() {
        return Err(bad(C, "the merged range must lie inside the table"));
    }
    let owners = table_cell_owners(rows, columns.len());
    if owners
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
        != Some((row, column))
    {
        return Err(bad(C, "the merge anchor is covered by another merge"));
    }
    // A merge may swallow merges that lie wholly inside the new range. A cell
    // that another merge owns from outside the range would be covered twice.
    for covered_row in row..row_end {
        for covered_column in column..column_end {
            let Some((owner_row, owner_column)) = owners
                .get(covered_row)
                .and_then(|line| line.get(covered_column))
                .copied()
                .flatten()
            else {
                continue;
            };
            let (span_rows, span_columns) = rows
                .get(owner_row)
                .and_then(|line| line.cells.get(owner_column))
                .map_or((1, 1), |cell| {
                    (
                        usize::from(cell.row_span.max(1)),
                        usize::from(cell.col_span.max(1)),
                    )
                });
            let inside = owner_row >= row
                && owner_column >= column
                && owner_row.saturating_add(span_rows) <= row_end
                && owner_column.saturating_add(span_columns) <= column_end;
            if !inside {
                return Err(bad(
                    C,
                    "the merged range overlaps another merged cell; split it first",
                ));
            }
        }
    }
    for covered_row in row..row_end {
        for covered_column in column..column_end {
            let Some(cell) = rows
                .get_mut(covered_row)
                .and_then(|line| line.cells.get_mut(covered_column))
            else {
                continue;
            };
            if (covered_row, covered_column) != (row, column) {
                *cell = TableCell::default();
            }
        }
    }
    let Some(anchor) = rows
        .get_mut(row)
        .and_then(|line| line.cells.get_mut(column))
    else {
        return Err(bad(C, "the merge anchor is missing"));
    };
    anchor.row_span = u8::try_from(row_count).unwrap_or(u8::MAX).max(1);
    anchor.col_span = u8::try_from(column_count).unwrap_or(u8::MAX).max(1);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_split(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.split";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let owners = table_cell_owners(rows, columns.len());
    let Some(owner) = owners
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
    else {
        return Err(EngineError::NotFound(format!(
            "no table cell {row},{column}"
        )));
    };
    let Some(cell) = rows
        .get_mut(owner.0)
        .and_then(|line| line.cells.get_mut(owner.1))
    else {
        return Err(EngineError::NotFound(format!(
            "no table cell {row},{column}"
        )));
    };
    cell.row_span = 1;
    cell.col_span = 1;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_set_borders(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_borders";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let has_outer = obj(C, p)?.contains_key("outer");
    let has_inner = obj(C, p)?.contains_key("inner");
    let outer: Option<coursecraft_course::Outline> =
        if has_outer { opt(C, p, "outer")? } else { None };
    let inner: Option<coursecraft_course::Outline> =
        if has_inner { opt(C, p, "inner")? } else { None };
    let (_, _, borders, _) = table_mut(s, &sid, &oid, C)?;
    if has_outer {
        borders.outer = outer;
    }
    if has_inner {
        borders.inner = inner;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

pub(super) fn table_set_header_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_header_row";
    let was_valid = s.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let header_row: bool = req(C, p, "header_row")?;
    let (_, _, _, current) = table_mut(s, &sid, &oid, C)?;
    *current = header_row;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}
