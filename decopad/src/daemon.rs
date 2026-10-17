//! decopad's state and behaviour: modes, the Popup, Commit and key dispatch.
//! See docs/CONTEXT.md for the vocabulary.
//!
//! Pad Mode is simply "the Popup is open": opening the Popup enters it and
//! closing the Popup returns to Desktop Mode.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::Result;
use evdev::{EventSummary, KeyCode};
use fontdue::Font;
use tiny_skia::IntRect;
use x11rb::connection::Connection;

use crate::actions::{Action, Builtin, VirtualKeyboard};
use crate::config::{self, Config, Hold, INK_SCALE_RANGE};
use crate::device::{PenGeometry, Tablet};
use crate::ink::{Ink, Placement, Point};
use crate::keys::{Decoder, ExpressKey};
use crate::mode::Mode;
use crate::pen::{PenState, PenTracker};
use crate::popup::{self, Popup};
use crate::preview::Preview;
use crate::render::{self, Canvas, InkImage, Overlay};
use crate::replay::{Job, Replayer};
use crate::xconn::{HotKey, XConn, XEvent};
use crate::xmap;

/// Ink Scale that shows as 100%, and the change per press of keys 5 and 6.
const INK_SCALE_REFERENCE: f64 = 3.0;
const INK_SCALE_STEP: f64 = 0.3;
/// Gap left after delivered Ink before the next Commit, at Ink Scale.
const WORD_GAP_MM: f64 = 4.0;
const ERASER_RADIUS_MM: f32 = 2.5;
/// Height of the caret marking the Anchor, at Ink Scale.
const CARET_MM: f64 = 7.0;
/// Replayed strokes stay within this many pixels of the written ones.
const REPLAY_TOLERANCE_PX: f64 = 0.2;
/// Picture Paste presses Ctrl+V as soon as the focused app has asked what
/// the clipboard offers (apps enable Paste only after that answer). Apps
/// that do not ask in advance get Ctrl+V after this long instead.
const PASTE_FALLBACK: Duration = Duration::from_millis(100);
/// Keys in `held` for shortcuts held by the Pen Buttons, after Express Keys 1–8.
const PEN_BUTTON_KEYS: [usize; 2] = [101, 102];
const SEPARATOR: &str = "  ·  ";

pub struct Daemon {
    pub config_path: PathBuf,
    pub config: Config,
    keyboard: VirtualKeyboard,
    pub tablet: Option<Tablet>,
    geometry: PenGeometry,
    decoder: Decoder,
    pen: PenTracker,
    pen_state: PenState,
    /// Shortcut keys currently held down, by Express Key or Pen Button.
    held: HashMap<usize, Vec<KeyCode>>,
    /// A mode switch requested mid-stroke, applied when the tip lifts.
    toggle_pending: bool,
    /// Pen Buttons held as an eraser, and whether the Pen touched since.
    pending_click: [Option<bool>; 2],

    pub x: XConn,
    popup: Popup,
    /// Where the user put the Popup, if they moved it.
    popup_position: Option<(i32, i32)>,
    popup_width: u32,
    /// Esc, Backspace, Tab and Enter are taken from apps while Writing.
    keys_grabbed: bool,
    canvas: Canvas,
    font: Option<Font>,
    ink: Ink,
    eraser: bool,
    ink_scale: f64,
    /// Where the last Replay left the Pointer, and where that line started.
    continuation: Option<((i32, i32), i32)>,
    /// When the Pen last left the Tablet's range while the Popup was open.
    away_since: Option<Instant>,
    pub replayer: Replayer,
    /// Pointer target of the Replay in progress.
    replaying: Option<(i32, i32)>,
    /// Pen Tip shown in the Popup: Tablet position and whether it erases.
    tip: Option<(i32, i32, bool)>,
    drawn_ring: Option<IntRect>,
    dirty: Option<IntRect>,
    /// A Picture Paste waiting for the focused client to ask about the
    /// clipboard, and when to press Ctrl+V anyway.
    paste: Option<(Option<u32>, Instant)>,

