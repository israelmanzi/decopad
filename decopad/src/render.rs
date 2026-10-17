//! Drawing: the Popup's canvas and the Picture Paste image.

use fontdue::{Font, FontSettings};
use tiny_skia::{
    Color, IntRect, LineCap, Paint, PathBuilder, Pixmap, Rect, Stroke as Pen, Transform,
};

use crate::ink::{Ink, Placement, Point};

const BACKGROUND: [u8; 3] = [0xfb, 0xfb, 0xf8];
const BORDER: [u8; 3] = [0xc4, 0xc4, 0xbc];
const RULE: [u8; 3] = [0xe9, 0xe9, 0xe2];
const INK: [u8; 3] = [0x1f, 0x24, 0x30];
const CHEATSHEET: [u8; 3] = [0xa3, 0xa6, 0xad];
const TIP: [u8; 3] = [0x7a, 0x80, 0x8a];
const ERASER: [u8; 3] = [0xd0, 0x53, 0x3f];

/// Ink width at full pressure; light touches go down to 35% of it.
const POPUP_STROKE_MM: f32 = 0.7;
const PICTURE_STROKE_MM: f32 = 0.8;

fn color([r, g, b]: [u8; 3]) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

fn paint(rgb: [u8; 3]) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color(rgb));
    paint.anti_alias = true;
    paint
}

fn stroke_width(pressure: f32, full: f32) -> f32 {
    full * (0.35 + 0.65 * pressure.clamp(0.0, 1.0))
}

/// Loads the desktop's default sans-serif font for the Cheatsheet.
pub fn load_font() -> Option<Font> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", "sans-serif"])
        .output()
        .ok()?;
    let path = String::from_utf8(output.stdout).ok()?;
    let bytes = std::fs::read(path.trim()).ok()?;
    Font::from_bytes(bytes, FontSettings::default()).ok()
}

/// What the Popup shows besides Ink.
pub struct Overlay<'a> {
    pub cheatsheet: &'a [String],
    pub eraser: bool,
}

/// The Popup's pixels: `base` holds everything but the Pen Tip ring, which is
/// drawn on `frame` when changed regions are presented.
pub struct Canvas {
    pub base: Pixmap,
    pub frame: Pixmap,
    /// Tablet extent (max coordinates) and lines per Tablet unit.
    tablet: (i32, i32),
    line_spacing_units: i32,
    units_per_mm: f32,
}

impl Canvas {
    pub fn new(width: u32, tablet: (i32, i32), units_per_mm: f32, line_spacing_units: i32) -> Self {
        let height = (width as i64 * (tablet.1 as i64 + 1) / (tablet.0 as i64 + 1)).max(1) as u32;
        Canvas {
            base: Pixmap::new(width, height).expect("popup size"),
            frame: Pixmap::new(width, height).expect("popup size"),
            tablet,
            line_spacing_units,
            units_per_mm,
        }
    }

    pub fn width(&self) -> u32 {
        self.base.width()
    }

    pub fn height(&self) -> u32 {
        self.base.height()
    }

    fn px_per_unit(&self) -> f32 {
        self.base.width() as f32 / (self.tablet.0 + 1) as f32
    }

    pub fn to_canvas(&self, x: i32, y: i32) -> (f32, f32) {
        let s = self.px_per_unit();
        (x as f32 * s, y as f32 * s)
    }

    fn ink_width(&self, pressure: f32) -> f32 {
        stroke_width(
            pressure,
            POPUP_STROKE_MM * self.units_per_mm * self.px_per_unit(),
        )
    }

