//! Theme colour palette and OOXML colour modifier resolution.

use std::collections::HashMap;

use coursecraft_course::Color;

use crate::XmlNode;

/// Resolved theme colours and colour-map aliases for a slide.
#[derive(Debug, Clone, Default)]
pub(crate) struct ThemePalette {
    /// Theme colour slots (`dk1`, `lt1`, `dk2`, `lt2`, `accent1`..`accent6`, `hlink`, `folHlink`).
    pub(crate) slots: HashMap<String, (u8, u8, u8)>,
    /// Colour mapping aliases (`tx1` -> `dk1`, `bg1` -> `lt1`, etc.).
    clr_map: HashMap<String, String>,
}

impl ThemePalette {
    /// Parse `a:clrScheme` from a theme XML tree.
    pub(crate) fn from_theme_tree(tree: &XmlNode) -> Self {
        let mut palette = Self::default();
        if let Some(scheme) = tree.find_descendant("clrScheme") {
            for child in &scheme.children {
                if let Some(rgb) = child
                    .child("srgbClr")
                    .and_then(|c| c.attr("val"))
                    .and_then(parse_hex_rgb)
                    .or_else(|| {
                        child
                            .child("sysClr")
                            .and_then(|c| c.attr("lastClr"))
                            .and_then(parse_hex_rgb)
                    })
                {
                    palette.slots.insert(child.name.clone(), rgb);
                }
            }
        }
        palette
    }

    /// Apply `p:clrMap` (from master) and optional `p:clrMapOvr/a:overrideClrMapping`
    /// (from layout or slide).
    pub(crate) fn apply_clr_map(&mut self, node: &XmlNode) {
        let map_node = node
            .find_descendant("overrideClrMapping")
            .or_else(|| node.find_descendant("clrMap"));
        let Some(map_node) = map_node else {
            return;
        };
        for (k, v) in &map_node.attributes {
            let key = k.rsplit(':').next().unwrap_or(k);
            self.clr_map.insert(key.to_string(), v.clone());
        }
    }

    /// Resolve a scheme colour name (`accent1`, `tx1`, `bg1`, `dk1`, etc.) to RGB.
    pub(crate) fn slot_rgb(&self, name: &str) -> Option<(u8, u8, u8)> {
        if let Some(&rgb) = self.slots.get(name) {
            return Some(rgb);
        }
        if let Some(mapped) = self.clr_map.get(name)
            && let Some(&rgb) = self.slots.get(mapped.as_str())
        {
            return Some(rgb);
        }
        let fallback_slot = match name {
            "tx1" => "dk1",
            "bg1" => "lt1",
            "tx2" => "dk2",
            "bg2" => "lt2",
            _ => return None,
        };
        self.slots.get(fallback_slot).copied()
    }
}

/// Resolve a solid fill colour from a parent element containing `<a:solidFill>`.
pub(crate) fn resolve_solid_fill(parent: &XmlNode, theme: &ThemePalette) -> Option<Color> {
    if parent.child("noFill").is_some() {
        return None;
    }
    let solid = parent.child("solidFill")?;
    resolve_first_color_child(solid, theme)
}

/// Resolve the first colour child (`srgbClr`, `schemeClr`, `prstClr`, `sysClr`) of `node`.
pub(crate) fn resolve_first_color_child(node: &XmlNode, theme: &ThemePalette) -> Option<Color> {
    for child in &node.children {
        if let Some(color) = resolve_color_node(child, theme) {
            return Some(color);
        }
    }
    None
}

