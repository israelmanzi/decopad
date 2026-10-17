//! decopad's X connection: the Pointer, global key grabs and the clipboard.

use anyhow::{Context, Result};
use x11rb::CURRENT_TIME;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, GrabMode, Keycode, ModMask, PropMode,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// Keys decopad listens for on the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotKey {
    /// Super+W, always.
    PopupToggle,
    /// The rest only while the Popup is open.
    Escape,
    Backspace,
    Tab,
    Enter,
}

const XK_W: u32 = 0x0077;
const XK_ESCAPE: u32 = 0xff1b;
const XK_BACKSPACE: u32 = 0xff08;
const XK_TAB: u32 = 0xff09;
const XK_RETURN: u32 = 0xff0d;

pub enum XEvent {
    Key(HotKey),
    Expose,
    /// Mouse button 1 in the Popup, in window and screen coordinates.
    Press {
        local: (i32, i32),
        root: (i32, i32),
    },
    Motion {
        local: (i32, i32),
        root: (i32, i32),
    },
    Release,
    /// The Pointer moved (reported only while `watch_pointer` is on).
    PointerMoved,
    /// The X client owning `client` asked what the clipboard offers.
    ClipboardAsked {
        client: u32,
    },
}

pub struct XConn {
    pub conn: RustConnection,
    pub screen_num: usize,
    pub root: Window,
    keys: Vec<(Keycode, HotKey)>,
    clipboard: Atom,
    targets: Atom,
    png: Atom,
    clipboard_png: Option<Vec<u8>>,
    /// Bits of a resource ID that identify the client owning it.
    client_bits: u32,
}

/// Lock-key combinations to grab alongside each shortcut, so Caps Lock and
/// Num Lock do not stop it from working.
fn lock_variants(base: ModMask) -> [ModMask; 4] {
    [
        base,
        base | ModMask::LOCK,
        base | ModMask::M2,
        base | ModMask::LOCK | ModMask::M2,
    ]
}

