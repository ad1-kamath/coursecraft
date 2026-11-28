//! The post-pass on an item's painted meshes: animation scale, rotation and flips, the
//! shaped-reveal mask and the colour tint.
//!
//! Everything an item paints is tessellated into meshes (text and images become textured
//! meshes), so the same pass turns, scales, masks and tints shapes, text, pictures and video
//! alike, on native, wasm and the offscreen rasteriser. Masks clip the triangles against
//! convex polygons (Sutherland–Hodgman), interpolating position, texture coordinate and colour,
//! so no render-to-texture is needed.

use coursecraft_logic::Tint;
use egui::emath::Rot2;
use egui::epaint::{Mesh, Vertex};
use egui::{Color32, Pos2, Rect, pos2};

/// How to change the meshes an item painted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MeshPost<'a> {
    /// Pivot for scale, rotation and flips (the item's frame centre, on screen).
    pub centre: Pos2,
    /// Horizontal and vertical scale, applied first, in the item's own axes.
    pub scale: (f32, f32),
    /// Clockwise degrees, applied after the scale.
    pub rotation: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Screen-space translation applied after scale/rotation/flips and before mask.
    pub offset: egui::Vec2,
    /// Screen-space convex polygons; only their inside is kept. Applied after the transform.
    pub mask: Option<&'a [Vec<Pos2>]>,
    pub tint: Option<Tint>,
    /// Multiplies every colour (premultiplied), for whole-slide fades in transitions.
    pub opacity: f32,
}

impl MeshPost<'_> {
    /// Only a rotation and flips.
    pub(crate) const fn turn(centre: Pos2, rotation: f32, flip_h: bool, flip_v: bool) -> Self {
        Self {
            centre,
            scale: (1.0, 1.0),
            rotation,
            flip_h,
            flip_v,
            offset: egui::Vec2::ZERO,
            mask: None,
            tint: None,
            opacity: 1.0,
        }
    }

    /// The pass for the text of a horizontally flipped item: not mirrored
    /// horizontally. The item's own pass rotates and then mirrors, which turns the
    /// object the other way on screen; text that skips the mirror must be turned
    /// by the opposite angle to stay glued to its object.
    pub(crate) fn unmirrored_text(self) -> Self {
        Self {
            rotation: -self.rotation,
            flip_h: false,
            ..self
        }
    }

    /// Whether the scale changes anything.
    pub(crate) fn scales(&self) -> bool {
        self.scale != (1.0, 1.0)
    }

    /// Apply the pass to `mesh`, returning the result (empty when the mask hides it all).
    pub(crate) fn apply(&self, mut mesh: Mesh) -> Mesh {
        if self.scales() {
            let (sx, sy) = self.scale;
            let c = self.centre;
            for v in &mut mesh.vertices {
                v.pos = pos2(c.x + (v.pos.x - c.x) * sx, c.y + (v.pos.y - c.y) * sy);
            }
        }
        let radians = if self.rotation.is_finite() {
            self.rotation.to_radians()
        } else {
            0.0
        };
        if radians != 0.0 {
            mesh.rotate(Rot2::from_angle(radians), self.centre);
        }
        if self.flip_h || self.flip_v {
            let c = self.centre;
            for vertex in &mut mesh.vertices {
                if self.flip_h {
                    vertex.pos.x = 2.0 * c.x - vertex.pos.x;
                }
                if self.flip_v {
                    vertex.pos.y = 2.0 * c.y - vertex.pos.y;
                }
            }
            if self.flip_h != self.flip_v {
                for triangle in mesh.indices.as_chunks_mut::<3>().0 {
                    let [a, _, c] = triangle;
                    std::mem::swap(a, c);
                }
            }
        }
        if self.offset != egui::Vec2::ZERO && self.offset.x.is_finite() && self.offset.y.is_finite()
        {
            for v in &mut mesh.vertices {
                v.pos += self.offset;
            }
        }
        if let Some(polys) = self.mask {
            mesh = clip_mesh(&mesh, polys);
        }
        if let Some(tint) = self.tint {
            tint_mesh(&mut mesh, tint);
        }
        if self.opacity < 1.0 {
            let a = if self.opacity.is_finite() {
                self.opacity.max(0.0)
            } else {
                1.0
            };
            for v in &mut mesh.vertices {
                v.color = v.color.linear_multiply(a);
            }
        }
        mesh
    }
}

/// Move every vertex colour toward the tint colour, keeping alpha (colours are premultiplied).
pub(crate) fn tint_mesh(mesh: &mut Mesh, tint: Tint) {
    let k = if tint.amount.is_finite() {
        tint.amount.clamp(0.0, 1.0)
    } else {
        return;
    };
    if k == 0.0 {
        return;
    }
    for v in &mut mesh.vertices {
        let [r, g, b, a] = v.color.to_array();
        let target = |t: u8| f32::from(t) * f32::from(a) / 255.0;
        let mix = |c: u8, t: u8| (f32::from(c) + (target(t) - f32::from(c)) * k).round() as u8;
        v.color = Color32::from_rgba_premultiplied(
            mix(r, tint.rgb[0]),
            mix(g, tint.rgb[1]),
            mix(b, tint.rgb[2]),
            a,
        );
    }
}