    preview: Preview,
    /// The Pointer as last seen, which is where Ink would land now.
    pointer: (i32, i32),
    preview_stale: bool,
    /// The preview's Ink at delivery size; `None` when it must be redrawn.
    preview_image: Option<InkImage>,
    /// Pieces of strokes written since the preview last updated.
    preview_segments: Vec<(Point, Point)>,
}

impl Daemon {
    pub fn new(config_path: PathBuf, config: Config, x: XConn) -> Result<Self> {
        let popup = Popup::new(&x.conn, x.screen_num)?;
        let preview = Preview::new(&x.conn, x.screen_num)?;
        let replayer = Replayer::new(x.screen_size())?;
        xmap::disable_smoothing_in_background(crate::replay::X_DEVICE_PREFIX);
        let font = render::load_font();
        if font.is_none() {
            eprintln!("decopad: no font found with fc-match; the Cheatsheet is hidden");
        }
        let geometry = PenGeometry::default();
        let ink_scale = config::remembered_ink_scale().unwrap_or(config.popup.ink_scale);
        let (popup_position, popup_width) = match config::remembered_popup() {
            Some((position, width)) => (Some(position), width),
            None => (None, config.popup.width),
        };
        let start_mode = config.start_mode;
        let mut daemon = Daemon {
            config_path,
            canvas: canvas_for(&config, &geometry, popup_width),
            config,
            keyboard: VirtualKeyboard::new()?,
            tablet: None,
            geometry,
            decoder: Decoder::default(),
            pen: PenTracker::default(),
            pen_state: PenState::default(),
            held: HashMap::new(),
            toggle_pending: false,
            pending_click: [None; 2],
            x,
            popup,
            popup_position,
            popup_width,
            keys_grabbed: false,
            font,
            ink: Ink::default(),
            eraser: false,
            ink_scale,
            continuation: None,
            away_since: None,
            replayer,
            replaying: None,
            tip: None,
            drawn_ring: None,
            dirty: None,
            paste: None,
            preview,
            pointer: (0, 0),
            preview_stale: false,
            preview_image: None,
            preview_segments: Vec::new(),
        };
        daemon.attach();
        if start_mode == Mode::Pad {
            daemon.open_popup()?;
        }
        Ok(daemon)
    }

    fn mode(&self) -> Mode {
        match self.popup.visible {
            true => Mode::Pad,
            false => Mode::Desktop,
        }
    }

    // ----- Tablet -----

    pub fn attach(&mut self) {
        match Tablet::open() {
            Ok(Some(tablet)) => {
                let geometry = tablet.geometry();
                xmap::map_tablet_in_background(
                    tablet.pen_name.clone(),
                    geometry.max_x,
                    geometry.max_y,
                );
                if geometry != self.geometry {
                    self.geometry = geometry;
                    self.rebuild_canvas();
                    self.preview_image = None;
                }
                self.decoder = Decoder::default();
                self.pen.reset();
                self.pen_state = PenState::default();
                self.tablet = Some(tablet);
                self.take_pen_buttons();
                self.sync_pen_grab();
            }
            Ok(None) => {}
            Err(error) => eprintln!("decopad: cannot open the tablet: {error:#}"),
        }
    }

    pub fn detach(&mut self) {
        if self.tablet.take().is_some() {
            eprintln!("decopad: tablet disconnected");
            for (_, keys) in self.held.drain() {
                let _ = self.keyboard.release(&keys);
            }
            self.toggle_pending = false;
            self.pen_state = PenState::default();
            self.tip = None;
        }
    }

    /// In Desktop Mode X owns the Pen, so its side buttons would also click;
    /// switch that off for buttons decopad has an action for.
    fn take_pen_buttons(&self) {
        if let Some(tablet) = &self.tablet {
            let desktop = &self.config.desktop;
            let take = desktop
                .pen_buttons
                .clone()
                .map(|action| action != Action::None);
            // An eraser hold in Desktop Mode is the app's own eraser.
            let float_eraser = take[1] && desktop.pen_holds[1] != Hold::Eraser;
            xmap::take_pen_buttons_in_background(tablet.pen_name.clone(), take, float_eraser);
        }
    }

    /// decopad holds the Pen whenever it, not X, should receive Pen input.
    fn pen_wanted(&self) -> bool {
        self.popup.visible || self.replaying.is_some()
    }

