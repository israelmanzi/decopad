//! Ink: the strokes written in the Popup, kept in Tablet units so they can be
//! drawn in the Popup and reproduced at any Ink Scale.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    /// 0.0 (lightest touch) to 1.0 (full pressure).
    pub pressure: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Stroke {
    pub points: Vec<Point>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

#[derive(Default)]
pub struct Ink {
    strokes: Vec<Stroke>,
    /// Strokes taken back by `undo`, most recent last.
    undone: Vec<Stroke>,
}

impl Ink {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    pub fn begin(&mut self, point: Point) {
        self.undone.clear();
        self.strokes.push(Stroke {
            points: vec![point],
        });
    }

    /// Adds a point to the stroke being written.
    pub fn extend(&mut self, point: Point) {
        match self.strokes.last_mut() {
            Some(stroke) => stroke.points.push(point),
            None => self.begin(point),
        }
    }

    pub fn undo(&mut self) -> bool {
        match self.strokes.pop() {
            Some(stroke) => {
                self.undone.push(stroke);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.undone.pop() {
            Some(stroke) => {
                self.strokes.push(stroke);
                true
            }
            None => false,
        }
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
        self.undone.clear();
    }

    /// Removes every stroke passing within `radius` of `(x, y)`.
    pub fn erase_near(&mut self, x: i32, y: i32, radius: i32) -> bool {
        let before = self.strokes.len();
        let target = (f64::from(x), f64::from(y));
        self.strokes.retain(|stroke| {
            let points = &stroke.points;
            let segments = std::iter::once((points[0], points[0]))
                .chain(points.windows(2).map(|w| (w[0], w[1])));
            !segments
                .into_iter()
                .any(|(a, b)| segment_distance(target, a, b) <= f64::from(radius))
        });
        self.strokes.len() != before
    }

    pub fn bounds(&self) -> Option<Bounds> {
        let mut points = self.strokes.iter().flat_map(|s| &s.points);
        let first = points.next()?;
        let mut b = Bounds {
            min_x: first.x,
            min_y: first.y,
            max_x: first.x,
            max_y: first.y,
        };
        for p in points {
            b.min_x = b.min_x.min(p.x);
            b.min_y = b.min_y.min(p.y);
            b.max_x = b.max_x.max(p.x);
            b.max_y = b.max_y.max(p.y);
        }
        Some(b)
    }
}

fn segment_distance((px, py): (f64, f64), a: Point, b: Point) -> f64 {
    let (ax, ay, bx, by) = (
        f64::from(a.x),
        f64::from(a.y),
        f64::from(b.x),
        f64::from(b.y),
    );
    let (dx, dy) = (bx - ax, by - ay);
    let length2 = dx * dx + dy * dy;
    let t = match length2 {
        0.0 => 0.0,
        _ => (((px - ax) * dx + (py - ay) * dy) / length2).clamp(0.0, 1.0),
    };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// How Ink maps onto the screen when it is reproduced at the Anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The Tablet point that lands exactly on the Anchor.
    pub origin: (f64, f64),
    pub anchor: (f64, f64),
    pub px_per_unit: f64,
    /// Width of the Ink on screen.
    pub width_px: f64,
}

impl Placement {
    /// Puts the Ink's left edge on the Anchor, and the baseline of the ruled
    /// line where writing started on the Anchor's height.
    pub fn new(
        ink: &Ink,
        anchor: (i32, i32),
        px_per_unit: f64,
        line_spacing_units: i32,
    ) -> Option<Self> {
        let bounds = ink.bounds()?;
        let first = ink.strokes.first()?.points.first()?;
        let line = first.y.max(0) / line_spacing_units;
        let baseline = (line + 1) * line_spacing_units;
        Some(Placement {
            origin: (f64::from(bounds.min_x), f64::from(baseline)),
            anchor: (f64::from(anchor.0), f64::from(anchor.1)),
            px_per_unit,
            width_px: f64::from(bounds.max_x - bounds.min_x) * px_per_unit,
        })
    }

    pub fn to_screen(self, p: Point) -> (f64, f64) {
        (
            self.anchor.0 + (f64::from(p.x) - self.origin.0) * self.px_per_unit,
            self.anchor.1 + (f64::from(p.y) - self.origin.1) * self.px_per_unit,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i32, y: i32) -> Point {
        Point {
            x,
            y,
            pressure: 0.5,
        }
    }

    fn ink(strokes: &[&[(i32, i32)]]) -> Ink {
        let mut ink = Ink::default();
        for stroke in strokes {
            ink.begin(point(stroke[0].0, stroke[0].1));
            for &(x, y) in &stroke[1..] {
                ink.extend(point(x, y));
            }
        }
        ink
    }

    #[test]
    fn writing_on_the_second_line_sits_on_the_anchor() {
        // 15 mm lines at 100 units/mm: the second line's baseline is y = 3000.
        let ink = ink(&[&[(2000, 2600), (2600, 3000)], &[(3000, 2400), (3400, 2950)]]);
        let place = Placement::new(&ink, (500, 300), 0.03, 1500).unwrap();
        assert_eq!(place.to_screen(point(2000, 3000)), (500.0, 300.0));
        assert_eq!(
            place.to_screen(point(3400, 2950)),
            (500.0 + 1400.0 * 0.03, 300.0 - 50.0 * 0.03)
        );
        assert!((place.width_px - 42.0).abs() < 1e-9);
    }

    #[test]
    fn eraser_removes_only_touched_strokes() {
        let mut ink = ink(&[&[(0, 0), (100, 0)], &[(1000, 1000), (1100, 1000)]]);
        assert!(ink.erase_near(1050, 1010, 50));
        assert_eq!(ink.strokes().len(), 1);
        assert!(!ink.erase_near(5000, 5000, 50));
        assert_eq!(
            ink.bounds(),
            Some(Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 100,
                max_y: 0
            })
        );
    }

    #[test]
    fn redo_restores_undone_strokes_until_new_writing() {
        let mut ink = ink(&[&[(0, 0)], &[(5, 5)]]);
        assert!(ink.undo() && ink.undo());
        assert!(ink.redo());
        assert_eq!(ink.strokes().len(), 1);
        ink.begin(point(9, 9));
        assert!(!ink.redo(), "new writing discards what was undone");
    }

    #[test]
    fn undo_and_clear() {
        let mut ink = ink(&[&[(0, 0)], &[(5, 5)]]);
        assert!(ink.undo());
        assert_eq!(ink.strokes().len(), 1);
        ink.clear();
        assert!(ink.is_empty() && !ink.undo() && ink.bounds().is_none());
    }
}