impl XConn {
    pub fn connect() -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).context("connecting to X")?;
        let root = conn.setup().roots[screen_num].root;
        let atom =
            |name: &[u8]| -> Result<Atom> { Ok(conn.intern_atom(false, name)?.reply()?.atom) };
        let (clipboard, targets, png) =
            (atom(b"CLIPBOARD")?, atom(b"TARGETS")?, atom(b"image/png")?);

        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let per = mapping.keysyms_per_keycode as usize;
        let keycode = |keysym: u32| {
            mapping
                .keysyms
                .chunks(per)
                .position(|syms| syms.first() == Some(&keysym))
                .map(|i| min + i as u8)
        };
        let mut keys = Vec::new();
        for (keysym, key) in [
            (XK_W, HotKey::PopupToggle),
            (XK_ESCAPE, HotKey::Escape),
            (XK_BACKSPACE, HotKey::Backspace),
            (XK_TAB, HotKey::Tab),
            (XK_RETURN, HotKey::Enter),
        ] {
            match keycode(keysym) {
                Some(code) => keys.push((code, key)),
                None => eprintln!("decopad: no keycode for {key:?}"),
            }
        }

        let conn_setup_mask = conn.setup().resource_id_mask;
        conn.xinput_xi_query_version(2, 0)?
            .reply()
            .context("X has no XInput 2")?;
        let x = XConn {
            conn,
            screen_num,
            root,
            keys,
            clipboard,
            targets,
            png,
            clipboard_png: None,
            client_bits: !conn_setup_mask,
        };
        x.grab(HotKey::PopupToggle, ModMask::M4, true)?;
        Ok(x)
    }

    pub fn screen_size(&self) -> (u32, u32) {
        let screen = &self.conn.setup().roots[self.screen_num];
        (
            screen.width_in_pixels.into(),
            screen.height_in_pixels.into(),
        )
    }

    fn grab(&self, key: HotKey, modifiers: ModMask, grab: bool) -> Result<()> {
        let Some(&(code, _)) = self.keys.iter().find(|(_, k)| *k == key) else {
            return Ok(());
        };
        for mods in lock_variants(modifiers) {
            let cookie = match grab {
                true => self.conn.grab_key(
                    true,
                    self.root,
                    mods,
                    code,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC,
                )?,
                false => self.conn.ungrab_key(code, self.root, mods)?,
            };
            if let Err(error) = cookie.check() {
                eprintln!("decopad: cannot grab {key:?}: another program holds it ({error})");
                break;
            }
        }
        Ok(())
    }

    /// Takes Esc, Backspace, Tab and Enter from apps while the Popup is open.
    pub fn grab_popup_keys(&self, grab: bool) -> Result<()> {
        for key in [
            HotKey::Escape,
            HotKey::Backspace,
            HotKey::Tab,
            HotKey::Enter,
        ] {
            self.grab(key, ModMask::from(0u16), grab)?;
        }
        self.conn.flush()?;
        Ok(())
    }

    /// Asks X to report Pointer motion (`XEvent::PointerMoved`) or to stop.
    pub fn watch_pointer(&self, on: bool) -> Result<()> {
        let mask = if on {
            XIEventMask::RAW_MOTION
        } else {
            XIEventMask::from(0u32)
        };
        let masks = [xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![mask],
        }];
        self.conn.xinput_xi_select_events(self.root, &masks)?;
        self.conn.flush()?;
        Ok(())
    }

    /// The X client whose window has the keyboard focus, if any.
    pub fn focused_client(&self) -> Result<Option<u32>> {
        let focus = self.conn.get_input_focus()?.reply()?.focus;
        // 0 is None and 1 is PointerRoot: no particular client.
        Ok((focus > 1).then_some(focus & self.client_bits))
    }

    pub fn pointer(&self) -> Result<(i32, i32)> {
        let reply = self.conn.query_pointer(self.root)?.reply()?;
        Ok((reply.root_x.into(), reply.root_y.into()))
    }

    pub fn warp(&self, x: i32, y: i32) -> Result<()> {
        self.conn
            .warp_pointer(x11rb::NONE, self.root, 0, 0, 0, 0, x as i16, y as i16)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Makes decopad the clipboard owner, offering `png` as an image.
    pub fn own_clipboard(&mut self, owner: Window, png: Vec<u8>) -> Result<()> {
        if png.len() + 1024 > self.conn.maximum_request_bytes() {
            anyhow::bail!(
                "the picture is too large for the X server ({} bytes)",
                png.len()
            );
        }
        self.conn
            .set_selection_owner(owner, self.clipboard, CURRENT_TIME)?;
        self.clipboard_png = Some(png);
        self.conn.flush()?;
        Ok(())
    }

    fn answer_selection(&mut self, request: SelectionRequestEvent) -> Result<()> {
        let property = match request.property {
            x11rb::NONE => request.target,
            property => property,
        };
        let answered = match (&self.clipboard_png, request.selection == self.clipboard) {
            (Some(_), true) if request.target == self.targets => {
                self.conn.change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &[self.targets, self.png],
                )?;
                true
            }
            (Some(png), true) if request.target == self.png => {
                self.conn.change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    self.png,
                    png,
                )?;
                true
            }
            _ => false,
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: if answered { property } else { x11rb::NONE },
        };
        self.conn
            .send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
        self.conn.flush()?;
        Ok(())
    }

    /// Handles everything X has queued, returning what decopad must act on.
    pub fn events(&mut self) -> Result<Vec<XEvent>> {
        let mut out = Vec::new();
        while let Some(event) = self.conn.poll_for_event()? {
            match event {
                Event::KeyPress(press) => {
                    if let Some(&(_, key)) =
                        self.keys.iter().find(|(code, _)| *code == press.detail)
                    {
                        out.push(XEvent::Key(key));
                    }
                }
                Event::Expose(expose) if expose.count == 0 => out.push(XEvent::Expose),
                Event::ButtonPress(press) if press.detail == 1 => out.push(XEvent::Press {
                    local: (press.event_x.into(), press.event_y.into()),
                    root: (press.root_x.into(), press.root_y.into()),
                }),
                Event::ButtonRelease(release) if release.detail == 1 => out.push(XEvent::Release),
                Event::MotionNotify(motion) => {
                    let event = XEvent::Motion {
                        local: (motion.event_x.into(), motion.event_y.into()),
                        root: (motion.root_x.into(), motion.root_y.into()),
                    };
                    // Only the latest position of a burst of motion matters.
                    match out.last_mut() {
                        Some(last @ XEvent::Motion { .. }) => *last = event,
                        _ => out.push(event),
                    }
                }
                Event::SelectionRequest(request) => {
                    if request.target == self.targets && self.clipboard_png.is_some() {
                        let client = request.requestor & self.client_bits;
                        out.push(XEvent::ClipboardAsked { client });
                    }
                    self.answer_selection(request)?
                }
                Event::XinputRawMotion(_) => {
                    if !out.iter().any(|e| matches!(e, XEvent::PointerMoved)) {
                        out.push(XEvent::PointerMoved);
                    }
                }
                Event::SelectionClear(_) => self.clipboard_png = None,
                _ => {}
            }
        }
        self.conn.flush()?;
        Ok(out)
    }
}
