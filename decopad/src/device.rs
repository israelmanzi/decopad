//! Finding the Tablet's evdev nodes and holding on to them.

use std::path::PathBuf;

use anyhow::{Context, Result};
use evdev::{AbsoluteAxisCode, Device};

pub const VENDOR: u16 = 0x28bd;
pub const PRODUCT: u16 = 0x2904;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PenGeometry {
    pub max_x: i32,
    pub max_y: i32,
    pub max_pressure: i32,
    pub units_per_mm: f32,
}

/// The Deco 640's values, used until the Tablet is attached.
impl Default for PenGeometry {
    fn default() -> Self {
        PenGeometry {
            max_x: 15999,
            max_y: 8999,
            max_pressure: 16383,
            units_per_mm: 100.0,
        }
    }
}

pub struct Tablet {
    pub pen: Device,
    pub pen_name: String,
    /// Factory Mode Express Key combos arrive here; always grabbed.
    pub keyboard: Device,
    /// Unused by the Tablet so far; grabbed so nothing stray reaches X.
    pub mouse: Option<Device>,
    pen_grabbed: bool,
}

impl Tablet {
    /// Opens the Tablet if it is plugged in and readable.
    ///
    /// `DECOPAD_TABLET` overrides the device name prefix, so tests can drive
    /// decopad with simulated devices while the real Tablet is plugged in.
    pub fn open() -> Result<Option<Tablet>> {
        let prefix = std::env::var("DECOPAD_TABLET").unwrap_or_else(|_| "XPPen Deco 640".into());
        let mut nodes: Vec<(PathBuf, Device)> = evdev::enumerate()
            .filter(|(_, d)| d.input_id().vendor() == VENDOR && d.input_id().product() == PRODUCT)
            .filter(|(_, d)| d.name().is_some_and(|n| n.starts_with(&prefix)))
            .collect();
        let mut take = |suffix: &str| {
            let position = nodes
                .iter()
                .position(|(_, d)| d.name().is_some_and(|n| n.ends_with(suffix)))?;
            Some(nodes.swap_remove(position))
        };
        let (Some((pen_path, pen)), Some((keyboard_path, mut keyboard))) =
            (take(" Pen"), take(" Keyboard"))
        else {
            return Ok(None);
        };
        let mut mouse = take(" Mouse").map(|(_, d)| d);

        keyboard.grab().with_context(|| {
            format!(
                "grabbing {} (is another decopad running?)",
                keyboard_path.display()
            )
        })?;
        if let Some(mouse) = &mut mouse {
            mouse.grab()?;
        }
        for device in [Some(&pen), Some(&keyboard), mouse.as_ref()]
            .into_iter()
            .flatten()
        {
            device.set_nonblocking(true)?;
        }
        eprintln!(
            "decopad: attached to {} (pen {}, keys {})",
            pen.name().unwrap_or("tablet"),
            pen_path.display(),
            keyboard_path.display()
        );
        Ok(Some(Tablet {
            pen_name: pen.name().unwrap_or_default().to_string(),
            pen,
            keyboard,
            mouse,
            pen_grabbed: false,
        }))
    }

    /// Whether X (false) or decopad (true) receives the Pen's events.
    pub fn grab_pen(&mut self, grab: bool) -> Result<()> {
        if grab != self.pen_grabbed {
            match grab {
                true => self.pen.grab()?,
                false => self.pen.ungrab()?,
            }
            self.pen_grabbed = grab;
        }
        Ok(())
    }

    /// The Pen's coordinate ranges, read from the device.
    pub fn geometry(&self) -> PenGeometry {
        let mut geometry = PenGeometry::default();
        let Ok(axes) = self.pen.get_absinfo() else {
            return geometry;
        };
        for (axis, info) in axes {
            match axis {
                AbsoluteAxisCode::ABS_X => {
                    geometry.max_x = info.maximum();
                    if info.resolution() > 0 {
                        geometry.units_per_mm = info.resolution() as f32;
                    }
                }
                AbsoluteAxisCode::ABS_Y => geometry.max_y = info.maximum(),
                AbsoluteAxisCode::ABS_PRESSURE => geometry.max_pressure = info.maximum(),
                _ => {}
            }
        }
        geometry
    }
}