/// Keep only the parts of `mesh` inside `polys` (convex, either winding, not overlapping).
pub(crate) fn clip_mesh(mesh: &Mesh, polys: &[Vec<Pos2>]) -> Mesh {
    let mut out = Mesh::with_texture(mesh.texture_id);
    let clips: Vec<Clip<'_>> = polys.iter().filter_map(|p| Clip::new(p)).collect();
    if clips.is_empty() {
        return out;
    }
    let mut work: Vec<Vertex> = Vec::with_capacity(16);
    let mut next: Vec<Vertex> = Vec::with_capacity(16);
    for tri in mesh.indices.as_chunks::<3>().0 {
        let (Some(a), Some(b), Some(c)) = (
            mesh.vertices.get(tri[0] as usize),
            mesh.vertices.get(tri[1] as usize),
            mesh.vertices.get(tri[2] as usize),
        ) else {
            continue;
        };
        let bounds = Rect::from_points(&[a.pos, b.pos, c.pos]);
        for clip in &clips {
            if !clip.bounds.intersects(bounds) {
                continue;
            }
            work.clear();
            work.extend([*a, *b, *c]);
            if !clip.contains_all(&work) {
                for edge in 0..clip.points.len() {
                    clip.cut(edge, &work, &mut next);
                    std::mem::swap(&mut work, &mut next);
                    if work.len() < 3 {
                        break;
                    }
                }
            }
            push_fan(&mut out, &work);
        }
    }
    out
}

/// The part of convex polygon `subject` inside convex polygon `clip` (either winding).
/// Empty when they do not overlap or `clip` is degenerate.
pub(crate) fn intersect_convex(subject: &[Pos2], clip: &[Pos2]) -> Vec<Pos2> {
    let Some(edges) = Clip::new(clip) else {
        return Vec::new();
    };
    let mut work: Vec<Pos2> = subject.to_vec();
    for i in 0..edges.points.len() {
        if work.len() < 3 {
            return Vec::new();
        }
        let mut next = Vec::with_capacity(work.len() + 2);
        let mut prev = work[work.len() - 1];
        let mut prev_d = edges.side(i, prev);
        for &cur in &work {
            let d = edges.side(i, cur);
            if d >= 0.0 {
                if prev_d < 0.0 {
                    let k = prev_d / (prev_d - d);
                    next.push(pos2(
                        prev.x + (cur.x - prev.x) * k,
                        prev.y + (cur.y - prev.y) * k,
                    ));
                }
                next.push(cur);
            } else if prev_d >= 0.0 {
                let k = prev_d / (prev_d - d);
                next.push(pos2(
                    prev.x + (cur.x - prev.x) * k,
                    prev.y + (cur.y - prev.y) * k,
                ));
            }
            prev = cur;
            prev_d = d;
        }
        work = next;
    }
    if work.len() < 3 { Vec::new() } else { work }
}

/// One convex clip polygon with its winding sign.
struct Clip<'a> {
    points: &'a [Pos2],
    sign: f32,
    bounds: Rect,
}

impl<'a> Clip<'a> {
    fn new(points: &'a [Pos2]) -> Option<Self> {
        if points.len() < 3 || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return None;
        }
        let mut twice = 0.0;
        for (i, p) in points.iter().enumerate() {
            let q = points[(i + 1) % points.len()];
            twice += p.x * q.y - q.x * p.y;
        }
        if twice.abs() < 1e-6 {
            return None;
        }
        Some(Self {
            points,
            sign: twice.signum(),
            bounds: Rect::from_points(points),
        })
    }

    /// Signed distance-like value of `v` against edge `i`: `>= 0` inside.
    fn side(&self, i: usize, v: Pos2) -> f32 {
        let p = self.points[i];
        let q = self.points[(i + 1) % self.points.len()];
        ((q.x - p.x) * (v.y - p.y) - (q.y - p.y) * (v.x - p.x)) * self.sign
    }

    fn contains_all(&self, vs: &[Vertex]) -> bool {
        (0..self.points.len()).all(|i| vs.iter().all(|v| self.side(i, v.pos) >= 0.0))
    }

    /// Clip polygon `input` against edge `i` into `out`.
    fn cut(&self, i: usize, input: &[Vertex], out: &mut Vec<Vertex>) {
        out.clear();
        let Some(&last) = input.last() else {
            return;
        };
        let mut prev = last;
        let mut prev_d = self.side(i, prev.pos);
        for &cur in input {
            let d = self.side(i, cur.pos);
            if d >= 0.0 {
                if prev_d < 0.0 {
                    out.push(lerp_vertex(prev, cur, prev_d / (prev_d - d)));
                }
                out.push(cur);
            } else if prev_d >= 0.0 {
                out.push(lerp_vertex(prev, cur, prev_d / (prev_d - d)));
            }
            prev = cur;
            prev_d = d;
        }
    }
}

