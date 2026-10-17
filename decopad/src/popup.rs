//! The Popup's X window: never focused, always on top, no decorations.
//! The mouse moves it by dragging anywhere and resizes it from the
//! bottom-right corner.

use anyhow::{Context, Result, bail};
use tiny_skia::IntRect;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClipOrdering, ConfigureWindowAux, ConnectionExt as _,
    CreateGCAux, CreateWindowAux, Cursor, EventMask, ImageFormat, ImageOrder, PropMode, StackMode,
    Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

use crate::render::Canvas;

/// Size of the bottom-right square that resizes instead of moving.
pub const GRIP: i32 = 20;
pub const WIDTH_RANGE: (u32, u32) = (320, 2560);

// Glyphs of the standard X cursor font.
const XC_FLEUR: u16 = 52;
const XC_BOTTOM_RIGHT_CORNER: u16 = 14;

enum Drag {
    Move {
        pointer: (i32, i32),
        origin: (i32, i32),
    },
    Resize {
        pointer: (i32, i32),
        width: u32,
    },
}

pub struct Popup {
    pub window: Window,
    gc: u32,
    depth: u8,
    lsb_first: bool,
    /// Open from the user's point of view; it may still be briefly unmapped
    /// while a Replay draws underneath it.
    pub visible: bool,
    mapped: bool,
    /// Position and size on screen.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    drag: Option<Drag>,
    move_cursor: Cursor,
    resize_cursor: Cursor,
    resize_cursor_shown: bool,
    click_through: bool,
}

impl Popup {
    pub fn new(conn: &RustConnection, screen_num: usize) -> Result<Self> {
        let screen = &conn.setup().roots[screen_num];
        let depth = screen.root_depth;
        let format = conn
            .setup()
            .pixmap_formats
            .iter()
            .find(|f| f.depth == depth);
        if format.map(|f| f.bits_per_pixel) != Some(32) {
            bail!("unsupported X pixel format (depth {depth}); the Popup needs 32 bits per pixel");
        }

        let font = conn.generate_id()?;
        conn.open_font(font, b"cursor")?;
        let cursor = |glyph: u16| -> Result<Cursor> {
            let id = conn.generate_id()?;
            conn.create_glyph_cursor(
                id,
                font,
                font,
                glyph,
                glyph + 1,
                0,
                0,
                0,
                0xffff,
                0xffff,
                0xffff,
            )?;
            Ok(id)
        };
        let (move_cursor, resize_cursor) = (cursor(XC_FLEUR)?, cursor(XC_BOTTOM_RIGHT_CORNER)?);
        conn.close_font(font)?;

        let window = conn.generate_id()?;
        conn.create_window(
            depth,
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
                .background_pixel(screen.white_pixel)
                .cursor(move_cursor)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION,
                ),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, window, &CreateGCAux::new())?;
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"decopad",
        )?;
        Ok(Popup {
            window,
            gc,
            depth,
            lsb_first: conn.setup().image_byte_order == ImageOrder::LSB_FIRST,
            visible: false,
            mapped: false,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            drag: None,
            move_cursor,
            resize_cursor,
            resize_cursor_shown: false,
            click_through: false,
        })
    }

    fn configure(&self, conn: &RustConnection) -> Result<()> {
        conn.configure_window(
            self.window,
            &ConfigureWindowAux::new()
                .x(self.x)
                .y(self.y)
                .width(self.width)
                .height(self.height)
                .stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

    /// Shows the Popup with its top-left corner at `(x, y)`.
    pub fn show(&mut self, conn: &RustConnection, x: i32, y: i32, canvas: &Canvas) -> Result<()> {
        (self.x, self.y) = (x, y);
        (self.width, self.height) = (canvas.width(), canvas.height());
        self.configure(conn)?;
        conn.map_window(self.window)?;
        conn.flush()?;
        self.visible = true;
        self.mapped = true;
        Ok(())
    }

    pub fn hide(&mut self, conn: &RustConnection) -> Result<()> {
        self.unmap(conn)?;
        self.visible = false;
        self.drag = None;
        Ok(())
    }

    fn unmap(&mut self, conn: &RustConnection) -> Result<()> {
        if self.mapped {
            conn.unmap_window(self.window)?;
            conn.flush()?;
            self.mapped = false;
        }
        Ok(())
    }

    /// Lets pen and mouse input pass through the window to whatever is below
    /// it (`true`), or catches it again (`false`). Used while a Replay draws
    /// underneath: unmapping instead would make the app below repaint just as
    /// the first stroke arrives, and a busy app then drops it.
    pub fn click_through(&mut self, conn: &RustConnection, on: bool) -> Result<()> {
        if on == self.click_through {
            return Ok(());
        }
        match on {
            true => conn.shape_rectangles(
                SO::SET,
                SK::INPUT,
                ClipOrdering::UNSORTED,
                self.window,
                0,
                0,
                &[],
            )?,
            false => conn.shape_mask(SO::SET, SK::INPUT, self.window, 0, 0, x11rb::NONE)?,
        };
        // A round trip: X applies the new input region before this returns,
        // so pen events sent next cannot overtake it.
        conn.get_input_focus()?.reply()?;
        self.click_through = on;
        Ok(())
    }

    /// Follows the canvas after its size changed.
    pub fn resize(&mut self, conn: &RustConnection, canvas: &Canvas) -> Result<()> {
        (self.width, self.height) = (canvas.width(), canvas.height());
        if self.mapped {
            self.configure(conn)?;
        }
        Ok(())
    }

    /// Whether a screen rectangle `[left, top, right, bottom]` overlaps the window.
    pub fn covers(&self, [left, top, right, bottom]: [i32; 4]) -> bool {
        self.mapped
            && left < self.x + self.width as i32
            && right >= self.x
            && top < self.y + self.height as i32
            && bottom >= self.y
    }

    fn in_grip(&self, (x, y): (i32, i32)) -> bool {
        x >= self.width as i32 - GRIP && y >= self.height as i32 - GRIP
    }

    /// Mouse button 1 went down inside the window.
    pub fn press(&mut self, local: (i32, i32), root: (i32, i32)) {
        self.drag = Some(match self.in_grip(local) {
            true => Drag::Resize {
                pointer: root,
                width: self.width,
            },
            false => Drag::Move {
                pointer: root,
                origin: (self.x, self.y),
            },
        });
    }

    /// The mouse moved over the window. Returns the new width while resizing.
    pub fn motion(
        &mut self,
        conn: &RustConnection,
        local: (i32, i32),
        root: (i32, i32),
    ) -> Result<Option<u32>> {
        match self.drag {
            Some(Drag::Move { pointer, origin }) => {
                (self.x, self.y) = (origin.0 + root.0 - pointer.0, origin.1 + root.1 - pointer.1);
                conn.configure_window(self.window, &ConfigureWindowAux::new().x(self.x).y(self.y))?;
                Ok(None)
            }
            Some(Drag::Resize { pointer, width }) => {
                // Follow whichever direction the corner was pulled further.
                let aspect = self.width as f64 / self.height as f64;
                let dx = f64::from(root.0 - pointer.0);
                let dy = f64::from(root.1 - pointer.1) * aspect;
                let delta = if dx.abs() > dy.abs() { dx } else { dy };
                let new = (f64::from(width) + delta).round() as i64;
                let new = new.clamp(i64::from(WIDTH_RANGE.0), i64::from(WIDTH_RANGE.1)) as u32;
                Ok((new != self.width).then_some(new))
            }
            None => {
                let in_grip = self.in_grip(local);
                if in_grip != self.resize_cursor_shown {
                    self.resize_cursor_shown = in_grip;
                    let cursor = if in_grip {
                        self.resize_cursor
                    } else {
                        self.move_cursor
                    };
                    conn.change_window_attributes(
                        self.window,
                        &ChangeWindowAttributesAux::new().cursor(cursor),
                    )?;
                }
                Ok(None)
            }
        }
    }

    /// Mouse button 1 came up. Returns true if a move or resize just ended.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// Uploads `rect` of the canvas's composed frame to the window.
    pub fn upload(&self, conn: &RustConnection, canvas: &Canvas, rect: IntRect) -> Result<()> {
        if !self.mapped || (self.width, self.height) != (canvas.width(), canvas.height()) {
            return Ok(());
        }
        let stride = canvas.width() as usize * 4;
        let row_bytes = rect.width() as usize * 4;
        // Stay well under the server's request size limit.
        let max_rows = ((conn.maximum_request_bytes() - 1024) / row_bytes).max(1);
        let pixels = canvas.frame.data();
        let mut row = rect.y() as usize;
        let end = row + rect.height() as usize;
        while row < end {
            let rows = max_rows.min(end - row);
            let mut data = Vec::with_capacity(rows * row_bytes);
            for r in row..row + rows {
                let start = r * stride + rect.x() as usize * 4;
                for px in pixels[start..start + row_bytes].as_chunks::<4>().0 {
                    match self.lsb_first {
                        true => data.extend_from_slice(&[px[2], px[1], px[0], 0]),
                        false => data.extend_from_slice(&[0, px[0], px[1], px[2]]),
                    }
                }
            }
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                self.window,
                self.gc,
                rect.width() as u16,
                rows as u16,
                rect.x() as i16,
                row as i16,
                0,
                self.depth,
                &data,
            )
            .context("uploading popup pixels")?;
            row += rows;
        }
        Ok(())
    }
}