/// Resolve a single OOXML colour node (`srgbClr`, `schemeClr`, `prstClr`, `sysClr`)
/// with its colour modifiers (`lumMod`, `lumOff`, `tint`, `shade`, `satMod`, `satOff`, `alpha`).
pub(crate) fn resolve_color_node(node: &XmlNode, theme: &ThemePalette) -> Option<Color> {
    let base_rgb = match node.name.as_str() {
        "srgbClr" => node.attr("val").and_then(parse_hex_rgb)?,
        "schemeClr" => {
            let name = node.attr("val")?;
            theme.slot_rgb(name)?
        }
        "sysClr" => {
            node.attr("lastClr")
                .and_then(parse_hex_rgb)
                .or_else(|| match node.attr("val") {
                    Some("windowText") => Some((0, 0, 0)),
                    Some("window") => Some((255, 255, 255)),
                    _ => None,
                })?
        }
        "prstClr" => node.attr("val").and_then(preset_color_rgb)?,
        _ => return None,
    };

    let (r, g, b, alpha) = apply_color_modifiers(base_rgb, node);
    let hex = if let Some(a) = alpha
        && a < 1.0
    {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            r,
            g,
            b,
            (a * 255.0).round().clamp(0.0, 255.0) as u8
        )
    } else {
        format!("#{r:02X}{g:02X}{b:02X}")
    };
    let color = Color(hex);
    color.is_valid().then_some(color)
}

fn apply_color_modifiers(base: (u8, u8, u8), node: &XmlNode) -> (u8, u8, u8, Option<f64>) {
    let mut r = f64::from(base.0) / 255.0;
    let mut g = f64::from(base.1) / 255.0;
    let mut b = f64::from(base.2) / 255.0;
    let mut alpha: Option<f64> = None;

    for child in &node.children {
        let Some(val) = child
            .attr("val")
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
        else {
            continue;
        };
        let f = val / 100_000.0;
        match child.name.as_str() {
            "alpha" => {
                alpha = Some(f.clamp(0.0, 1.0));
            }
            "lumMod" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                (r, g, b) = hsl_to_rgb(h, s, (l * f).clamp(0.0, 1.0));
            }
            "lumOff" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                (r, g, b) = hsl_to_rgb(h, s, (l + f).clamp(0.0, 1.0));
            }
            "satMod" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                (r, g, b) = hsl_to_rgb(h, (s * f).clamp(0.0, 1.0), l);
            }
            "satOff" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                (r, g, b) = hsl_to_rgb(h, (s + f).clamp(0.0, 1.0), l);
            }
            "tint" => {
                let t = f.clamp(0.0, 1.0);
                r = r * t + (1.0 - t);
                g = g * t + (1.0 - t);
                b = b * t + (1.0 - t);
            }
            "shade" => {
                let s = f.clamp(0.0, 1.0);
                r *= s;
                g *= s;
                b *= s;
            }
            _ => {}
        }
    }

    (
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
        alpha,
    )
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-9 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < 1e-9 {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if (max - g).abs() < 1e-9 {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < 1e-9 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue_to_rgb = |t: f64| -> f64 {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (
        hue_to_rgb(h + 1.0 / 3.0),
        hue_to_rgb(h),
        hue_to_rgb(h - 1.0 / 3.0),
    )
}

fn preset_color_rgb(name: &str) -> Option<(u8, u8, u8)> {
    match name {
        "black" => Some((0, 0, 0)),
        "white" => Some((255, 255, 255)),
        "red" => Some((255, 0, 0)),
        "green" => Some((0, 128, 0)),
        "blue" => Some((0, 0, 255)),
        "yellow" => Some((255, 255, 0)),
        "cyan" | "aqua" => Some((0, 255, 255)),
        "magenta" | "fuchsia" => Some((255, 0, 255)),
        "gray" | "grey" => Some((128, 128, 128)),
        "dkGray" | "dkGrey" | "darkGray" => Some((169, 169, 169)),
        "ltGray" | "ltGrey" | "lightGray" => Some((211, 211, 211)),
        "orange" => Some((255, 165, 0)),
        "purple" => Some((128, 0, 128)),
        "navy" => Some((0, 0, 128)),
        "teal" => Some((0, 128, 128)),
        "silver" => Some((192, 192, 192)),
        _ => None,
    }
}