    /// Repaints the background, ruled lines, Cheatsheet and all Ink.
    pub fn redraw(&mut self, ink: &Ink, overlay: &Overlay, font: Option<&Font>) {
        let (w, h) = (self.width() as f32, self.height() as f32);
        self.base.fill(color(BACKGROUND));

        let spacing = self.line_spacing_units as f32 * self.px_per_unit();
        let rule = paint(RULE);
        let mut y = spacing;
        while y < h - 2.0 {
            if let Some(r) = Rect::from_xywh(0.0, y.round(), w, 1.0) {
                self.base.fill_rect(r, &rule, Transform::identity(), None);
            }
            y += spacing;
        }

        for stroke in ink.strokes() {
            let points = &stroke.points;
            self.segment(points[0], points[0]);
            for pair in points.windows(2) {
                self.segment(pair[0], pair[1]);
            }
        }

        if let Some(font) = font {
            let size = (h / 34.0).clamp(11.0, 16.0);
            let line_height = (size * 1.45).round();
            let mut baseline = h - 8.0 - line_height * (overlay.cheatsheet.len() as f32 - 1.0);
            for line in overlay.cheatsheet {
                draw_text(&mut self.base, font, 10.0, baseline, size, line, CHEATSHEET);
                baseline += line_height;
            }
            if overlay.eraser {
                let width = text_width(font, size, "eraser");
                draw_text(
                    &mut self.base,
                    font,
                    w - width - 10.0,
                    8.0 + size,
                    size,
                    "eraser",
                    ERASER,
                );
            }
        }

        let border = paint(BORDER);
        for r in [
            Rect::from_xywh(0.0, 0.0, w, 1.0),
            Rect::from_xywh(0.0, h - 1.0, w, 1.0),
            Rect::from_xywh(0.0, 0.0, 1.0, h),
            Rect::from_xywh(w - 1.0, 0.0, 1.0, h),
        ]
        .into_iter()
        .flatten()
        {
            self.base.fill_rect(r, &border, Transform::identity(), None);
        }

        // Resize grip: three short diagonals in the bottom-right corner.
        let pen = Pen {
            width: 1.2,
            line_cap: LineCap::Round,
            ..Pen::default()
        };
        for offset in [5.0, 9.0, 13.0] {
            let mut path = PathBuilder::new();
            path.move_to(w - 3.0, h - offset);
            path.line_to(w - offset, h - 3.0);
            if let Some(path) = path.finish() {
                self.base
                    .stroke_path(&path, &border, &pen, Transform::identity(), None);
            }
        }
    }

    /// Draws one piece of a stroke onto `base` and returns the pixels it touched.
    pub fn segment(&mut self, a: Point, b: Point) -> Option<IntRect> {
        let (ax, ay) = self.to_canvas(a.x, a.y);
        let (bx, by) = self.to_canvas(b.x, b.y);
        let width = self.ink_width((a.pressure + b.pressure) / 2.0);
        let mut path = PathBuilder::new();
        path.move_to(ax, ay);
        // A zero-length line with round caps draws a dot.
        path.line_to(bx + 0.01, by);
        let pen = Pen {
            width,
            line_cap: LineCap::Round,
            ..Pen::default()
        };
        self.base.stroke_path(
            &path.finish()?,
            &paint(INK),
            &pen,
            Transform::identity(),
            None,
        );
        let pad = width / 2.0 + 2.0;
        int_rect(
            ax.min(bx) - pad,
            ay.min(by) - pad,
            ax.max(bx) + pad,
            ay.max(by) + pad,
        )
    }

    /// Ring showing where the Pen is, in canvas pixels, plus its bounding box.
    pub fn tip_ring(
        &self,
        x: i32,
        y: i32,
        erasing: bool,
        eraser_radius_units: i32,
    ) -> (f32, f32, f32, IntRect) {
        let (cx, cy) = self.to_canvas(x, y);
        let r = match erasing {
            true => eraser_radius_units as f32 * self.px_per_unit(),
            false => 5.0,
        };
        let rect = int_rect(cx - r - 2.0, cy - r - 2.0, cx + r + 2.0, cy + r + 2.0)
            .unwrap_or_else(|| IntRect::from_xywh(0, 0, 1, 1).unwrap());
        (cx, cy, r, rect)
    }

    /// Copies `rect` from `base` to `frame` and draws the ring over it.
    pub fn compose(&mut self, rect: IntRect, ring: Option<(f32, f32, f32, bool)>) {
        let stride = self.width() as usize * 4;
        let (x0, x1) = (
            rect.x() as usize * 4,
            (rect.x() as usize + rect.width() as usize) * 4,
        );
        for row in rect.y() as usize..rect.y() as usize + rect.height() as usize {
            let range = row * stride + x0..row * stride + x1;
            self.frame.data_mut()[range.clone()].copy_from_slice(&self.base.data()[range]);
        }
        if let Some((x, y, r, erasing)) = ring
            && let Some(circle) = PathBuilder::from_circle(x, y, r)
        {
            let pen = Pen {
                width: 1.5,
                ..Pen::default()
            };
            let rgb = if erasing { ERASER } else { TIP };
            self.frame
                .stroke_path(&circle, &paint(rgb), &pen, Transform::identity(), None);
        }
    }

    /// Clamps a rectangle to the canvas.
    pub fn clip(&self, rect: IntRect) -> Option<IntRect> {
        let x0 = rect.x().max(0);
        let y0 = rect.y().max(0);
        let x1 = (rect.x() + rect.width() as i32).min(self.width() as i32);
        let y1 = (rect.y() + rect.height() as i32).min(self.height() as i32);
        IntRect::from_ltrb(x0, y0, x1, y1)
    }

    pub fn full(&self) -> IntRect {
        IntRect::from_xywh(0, 0, self.width(), self.height()).unwrap()
    }
}

fn int_rect(l: f32, t: f32, r: f32, b: f32) -> Option<IntRect> {
    IntRect::from_ltrb(
        l.floor() as i32,
        t.floor() as i32,
        r.ceil() as i32,
        b.ceil() as i32,
    )
}