/// Where the Popup's top-left goes the first time: below and right of the
/// Pointer, flipped to stay on screen.
pub fn position(pointer: (i32, i32), size: (u32, u32), screen: (u32, u32)) -> (i32, i32) {
    const GAP: i32 = 32;
    let (w, h) = (size.0 as i32, size.1 as i32);
    let mut x = pointer.0 + GAP;
    if x + w > screen.0 as i32 {
        x = pointer.0 - GAP - w;
    }
    let mut y = pointer.1 + GAP;
    if y + h > screen.1 as i32 {
        y = pointer.1 - GAP - h;
    }
    keep_on_screen((x, y), size, screen)
}

/// Moves a window position so the whole window is on screen.
pub fn keep_on_screen((x, y): (i32, i32), size: (u32, u32), screen: (u32, u32)) -> (i32, i32) {
    let max_x = (screen.0 as i32 - size.0 as i32).max(0);
    let max_y = (screen.1 as i32 - size.1 as i32).max(0);
    (x.clamp(0, max_x), y.clamp(0, max_y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_sits_below_right_and_flips_near_edges() {
        let screen = (2560, 1440);
        assert_eq!(position((100, 100), (960, 540), screen), (132, 132));
        assert_eq!(
            position((2400, 1300), (960, 540), screen),
            (2400 - 32 - 960, 1300 - 32 - 540)
        );
        assert_eq!(
            position((10, 1400), (960, 540), screen),
            (42, 1400 - 32 - 540)
        );
    }

    #[test]
    fn remembered_position_is_pulled_back_on_screen() {
        assert_eq!(
            keep_on_screen((2400, -50), (960, 540), (2560, 1440)),
            (1600, 0)
        );
        assert_eq!(
            keep_on_screen((100, 200), (960, 540), (2560, 1440)),
            (100, 200)
        );
    }
}
