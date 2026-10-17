//! Settings decopad applies to pens through the X wacom driver.
//!
//! - The Tablet's area keeps the screen's aspect ratio in Desktop Mode.
//! - The Replay pen has smoothing off, since the driver's default averaging of
//!   4 samples makes replayed strokes lag behind their real positions.

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _, XIChangePropertyAux};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode};

/// The largest centred region of a `tablet_w`×`tablet_h` area (max coordinates)
/// with the screen's aspect ratio, as `[left, top, right, bottom]`.
pub fn aspect_area(tablet_w: i32, tablet_h: i32, screen_w: u32, screen_h: u32) -> [i32; 4] {
    let (tw, th) = (tablet_w as i64 + 1, tablet_h as i64 + 1);
    let (sw, sh) = (screen_w as i64, screen_h as i64);
    let (w, h) = if tw * sh > th * sw {
        (th * sw / sh, th)
    } else {
        (tw, tw * sh / sw)
    };
    let left = (tw - w) / 2;
    let top = (th - h) / 2;
    [
        left as i32,
        top as i32,
        (left + w - 1) as i32,
        (top + h - 1) as i32,
    ]
}

/// `pen_name` is the Pen's evdev name; X names its devices "… stylus" and "… eraser".
pub fn map_tablet_in_background(pen_name: String, tablet_w: i32, tablet_h: i32) {
    retry_in_background("tablet area", move || {
        let (conn, screen_num) = x11rb::connect(None).context("connecting to X")?;
        let screen = &conn.setup().roots[screen_num];
        let area = aspect_area(
            tablet_w,
            tablet_h,
            screen.width_in_pixels.into(),
            screen.height_in_pixels.into(),
        );
        set_property(
            &conn,
            &pen_name,
            b"Wacom Tablet Area",
            &area.map(|v| v as u32),
        )?;
        Ok(format!("{area:?}"))
    });
}

pub fn disable_smoothing_in_background(prefix: &'static str) {
    retry_in_background("replay pen smoothing", move || {
        let (conn, _) = x11rb::connect(None).context("connecting to X")?;
        // [suppress, raw sample]: no suppression, no averaging.
        set_property(&conn, prefix, b"Wacom Sample and Suppress", &[0, 1])?;
        Ok("off".into())
    });
}

/// Turns the X driver's own handling of the Pen's side buttons off where
/// decopad has an action for them in Desktop Mode, and back on elsewhere.
/// The lower button is X button 2. The Deco 640's upper button reports the
/// eraser tool. With `float_eraser`, X's "… eraser" device is floated
/// (detached from the core pointer) so apps never see an eraser while it is
/// held; otherwise it stays attached and apps erase with it. Disabling it
/// instead would also silence the stylus, which shares its input. X button 3
/// covers pens whose upper button is an ordinary button.
pub fn take_pen_buttons_in_background(pen_name: String, take: [bool; 2], float_eraser: bool) {
    retry_in_background("pen buttons", move || {
        let stylus = format!("{pen_name} stylus");
        for (button, take) in [("2", take[0]), ("3", take[1])] {
            let action = if take { "0" } else { button };
            let set = Command::new("xsetwacom")
                .args(["set", &stylus, "Button", button, action])
                .stderr(Stdio::null())
                .status()
                .context("running xsetwacom")?;
            if !set.success() {
                bail!("X has not added {stylus} yet");
            }
        }
        // Pens with a real upper button have no eraser device; that is fine.
        let eraser = format!("{pen_name} eraser");
        let args: &[&str] = match float_eraser {
            true => &["float", &eraser],
            false => &["reattach", &eraser, "Virtual core pointer"],
        };
        let _ = Command::new("xinput")
            .args(args)
            .stderr(Stdio::null())
            .status();
        Ok(format!("handled by decopad: {take:?}"))
    });
}

/// Runs `apply` until it succeeds, giving X time to add a new device.
fn retry_in_background(what: &'static str, apply: impl Fn() -> Result<String> + Send + 'static) {
    thread::spawn(move || {
        let mut last_error = None;
        for _ in 0..20 {
            match apply() {
                Ok(result) => {
                    eprintln!("decopad: {what} {result}");
                    return;
                }
                Err(error) => last_error = Some(error),
            }
            thread::sleep(Duration::from_millis(500));
        }
        if let Some(error) = last_error {
            eprintln!("decopad: could not set {what}: {error:#}");
        }
    });
}

fn set_property(conn: &impl Connection, prefix: &str, name: &[u8], values: &[u32]) -> Result<()> {
    let property = conn.intern_atom(true, name)?.reply()?.atom;
    if property == u32::from(AtomEnum::NONE) {
        bail!("the X wacom driver is not loaded");
    }
    let devices = conn
        .xinput_xi_query_device(xinput::Device::ALL)?
        .reply()?
        .infos;
    let targets: Vec<_> = devices
        .iter()
        .filter(|d| d.name.starts_with(prefix.as_bytes()))
        .collect();
    if targets.is_empty() {
        bail!("X has not added {prefix} yet");
    }
    let data = XIChangePropertyAux::Data32(values.to_vec());
    for device in targets {
        conn.xinput_xi_change_property(
            device.deviceid,
            PropMode::REPLACE,
            property,
            AtomEnum::INTEGER.into(),
            values.len() as u32,
            &data,
        )?
        .check()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::aspect_area;

    #[test]
    fn same_aspect_uses_the_whole_tablet() {
        assert_eq!(aspect_area(15999, 8999, 2560, 1440), [0, 0, 15999, 8999]);
    }

    #[test]
    fn taller_screen_trims_the_sides() {
        // 16:10 screen: height stays, width shrinks to 14400 and is centred.
        assert_eq!(aspect_area(15999, 8999, 1920, 1200), [800, 0, 15199, 8999]);
    }

    #[test]
    fn wider_screen_trims_top_and_bottom() {
        // 21:9-ish screen: width stays, height shrinks and is centred.
        let [left, top, right, bottom] = aspect_area(15999, 8999, 3440, 1440);
        assert_eq!((left, right), (0, 15999));
        assert_eq!(bottom - top + 1, 16000 * 1440 / 3440);
        assert_eq!(top, (9000 - (bottom - top + 1)) / 2);
    }
}