pub fn union(a: Option<IntRect>, b: Option<IntRect>) -> Option<IntRect> {
    match (a, b) {
        (Some(a), Some(b)) => IntRect::from_ltrb(
            a.x().min(b.x()),
            a.y().min(b.y()),
            (a.x() + a.width() as i32).max(b.x() + b.width() as i32),
            (a.y() + a.height() as i32).max(b.y() + b.height() as i32),
        ),
        (a, b) => a.or(b),
    }
}

fn text_width(font: &Font, size: f32, text: &str) -> f32 {
    text.chars()
        .map(|c| font.metrics(c, size).advance_width)
        .sum()
}

/// Blends `text` onto an opaque pixmap with its baseline at `y`.
fn draw_text(
    pixmap: &mut Pixmap,
    font: &Font,
    x: f32,
    y: f32,
    size: f32,
    text: &str,
    rgb: [u8; 3],
) {
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    let mut pen_x = x;
    for c in text.chars() {
        let (m, coverage) = font.rasterize(c, size);
        let left = (pen_x + m.xmin as f32).round() as i32;
        let top = (y - m.ymin as f32 - m.height as f32).round() as i32;
        for gy in 0..m.height as i32 {
            for gx in 0..m.width as i32 {
                let (px, py) = (left + gx, top + gy);
                if px < 0 || py < 0 || px >= pw || py >= ph {
                    continue;
                }
                let a = u32::from(coverage[(gy * m.width as i32 + gx) as usize]);
                let i = ((py * pw + px) * 4) as usize;
                for (channel, &value) in rgb.iter().enumerate() {
                    let old = u32::from(data[i + channel]);
                    data[i + channel] = ((old * (255 - a) + u32::from(value) * a) / 255) as u8;
                }
            }
        }
        pen_x += m.advance_width;
    }
}

/// Ink drawn at its delivered size on a transparent background, for Picture
/// Paste and the preview. The Tablet point `origin` sits at `(pad, pad)`.
pub struct InkImage {
    pub pixmap: Pixmap,
    pub pad: f32,
    origin: (i32, i32),
    scale: f32,
    full_width: f32,
}

impl InkImage {
    /// An empty image covering `size` Tablet units from `origin`.
    pub fn new(
        origin: (i32, i32),
        size: (i32, i32),
        px_per_unit: f64,
        units_per_mm: f32,
    ) -> Option<Self> {
        let scale = px_per_unit as f32;
        let full_width = PICTURE_STROKE_MM * units_per_mm * scale;
        let pad = (full_width + 4.0).ceil();
        let width = (size.0 as f32 * scale + 2.0 * pad).ceil() as u32;
        let height = (size.1 as f32 * scale + 2.0 * pad).ceil() as u32;
        Some(InkImage {
            pixmap: Pixmap::new(width.max(1), height.max(1))?,
            pad,
            origin,
            scale,
            full_width,
        })
    }

    pub fn draw(&mut self, ink: &Ink) {
        for stroke in ink.strokes() {
            let points = &stroke.points;
            self.segment(points[0], points[0]);
            for pair in points.windows(2) {
                self.segment(pair[0], pair[1]);
            }
        }
    }

    /// Draws one piece of a stroke and returns the pixels it touched.
    pub fn segment(&mut self, a: Point, b: Point) -> Option<IntRect> {
        let at = |p: Point| {
            (
                (p.x - self.origin.0) as f32 * self.scale + self.pad,
                (p.y - self.origin.1) as f32 * self.scale + self.pad,
            )
        };
        let ((ax, ay), (bx, by)) = (at(a), at(b));
        let width = stroke_width((a.pressure + b.pressure) / 2.0, self.full_width);
        let mut path = PathBuilder::new();
        path.move_to(ax, ay);
        path.line_to(bx + 0.01, by);
        let pen = Pen {
            width,
            line_cap: LineCap::Round,
            ..Pen::default()
        };
        self.pixmap.stroke_path(
            &path.finish()?,
            &paint(INK),
            &pen,
            Transform::identity(),
            None,
        );
        let margin = width / 2.0 + 2.0;
        int_rect(
            ax.min(bx) - margin,
            ay.min(by) - margin,
            ax.max(bx) + margin,
            ay.max(by) + margin,
        )
    }
}

/// Renders Ink cropped to its bounds, for Picture Paste. Returns the image
/// and the margin around the Ink.
pub fn picture(ink: &Ink, px_per_unit: f64, units_per_mm: f32) -> Option<(Pixmap, f32)> {
    let b = ink.bounds()?;
    let size = (b.max_x - b.min_x, b.max_y - b.min_y);
    let mut image = InkImage::new((b.min_x, b.min_y), size, px_per_unit, units_per_mm)?;
    image.draw(ink);
    Some((image.pixmap, image.pad))
}

