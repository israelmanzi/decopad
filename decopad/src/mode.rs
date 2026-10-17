use std::fmt;

use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// The Pen drives the Pointer through the X wacom driver.
    #[default]
    Desktop,
    /// decopad holds the Pen; the Pointer stays where it is.
    Pad,
}

impl Mode {
    pub fn toggled(self) -> Mode {
        match self {
            Mode::Desktop => Mode::Pad,
            Mode::Pad => Mode::Desktop,
        }
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Mode::Desktop => "Desktop Mode",
            Mode::Pad => "Pad Mode",
        })
    }
}
