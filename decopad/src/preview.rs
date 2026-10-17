//! Shows, in the target app, where Ink will land: a caret at the Anchor and
//! a live preview of the Ink at its delivered size. Both are shaped windows
//! that take no input, so clicks and Replay strokes pass straight through.

use anyhow::Result;
use tiny_skia::{IntRect, Pixmap};
use x11rb::connection::Connection;
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    ClipOrdering, ConfigureWindowAux, ConnectionExt as _, CreateWindowAux, Rectangle, StackMode,
    Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// Preview colour, distinct from delivered ink.
const COLOR: u32 = 0x2b6cd9;
/// Pixels at least this opaque are part of the preview's shape.
const ALPHA_THRESHOLD: u8 = 96;

struct Overlay {
    window: Window,
    mapped: bool,
    geometry: (i32, i32, u32, u32),
}

impl Overlay {
    fn new(conn: &RustConnection, screen_num: usize) -> Result<Self> {
        let screen = &conn.setup().roots[screen_num];
        let window = conn.generate_id()?;
        conn.create_window(
            screen.root_depth,
            window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(COLOR),
        )?;
        // An empty input shape: the window never receives pointer events.
        conn.shape_rectangles(
            SO::SET,
            SK::INPUT,
            ClipOrdering::UNSORTED,
            window,
            0,
            0,
            &[],
        )?;
        Ok(Overlay {
            window,
            mapped: false,
            geometry: (0, 0, 0, 0),
        })
    }

    fn place(&mut self, conn: &RustConnection, geometry: (i32, i32, u32, u32)) -> Result<()> {
        if geometry != self.geometry || !self.mapped {
            let (x, y, w, h) = geometry;
            conn.configure_window(
                self.window,
                &ConfigureWindowAux::new()
                    .x(x)
                    .y(y)
                    .width(w.max(1))
                    .height(h.max(1))
                    .stack_mode(StackMode::ABOVE),
            )?;
            self.geometry = geometry;
        }
        if !self.mapped {
            conn.map_window(self.window)?;
            self.mapped = true;
        }
        Ok(())
    }

    fn hide(&mut self, conn: &RustConnection) -> Result<()> {
        if self.mapped {
            conn.unmap_window(self.window)?;
            self.mapped = false;
        }
        Ok(())
    }
}

pub struct Preview {
    caret: Overlay,
    ink: Overlay,
    ink_size: (u32, u32),
}

impl Preview {
    pub fn new(conn: &RustConnection, screen_num: usize) -> Result<Self> {
        Ok(Preview {
            caret: Overlay::new(conn, screen_num)?,
            ink: Overlay::new(conn, screen_num)?,
            ink_size: (1, 1),
        })
    }

    /// A 2-pixel bar standing on the Anchor's baseline.
    pub fn show_caret(
        &mut self,
        conn: &RustConnection,
        anchor: (i32, i32),
        height: u32,
    ) -> Result<()> {
        self.caret
            .place(conn, (anchor.0 - 1, anchor.1 - height as i32, 2, height))
    }

    /// Replaces the preview's shape with all of `image`.
    pub fn set_ink(&mut self, conn: &RustConnection, image: &Pixmap) -> Result<()> {
        let full = IntRect::from_xywh(0, 0, image.width(), image.height()).unwrap();
        let rects = opaque_runs(image, full);
        conn.shape_rectangles(
            SO::SET,
            SK::BOUNDING,
            ClipOrdering::YX_SORTED,
            self.ink.window,
            0,
            0,
            &rects,
        )?;
        self.ink_size = (image.width(), image.height());
        Ok(())
    }

    /// Adds the part of `image` inside `rect` to the preview's shape, after
    /// new Ink was drawn there.
    pub fn add_ink(&mut self, conn: &RustConnection, image: &Pixmap, rect: IntRect) -> Result<()> {
        let rects = opaque_runs(image, rect);
        if !rects.is_empty() {
            conn.shape_rectangles(
                SO::UNION,
                SK::BOUNDING,
                ClipOrdering::YX_SORTED,
                self.ink.window,
                0,
                0,
                &rects,
            )?;
        }
        Ok(())
    }

    /// Shows the preview with its image's top-left corner at `at` on screen.
    pub fn place_ink(&mut self, conn: &RustConnection, at: (i32, i32)) -> Result<()> {
        let (w, h) = self.ink_size;
        self.ink.place(conn, (at.0, at.1, w, h))
    }

    pub fn hide_ink(&mut self, conn: &RustConnection) -> Result<()> {
        self.ink.hide(conn)
    }

    pub fn hide(&mut self, conn: &RustConnection) -> Result<()> {
        self.caret.hide(conn)?;
        self.ink.hide(conn)?;
        conn.flush()?;
        Ok(())
    }
}

/// One rectangle per horizontal run of opaque pixels inside `rect`, in row order.
fn opaque_runs(picture: &Pixmap, rect: IntRect) -> Vec<Rectangle> {
    let width = picture.width() as usize;
    let Some(rect) =
        rect.intersect(&IntRect::from_xywh(0, 0, picture.width(), picture.height()).unwrap())
    else {
        return Vec::new();
    };
    let (x0, x1) = (rect.x() as usize, rect.x() as usize + rect.width() as usize);
    let mut rects = Vec::new();
    for y in rect.y() as usize..rect.y() as usize + rect.height() as usize {
        let row = &picture.pixels()[y * width..(y + 1) * width];
        let mut x = x0;
        while x < x1 {
            if row[x].alpha() < ALPHA_THRESHOLD {
                x += 1;
                continue;
            }
            let start = x;
            while x < x1 && row[x].alpha() >= ALPHA_THRESHOLD {
                x += 1;
            }
            rects.push(Rectangle {
                x: start as i16,
                y: y as i16,
                width: (x - start) as u16,
                height: 1,
            });
        }
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{Color, Paint, Rect, Transform};

    #[test]
    fn runs_cover_exactly_the_opaque_pixels() {
        let mut pixmap = Pixmap::new(10, 3).unwrap();
        let mut paint = Paint::default();
        paint.set_color(Color::BLACK);
        pixmap.fill_rect(
            Rect::from_xywh(2.0, 1.0, 3.0, 1.0).unwrap(),
            &paint,
            Transform::identity(),
            None,
        );
        pixmap.fill_rect(
            Rect::from_xywh(7.0, 1.0, 2.0, 2.0).unwrap(),
            &paint,
            Transform::identity(),
            None,
        );
        let full = IntRect::from_xywh(0, 0, 10, 3).unwrap();
        let runs: Vec<_> = opaque_runs(&pixmap, full)
            .iter()
            .map(|r| (r.x, r.y, r.width))
            .collect();
        assert_eq!(runs, [(2, 1, 3), (7, 1, 2), (7, 2, 2)]);
        let right: Vec<_> = opaque_runs(&pixmap, IntRect::from_xywh(3, 1, 5, 1).unwrap())
            .iter()
            .map(|r| (r.x, r.y, r.width))
            .collect();
        assert_eq!(right, [(3, 1, 2), (7, 1, 1)]);
    }
}