fn lerp_vertex(a: Vertex, b: Vertex, t: f32) -> Vertex {
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let l = |x: f32, y: f32| x + (y - x) * t;
    let ca = a.color.to_array();
    let cb = b.color.to_array();
    let lc = |i: usize| l(f32::from(ca[i]), f32::from(cb[i])).round() as u8;
    Vertex {
        pos: pos2(l(a.pos.x, b.pos.x), l(a.pos.y, b.pos.y)),
        uv: pos2(l(a.uv.x, b.uv.x), l(a.uv.y, b.uv.y)),
        color: Color32::from_rgba_premultiplied(lc(0), lc(1), lc(2), lc(3)),
    }
}

fn push_fan(out: &mut Mesh, poly: &[Vertex]) {
    if poly.len() < 3 {
        return;
    }
    let Ok(base) = u32::try_from(out.vertices.len()) else {
        return;
    };
    let Ok(n) = u32::try_from(poly.len()) else {
        return;
    };
    out.vertices.extend_from_slice(poly);
    for k in 1..n.saturating_sub(1) {
        out.indices
            .extend([base, base.saturating_add(k), base.saturating_add(k + 1)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(rect: Rect, color: Color32) -> Mesh {
        let mut m = Mesh::default();
        m.add_colored_rect(rect, color);
        m
    }

    fn area(mesh: &Mesh) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| mesh.vertices[i as usize].pos);
                ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() * 0.5
            })
            .sum()
    }

    #[test]
    fn clipping_keeps_the_area_inside_the_polygon() {
        let m = quad(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
            Color32::RED,
        );
        // Left half, as a square in either winding.
        for poly in [
            vec![
                pos2(-10.0, -10.0),
                pos2(50.0, -10.0),
                pos2(50.0, 110.0),
                pos2(-10.0, 110.0),
            ],
            vec![
                pos2(-10.0, 110.0),
                pos2(50.0, 110.0),
                pos2(50.0, -10.0),
                pos2(-10.0, -10.0),
            ],
        ] {
            let out = clip_mesh(&m, &[poly]);
            assert!((area(&out) - 5000.0).abs() < 0.5, "{}", area(&out));
        }
        // A diamond inscribed in the square keeps half its area.
        let diamond = vec![
            pos2(50.0, 0.0),
            pos2(100.0, 50.0),
            pos2(50.0, 100.0),
            pos2(0.0, 50.0),
        ];
        assert!((area(&clip_mesh(&m, &[diamond])) - 5000.0).abs() < 0.5);
        // No polygons, or degenerate ones, hide everything.
        assert!(clip_mesh(&m, &[]).is_empty());
        let line = vec![pos2(0.0, 0.0), pos2(50.0, 50.0), pos2(100.0, 100.0)];
        assert!(clip_mesh(&m, &[line]).is_empty());
        let nan = vec![pos2(f32::NAN, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0)];
        assert!(clip_mesh(&m, &[nan]).is_empty());
    }

    #[test]
    fn clipping_interpolates_uv() {
        let mut m = Mesh::with_texture(egui::TextureId::Managed(1));
        m.add_rect_with_uv(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 100.0)),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        let half = vec![
            pos2(0.0, 0.0),
            pos2(50.0, 0.0),
            pos2(50.0, 100.0),
            pos2(0.0, 100.0),
        ];
        let out = clip_mesh(&m, &[half]);
        assert_eq!(out.texture_id, m.texture_id);
        for v in &out.vertices {
            assert!((v.uv.x - v.pos.x / 100.0).abs() < 1e-4);
            assert!((v.uv.y - v.pos.y / 100.0).abs() < 1e-4);
        }
    }

    #[test]
    fn scale_then_turn_about_the_centre() {
        let m = quad(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 50.0)),
            Color32::RED,
        );
        let post = MeshPost {
            scale: (0.5, 1.0),
            ..MeshPost::turn(pos2(50.0, 25.0), 90.0, false, false)
        };
        let out = post.apply(m);
        let b = Rect::from_points(&out.vertices.iter().map(|v| v.pos).collect::<Vec<_>>());
        // 50×50 after the scale, still 50×50 after a quarter turn about the centre.
        assert!((b.width() - 50.0).abs() < 1e-3 && (b.height() - 50.0).abs() < 1e-3);
        assert!((b.center() - pos2(50.0, 25.0)).length() < 1e-3);
    }

    #[test]
    fn tint_moves_colour_and_keeps_alpha() {
        let mut m = quad(
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::from_rgba_premultiplied(0, 0, 0, 128),
        );
        tint_mesh(
            &mut m,
            Tint {
                rgb: [255, 0, 0],
                amount: 1.0,
            },
        );
        for v in &m.vertices {
            assert_eq!(v.color.to_array(), [128, 0, 0, 128]);
        }
        tint_mesh(
            &mut m,
            Tint {
                rgb: [0, 0, 255],
                amount: f32::NAN,
            },
        );
        assert_eq!(m.vertices[0].color.to_array(), [128, 0, 0, 128]);
    }
}