/// Screen-space points of each stroke for Replay, simplified so the replayed
/// line stays within `tolerance` pixels of the written one and its pressure
/// within `PRESSURE_TOLERANCE`. Fewer points make Replay faster.
pub fn replay_points(
    ink: &Ink,
    placement: &Placement,
    tolerance: f64,
) -> Vec<Vec<(f64, f64, f32)>> {
    ink.strokes()
        .iter()
        .map(|stroke| {
            let points: Vec<(f64, f64, f32)> = stroke
                .points
                .iter()
                .map(|&p| {
                    let (x, y) = placement.to_screen(p);
                    (x, y, p.pressure)
                })
                .collect();
            simplify(&points, tolerance)
        })
        .collect()
}

const PRESSURE_TOLERANCE: f32 = 0.08;

/// Ramer–Douglas–Peucker over position and pressure: keeps a point when
/// dropping it would move the line by more than `tolerance` pixels or change
/// the pressure there by more than `PRESSURE_TOLERANCE`.
fn simplify(points: &[(f64, f64, f32)], tolerance: f64) -> Vec<(f64, f64, f32)> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut spans = vec![(0, points.len() - 1)];
    while let Some((first, last)) = spans.pop() {
        let (ax, ay, ap) = points[first];
        let (bx, by, bp) = points[last];
        let (dx, dy) = (bx - ax, by - ay);
        let length2 = dx * dx + dy * dy;
        let mut worst = (0.0, first);
        for (i, &(px, py, pp)) in points.iter().enumerate().take(last).skip(first + 1) {
            let t = if length2 == 0.0 {
                0.0
            } else {
                (((px - ax) * dx + (py - ay) * dy) / length2).clamp(0.0, 1.0)
            };
            let distance = (px - (ax + t * dx)).hypot(py - (ay + t * dy));
            let pressure = (pp - (ap + t as f32 * (bp - ap))).abs();
            let error = (distance / tolerance).max(f64::from(pressure / PRESSURE_TOLERANCE));
            if error > worst.0 {
                worst = (error, i);
            }
        }
        if worst.0 > 1.0 {
            keep[worst.1] = true;
            spans.push((first, worst.1));
            spans.push((worst.1, last));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(&p, _)| p)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_strokes_collapse_to_their_ends() {
        let line: Vec<_> = (0..=100).map(|i| (i as f64, 2.0 * i as f64, 0.5)).collect();
        assert_eq!(simplify(&line, 0.2), [(0.0, 0.0, 0.5), (100.0, 200.0, 0.5)]);
    }

    #[test]
    fn curves_stay_within_tolerance_and_pressure_changes_are_kept() {
        let circle: Vec<_> = (0..=200)
            .map(|i| {
                let a = i as f64 / 200.0 * std::f64::consts::TAU;
                (50.0 * a.cos(), 50.0 * a.sin(), 0.5)
            })
            .collect();
        let simple = simplify(&circle, 0.2);
        assert!(simple.len() < circle.len() / 2, "{} points", simple.len());
        // Every original point lies within 0.2 px of the simplified outline.
        for &(x, y, _) in &circle {
            let near = simple.windows(2).any(|w| {
                let ((ax, ay, _), (bx, by, _)) = (w[0], w[1]);
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (x - ax - t * dx).hypot(y - ay - t * dy) <= 0.2 + 1e-9
            });
            assert!(near, "({x}, {y}) strayed");
        }
        // A straight line whose pressure jumps keeps the jump.
        let line: Vec<_> = (0..=10)
            .map(|i| (i as f64, 0.0, if i < 5 { 0.2 } else { 0.9 }))
            .collect();
        assert!(simplify(&line, 0.2).len() > 2);
    }

    #[test]
    fn picture_is_sized_to_the_ink_at_delivery_scale() {
        let mut ink = Ink::default();
        ink.begin(Point {
            x: 1000,
            y: 1000,
            pressure: 1.0,
        });
        ink.extend(Point {
            x: 3000,
            y: 2000,
            pressure: 1.0,
        });
        // 3 px/mm at 100 units/mm: 20 mm × 10 mm of ink is 60 × 30 px plus padding.
        let (pixmap, _) = picture(&ink, 0.03, 100.0).unwrap();
        let pad = (0.8f32 * 3.0 + 4.0).ceil() as u32;
        assert_eq!(
            (pixmap.width(), pixmap.height()),
            (60 + 2 * pad, 30 + 2 * pad)
        );
        // Corners stay transparent; the stroke itself is opaque ink.
        assert_eq!(pixmap.pixel(0, 0).unwrap().alpha(), 0);
        assert!(pixmap.pixel(pad + 30, pad + 15).unwrap().alpha() > 200);
    }
}