    fn sync_pen_grab(&mut self) {
        let wanted = self.pen_wanted();
        if let Some(tablet) = &mut self.tablet
            && let Err(error) = tablet.grab_pen(wanted)
        {
            eprintln!("decopad: cannot switch the pen: {error:#}");
        }
    }

    /// The Popup's keys are taken only while Writing, so typing in other
    /// apps keeps working while the Popup stays open.
    pub fn sync_key_grab(&mut self) -> Result<()> {
        let wanted = self.popup.visible && (self.pen_state.in_range || !self.ink.is_empty());
        if wanted != self.keys_grabbed {
            self.x.grab_popup_keys(wanted)?;
            self.keys_grabbed = wanted;
        }
        Ok(())
    }

    /// Reads whatever the Tablet has queued. Returns false once it is gone.
    pub fn read_tablet(&mut self) -> Result<bool> {
        let Some(tablet) = &mut self.tablet else {
            return Ok(true);
        };

        let mut key_changes = Vec::new();
        let mut key_reports = Vec::new();
        match tablet.keyboard.fetch_events() {
            Ok(events) => {
                for event in events {
                    match event.destructure() {
                        EventSummary::Key(_, code, value) => key_changes.push((code, value)),
                        EventSummary::Synchronization(..) if !key_changes.is_empty() => {
                            key_reports.push(std::mem::take(&mut key_changes))
                        }
                        _ => {}
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return Ok(false),
        }

        let mut pen_reports = Vec::new();
        match tablet.pen.fetch_events() {
            Ok(events) => pen_reports.extend(events.filter_map(|e| self.pen.feed(e))),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return Ok(false),
        }

        if let Some(mouse) = &mut tablet.mouse {
            match mouse.fetch_events() {
                Ok(events) => events.for_each(drop),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => return Ok(false),
            }
        }

        for (previous, current) in pen_reports {
            self.pen_state = current;
            self.on_pen(previous, current)?;
        }
        for report in key_reports {
            for event in self.decoder.feed(&report) {
                self.on_express_key(event)?;
            }
        }
        Ok(true)
    }

    // ----- Modes and keys -----

    /// Pad Mode opens the Popup, Desktop Mode closes it (keeping its Ink).
    fn set_mode(&mut self, mode: Mode) -> Result<()> {
        match mode {
            Mode::Pad if self.replaying.is_none() => self.open_popup()?,
            Mode::Pad => return Ok(()),
            Mode::Desktop => self.close_popup()?,
        }
        eprintln!("decopad: {mode}");
        Ok(())
    }

    fn toggle_mode(&mut self) -> Result<()> {
        self.set_mode(self.mode().toggled())
    }

    /// Runs `action` for a key going down; shortcuts stay held under `slot`
    /// until `release(slot)`.
    fn press(&mut self, slot: usize, action: Action) -> Result<()> {
        match action {
            Action::None => {}
            Action::TogglePad if self.pen_state.touching => self.toggle_pending = true,
            Action::TogglePad => self.toggle_mode()?,
            Action::Shortcut(keys) => {
                self.keyboard.press(&keys)?;
                self.held.insert(slot, keys);
            }
            Action::Builtin(action) => self.builtin(action)?,
        }
        Ok(())
    }

    fn release(&mut self, slot: usize) -> Result<()> {
        if let Some(keys) = self.held.remove(&slot) {
            self.keyboard.release(&keys)?;
        }
        Ok(())
    }

    fn on_express_key(&mut self, event: ExpressKey) -> Result<()> {
        match event {
            ExpressKey::Pressed(key) => {
                let action = self.config.table(self.mode()).keys[key - 1].clone();
                self.press(key, action)
            }
            ExpressKey::Released(key) => self.release(key),
        }
    }

    fn tap(&mut self, keys: &[KeyCode]) -> Result<()> {
        self.keyboard.press(keys)?;
        self.keyboard.release(keys)
    }

    fn builtin(&mut self, action: Builtin) -> Result<()> {
        match action {
            Builtin::Commit if self.popup.visible => self.commit(false)?,
            Builtin::NewLine if self.popup.visible => self.commit(true)?,
            Builtin::PicturePaste if self.popup.visible => self.picture_paste()?,
            Builtin::Undo if self.popup.visible && self.ink.undo() => self.ink_changed(),
            Builtin::Undo => self.tap(&[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_Z])?,
            Builtin::Redo if self.popup.visible && self.ink.redo() => self.ink_changed(),
            Builtin::Redo => self.tap(&[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_Y])?,
            Builtin::InkSmaller => self.adjust_ink_scale(-INK_SCALE_STEP),
            Builtin::InkLarger => self.adjust_ink_scale(INK_SCALE_STEP),
            _ => {}
        }
        Ok(())
    }

    pub fn on_x_event(&mut self, event: XEvent) -> Result<()> {
        match event {
            XEvent::Expose => self.dirty = Some(self.canvas.full()),
            XEvent::Press { local, root } => self.popup.press(local, root),
            XEvent::Motion { local, root } => {
                if let Some(width) = self.popup.motion(&self.x.conn, local, root)? {
                    self.popup_width = width;
                    self.rebuild_canvas();
                    self.popup.resize(&self.x.conn, &self.canvas)?;
                }
            }
            XEvent::Release => {
                if self.popup.release() {
                    self.popup_position = Some((self.popup.x, self.popup.y));
                    config::remember_popup((self.popup.x, self.popup.y), self.popup_width);
                    self.preview_stale = true;
                }
            }
            XEvent::PointerMoved if self.popup.visible && self.replaying.is_none() => {
                let pointer = self.x.pointer()?;
                if pointer != self.pointer {
                    self.pointer = pointer;
                    self.preview_stale = true;
                }
            }
            XEvent::PointerMoved => {}
            XEvent::ClipboardAsked { client } => {
                if self
                    .paste
                    .is_some_and(|(focused, _)| focused == Some(client))
                {
                    self.paste_now()?;
                }
            }
            XEvent::Key(HotKey::PopupToggle) => self.toggle_mode()?,
            XEvent::Key(_) if !self.popup.visible => {}
            XEvent::Key(HotKey::Escape) => self.set_mode(Mode::Desktop)?,
            XEvent::Key(HotKey::Backspace) => {
                self.ink.clear();
                self.ink_changed();
            }
            XEvent::Key(HotKey::Tab) => {
                self.eraser = !self.eraser;
                self.redraw();
            }
            XEvent::Key(HotKey::Enter) => self.commit(true)?,
        }
        Ok(())
    }

    // ----- Pen and Popup -----

    fn on_pen(&mut self, previous: PenState, now: PenState) -> Result<()> {
        if previous.touching && !now.touching && self.toggle_pending {
            self.toggle_pending = false;
            self.toggle_mode()?;
        }
        if self.replaying.is_some() {
            return Ok(());
        }
        // In Desktop Mode X moves the Pointer; decopad only runs the side
        // buttons, whose own X action it switched off.
        self.pen_buttons(previous, now)?;
        if !self.popup.visible || self.replaying.is_some() {
            return Ok(());
        }

        let erasing = self.eraser || self.pending_click.iter().any(Option::is_some);
        let point = |s: PenState| Point {
            x: s.x,
            y: s.y,
            pressure: s.pressure as f32 / self.geometry.max_pressure.max(1) as f32,
        };
        if now.touching && erasing {
            let radius = (ERASER_RADIUS_MM * self.geometry.units_per_mm) as i32;
            if self.ink.erase_near(now.x, now.y, radius) {
                self.ink_changed();
            }
        } else if now.touching && !previous.touching {
            self.ink.begin(point(now));
            let rect = self.canvas.segment(point(now), point(now));
            self.dirty = render::union(self.dirty, rect);
            self.preview_segments.push((point(now), point(now)));
            self.preview_stale = true;
        } else if now.touching {
            self.ink.extend(point(now));
            let rect = self.canvas.segment(point(previous), point(now));
            self.dirty = render::union(self.dirty, rect);
            self.preview_segments.push((point(previous), point(now)));
            self.preview_stale = true;
        }

        self.tip = now.in_range.then_some((now.x, now.y, erasing));
        if now.in_range {
            self.away_since = None;
        } else if previous.in_range {
            self.away_since = Some(Instant::now());
        }
        Ok(())
    }

    /// A Pen Button with an action on press runs it at once (holding a
    /// shortcut while held). One that also erases while held runs its action
    /// on release instead, and only if the Pen did not touch the Tablet.
    fn pen_buttons(&mut self, previous: PenState, now: PenState) -> Result<()> {
        let changes = [
            (previous.button1, now.button1),
            (previous.button2, now.button2),
        ];
        for (i, (was, is)) in changes.into_iter().enumerate() {
            let table = self.config.table(self.mode());
            let (action, hold) = (table.pen_buttons[i].clone(), table.pen_holds[i]);
            let slot = PEN_BUTTON_KEYS[i];
            match (was, is) {
                (false, true) if hold == Hold::Eraser => self.pending_click[i] = Some(false),
                (false, true) => self.press(slot, action)?,
                (true, false) => match self.pending_click[i].take() {
                    Some(false) => {
                        self.press(slot, action)?;
                        self.release(slot)?;
                    }
                    Some(true) => {}
                    None => self.release(slot)?,
                },
                _ => {}
            }
            if now.touching
                && let Some(touched) = &mut self.pending_click[i]
            {
                *touched = true;
            }
        }
        Ok(())
    }

    fn open_popup(&mut self) -> Result<()> {
        if self.popup.visible {
            return Ok(());
        }
        let size = (self.canvas.width(), self.canvas.height());
        let screen = self.x.screen_size();
        self.pointer = self.x.pointer()?;
        let (x, y) = match self.popup_position {
            Some(position) => popup::keep_on_screen(position, size, screen),
            None => popup::position(self.pointer, size, screen),
        };
        self.redraw();
        self.popup.show(&self.x.conn, x, y, &self.canvas)?;
        self.away_since = None;
        self.drawn_ring = None;
        self.preview_stale = true;
        self.x.watch_pointer(true)?;
        self.sync_pen_grab();
        Ok(())
    }

    fn close_popup(&mut self) -> Result<()> {
        self.popup.hide(&self.x.conn)?;
        self.preview.hide(&self.x.conn)?;
        self.x.watch_pointer(false)?;
        self.tip = None;
        self.away_since = None;
        self.eraser = false;
        self.sync_pen_grab();
        self.sync_key_grab()
    }

    fn rebuild_canvas(&mut self) {
        self.canvas = canvas_for(&self.config, &self.geometry, self.popup_width);
        self.drawn_ring = None;
        self.redraw();
    }

    /// The Popup's legend: the Tablet's keys on the first line; the Pen
    /// Buttons, the keyboard and the Ink size on the second.
    fn cheatsheet(&self) -> Vec<String> {
        let pad = &self.config.pad;
        let mut pen = Vec::new();
        let clicks: Vec<String> = pad
            .pen_buttons
            .iter()
            .filter_map(|a| a.label(true))
            .collect();
        if !clicks.is_empty() {
            pen.push(clicks.join(" / "));
        }
        for (name, hold) in ["lower", "upper"].iter().zip(pad.pen_holds) {
            if hold == Hold::Eraser {
                pen.push(format!("hold {name} to erase"));
            }
        }
        let percent = (self.ink_scale / INK_SCALE_REFERENCE * 100.0).round();
        let mut second = Vec::new();
        if !pen.is_empty() {
            second.push(format!("Pen  {}", pen.join(SEPARATOR)));
        }
        second.push(format!("Ink {percent}%"));
        let keyboard = [
            "Enter New line",
            "Backspace Clear",
            "Tab Eraser",
            "Esc Close",
        ];
        vec![
            format!("Keys  {}", key_summary(&pad.keys, true)),
            second.join(SEPARATOR),
            format!("Keyboard  {}", keyboard.join(SEPARATOR)),
        ]
    }

    fn redraw(&mut self) {
        let cheatsheet = self.cheatsheet();
        let overlay = Overlay {
            cheatsheet: &cheatsheet,
            eraser: self.eraser,
        };
        self.canvas.redraw(&self.ink, &overlay, self.font.as_ref());
        self.dirty = Some(self.canvas.full());
    }

    /// Repaints the Popup and the preview after Ink changed other than by
    /// adding to a stroke.
    fn ink_changed(&mut self) {
        self.redraw();
        self.preview_image = None;
        self.preview_stale = true;
    }

    /// Sends changed pixels and the Pen Tip ring to the Popup window.
    pub fn present(&mut self) -> Result<()> {
        if !self.popup.visible {
            self.dirty = None;
            return Ok(());
        }
        let eraser_radius = (ERASER_RADIUS_MM * self.geometry.units_per_mm) as i32;
        let ring = self.tip.map(|(x, y, erasing)| {
            let (cx, cy, r, rect) = self.canvas.tip_ring(x, y, erasing, eraser_radius);
            ((cx, cy, r, erasing), rect)
        });
        let rect = render::union(
            self.dirty.take(),
            render::union(self.drawn_ring, ring.map(|(_, r)| r)),
        );
        self.drawn_ring = ring.map(|(_, r)| r);
        if let Some(rect) = rect.and_then(|r| self.canvas.clip(r)) {
            self.canvas.compose(rect, ring.map(|(params, _)| params));
            self.popup.upload(&self.x.conn, &self.canvas, rect)?;
        }
        if self.preview_stale {
            self.update_preview()?;
        }
        Ok(())
    }

    /// Shows where Ink would land right now: a caret at the Pointer and the
    /// Ink itself at delivery size. Hidden during a Replay and while the
    /// Pointer is over the Popup, where there is nothing to write into.
    fn update_preview(&mut self) -> Result<()> {
        self.preview_stale = false;
        let (px, py) = self.pointer;
        let over_popup = px >= self.popup.x
            && px < self.popup.x + self.popup.width as i32
            && py >= self.popup.y
            && py < self.popup.y + self.popup.height as i32;
        if !self.popup.visible || self.replaying.is_some() || over_popup {
            return self.preview.hide(&self.x.conn);
        }
        let caret = (CARET_MM * self.ink_scale).round().max(4.0) as u32;
        self.preview.show_caret(&self.x.conn, self.pointer, caret)?;

        let segments = std::mem::take(&mut self.preview_segments);
        if self.ink.is_empty() {
            return self.preview.hide_ink(&self.x.conn);
        }
        // The image covers the whole Tablet at delivery size, so new Ink is
        // drawn into it incrementally and moving the Anchor only moves it.
        match &mut self.preview_image {
            Some(image) => {
                for (a, b) in segments {
                    if let Some(rect) = image.segment(a, b) {
                        self.preview.add_ink(&self.x.conn, &image.pixmap, rect)?;
                    }
                }
            }
            None => {
                let g = self.geometry;
                let image = InkImage::new(
                    (0, 0),
                    (g.max_x, g.max_y),
                    self.px_per_unit(),
                    g.units_per_mm,
                );
                if let Some(mut image) = image {
                    image.draw(&self.ink);
                    self.preview.set_ink(&self.x.conn, &image.pixmap)?;
                    self.preview_image = Some(image);
                }
            }
        }
        let line_spacing_units = line_spacing_units(&self.config, &self.geometry);
        let placement = Placement::new(
            &self.ink,
            self.pointer,
            self.px_per_unit(),
            line_spacing_units,
        );
        if let (Some(placement), Some(image)) = (placement, &self.preview_image) {
            let (x, y) = placement.to_screen(Point {
                x: 0,
                y: 0,
                pressure: 0.0,
            });
            let pad = f64::from(image.pad);
            let at = ((x - pad).round() as i32, (y - pad).round() as i32);
            self.preview.place_ink(&self.x.conn, at)?;
        }
        self.x.conn.flush()?;
        Ok(())
    }

    // ----- Commit -----

    fn px_per_unit(&self) -> f64 {
        self.ink_scale / f64::from(self.geometry.units_per_mm)
    }

    /// Replays the Ink at the Pointer (the Anchor), then leaves the Pointer
    /// after it, or at the start of the next line. The Popup stays open.
    fn commit(&mut self, new_line: bool) -> Result<()> {
        if self.replaying.is_some() {
            return Ok(());
        }
        let anchor = self.x.pointer()?;
        // Moving the Pointer since the last Replay starts a new line origin.
        let line_start = match self.continuation {
            Some((at, start)) if (at.0 - anchor.0).abs() <= 2 && (at.1 - anchor.1).abs() <= 2 => {
                start
            }
            _ => anchor.0,
        };
        let line_spacing_units = line_spacing_units(&self.config, &self.geometry);
        let placement = Placement::new(&self.ink, anchor, self.px_per_unit(), line_spacing_units);
        let next = match (new_line, &placement) {
            (true, _) => {
                let line_height =
                    (self.config.popup.line_spacing_mm * self.ink_scale).round() as i32;
                (line_start, anchor.1 + line_height)
            }
            (false, Some(placement)) => {
                let gap = (WORD_GAP_MM * self.ink_scale).round() as i32;
                (anchor.0 + placement.width_px.round() as i32 + gap, anchor.1)
            }
            (false, None) => return Ok(()),
        };
        let strokes = placement
            .map(|p| render::replay_points(&self.ink, &p, REPLAY_TOLERANCE_PX))
            .unwrap_or_default();

        // Strokes landing under the Popup would hit it, not the app below.
        let mut area = [next.0, next.1, next.0, next.1];
        for &(x, y, _) in strokes.iter().flatten() {
            area = [
                area[0].min(x as i32),
                area[1].min(y as i32),
                area[2].max(x as i32 + 1),
                area[3].max(y as i32 + 1),
            ];
        }
        if self.popup.covers(area) {
            self.popup.click_through(&self.x.conn, true)?;
        }

        self.replaying = Some(next);
        self.preview.hide(&self.x.conn)?;
        self.replayer.start(Job {
            strokes,
            finish_at: next,
            interval: Duration::from_millis(self.config.popup.replay_interval_ms),
        });
        self.continuation = Some((next, line_start));
        self.ink.clear();
        self.preview_image = None;
        self.tip = None;
        self.away_since = None;
        self.redraw();
        Ok(())
    }

    pub fn on_replay_done(&mut self) -> Result<()> {
        if self.replayer.finished() == 0 {
            return Ok(());
        }
        if let Some((x, y)) = self.replaying.take() {
            self.x.warp(x, y)?;
            self.pointer = (x, y);
        }
        self.popup.click_through(&self.x.conn, false)?;
        self.preview_stale = true;
        self.sync_pen_grab();
        Ok(())
    }

    /// Puts the Ink on the clipboard as an image and presses Ctrl+V.
    fn picture_paste(&mut self) -> Result<()> {
        let units_per_mm = self.geometry.units_per_mm;
        let Some((pixmap, _)) = render::picture(&self.ink, self.px_per_unit(), units_per_mm) else {
            return Ok(());
        };
        let png = pixmap.encode_png()?;
        self.x.own_clipboard(self.popup.window, png)?;
        self.ink.clear();
        self.ink_changed();
        self.paste = Some((self.x.focused_client()?, Instant::now() + PASTE_FALLBACK));
        Ok(())
    }

    /// When the pause Commit fires, if one is due.
    fn commit_deadline(&self) -> Option<Instant> {
        let delay = self.config.popup.commit_delay_ms;
        if delay == 0 || !self.popup.visible || self.ink.is_empty() || self.replaying.is_some() {
            return None;
        }
        Some(self.away_since? + Duration::from_millis(delay))
    }

    /// The next moment something is scheduled to happen.
    pub fn next_deadline(&self) -> Option<Instant> {
        [self.commit_deadline(), self.paste.map(|(_, at)| at)]
            .into_iter()
            .flatten()
            .min()
    }

    /// Runs whatever was scheduled for now or earlier.
    pub fn run_due(&mut self) -> Result<()> {
        let now = Instant::now();
        if self.paste.is_some_and(|(_, at)| now >= at) {
            self.paste_now()?;
        }
        if self.commit_deadline().is_some_and(|due| now >= due) {
            self.commit(false)?;
        }
        Ok(())
    }

    fn paste_now(&mut self) -> Result<()> {
        self.paste = None;
        self.tap(&[KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V])
    }

    fn adjust_ink_scale(&mut self, delta: f64) {
        let scale = ((self.ink_scale + delta) * 100.0).round() / 100.0;
        let scale = scale.clamp(INK_SCALE_RANGE.0, INK_SCALE_RANGE.1);
        if scale == self.ink_scale {
            return;
        }
        self.ink_scale = scale;
        config::remember_ink_scale(scale);
        self.preview_image = None;
        self.preview_stale = true;
        if self.popup.visible {
            self.redraw();
        }
    }

    // ----- Config -----

    pub fn reload_config(&mut self) {
        match config::load_or_create(&self.config_path) {
            Ok(config) if config == self.config => {}
            Ok(config) => {
                let old = std::mem::replace(&mut self.config, config);
                if self.config.popup.ink_scale != old.popup.ink_scale {
                    self.ink_scale = self.config.popup.ink_scale;
                    config::remember_ink_scale(self.ink_scale);
                }
                if self.config.popup.width != old.popup.width {
                    self.popup_width = self.config.popup.width;
                }
                if self.config.desktop.pen_buttons != old.desktop.pen_buttons
                    || self.config.desktop.pen_holds != old.desktop.pen_holds
                {
                    self.take_pen_buttons();
                }
                self.rebuild_canvas();
                self.preview_image = None;
                self.preview_stale = true;
                if let Err(error) = self.popup.resize(&self.x.conn, &self.canvas) {
                    eprintln!("decopad: {error:#}");
                }
                eprintln!("decopad: config reloaded");
            }
            Err(error) => {
                eprintln!("decopad: keeping the previous config: {error:#}");
                notify(&format!(
                    "Config error, keeping the previous settings:\n{error:#}"
                ));
            }
        }
    }
}

/// "1 Close  ·  2 Commit  ·  …  ·  7/8 Undo / Redo" for the keys that do
/// something, with matching neighbours merged.
fn key_summary(keys: &[Action; 8], pad_open: bool) -> String {
    let labels: Vec<Option<String>> = keys.iter().map(|a| a.label(pad_open)).collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < labels.len() {
        let pair = labels
            .get(i + 1)
            .and_then(|next| pair_label(labels[i].as_deref()?, next.as_deref()?));
        match (pair, &labels[i]) {
            (Some(pair), _) => {
                parts.push(format!("{}/{} {pair}", i + 1, i + 2));
                i += 2;
                continue;
            }
            (None, Some(label)) => parts.push(format!("{} {label}", i + 1)),
            (None, None) => {}
        }
        i += 1;
    }
    parts.join(SEPARATOR)
}

/// One name for two neighbouring keys that do opposite things.
fn pair_label(first: &str, second: &str) -> Option<&'static str> {
    match (first, second) {
        ("Smaller ink", "Larger ink") => Some("Ink size"),
        ("Zoom out", "Zoom in") => Some("Zoom"),
        ("Undo", "Redo") => Some("Undo / Redo"),
        _ => None,
    }
}

fn line_spacing_units(config: &Config, geometry: &PenGeometry) -> i32 {
    (config.popup.line_spacing_mm * f64::from(geometry.units_per_mm)).round() as i32
}

fn canvas_for(config: &Config, geometry: &PenGeometry, width: u32) -> Canvas {
    Canvas::new(
        width,
        (geometry.max_x, geometry.max_y),
        geometry.units_per_mm,
        line_spacing_units(config, geometry),
    )
}

/// Shows a short desktop notification that replaces the previous one. Used
/// only for config errors, which would otherwise go unnoticed.
pub fn notify(text: &str) {
    let (title, body) = text.split_once('\n').unwrap_or((text, ""));
    let spawned = Command::new("notify-send")
        .args(["--app-name=decopad", "--expire-time=2500"])
        .args([
            "--hint=string:x-canonical-private-synchronous:decopad",
            title,
            body,
        ])
        .spawn();
    if let Ok(mut child) = spawned {
        std::thread::spawn(move || child.wait());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_legends_merge_paired_keys() {
        let config = config::parse(config::DEFAULT).unwrap();
        assert_eq!(
            key_summary(&config.pad.keys, true),
            "1 Close  ·  2 Commit  ·  3 Paste as picture  ·  4 New line  ·  5/6 Ink size  ·  7/8 Undo / Redo"
        );
        assert_eq!(
            key_summary(&config.desktop.keys, false),
            "1 Writing pad  ·  2 Right click  ·  3 Paste  ·  4 Enter  ·  5/6 Zoom  ·  7/8 Undo / Redo"
        );
    }
}
