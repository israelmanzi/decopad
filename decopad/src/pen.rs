//! Folds the Pen's evdev events into one state per hardware report.
//!
//! The Deco 640's pen has no eraser end. Its upper side button instead
//! reports the eraser tool coming into range (BTN_TOOL_RUBBER), so that is
//! read as Pen Button 2 rather than as erasing (docs/hardware-deco-640.md).

use evdev::{AbsoluteAxisCode, EventSummary, InputEvent, KeyCode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PenState {
    pub x: i32,
    pub y: i32,
    pub pressure: i32,
    pub touching: bool,
    pub in_range: bool,
    pub button1: bool,
    pub button2: bool,
}

#[derive(Default)]
pub struct PenTracker {
    state: PenState,
    reported: PenState,
    pen_tool: bool,
    rubber_tool: bool,
    stylus2: bool,
}

impl PenTracker {
    /// Returns `(previous, current)` at the end of each report that changed something.
    pub fn feed(&mut self, event: InputEvent) -> Option<(PenState, PenState)> {
        let s = &mut self.state;
        match event.destructure() {
            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_X, v) => s.x = v,
            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_Y, v) => s.y = v,
            EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_PRESSURE, v) => s.pressure = v,
            EventSummary::Key(_, KeyCode::BTN_TOUCH, v) => s.touching = v != 0,
            EventSummary::Key(_, KeyCode::BTN_TOOL_PEN, v) => self.pen_tool = v != 0,
            EventSummary::Key(_, KeyCode::BTN_TOOL_RUBBER, v) => self.rubber_tool = v != 0,
            EventSummary::Key(_, KeyCode::BTN_STYLUS, v) => s.button1 = v != 0,
            EventSummary::Key(_, KeyCode::BTN_STYLUS2, v) => self.stylus2 = v != 0,
            EventSummary::Synchronization(..) => {
                s.in_range = self.pen_tool || self.rubber_tool;
                s.button2 = self.stylus2 || self.rubber_tool;
                if self.state != self.reported {
                    let previous = std::mem::replace(&mut self.reported, self.state);
                    return Some((previous, self.state));
                }
            }
            _ => {}
        }
        None
    }

    pub fn reset(&mut self) {
        *self = PenTracker::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evdev::{EventType, SynchronizationCode};

    fn key(code: KeyCode, value: i32) -> InputEvent {
        InputEvent::new(EventType::KEY.0, code.0, value)
    }

    fn syn() -> InputEvent {
        InputEvent::new(
            EventType::SYNCHRONIZATION.0,
            SynchronizationCode::SYN_REPORT.0,
            0,
        )
    }

    fn report(tracker: &mut PenTracker, events: &[InputEvent]) -> Option<PenState> {
        events
            .iter()
            .for_each(|&e| assert!(tracker.feed(e).is_none()));
        tracker.feed(syn()).map(|(_, now)| now)
    }

    #[test]
    fn upper_button_reported_as_eraser_tool_is_button_2() {
        // Captured sequence: the pen tool leaves, the eraser tool arrives while
        // the button is held, then the pen tool returns.
        let mut tracker = PenTracker::default();
        let now = report(&mut tracker, &[key(KeyCode::BTN_TOOL_PEN, 1)]).unwrap();
        assert!(now.in_range && !now.button2);
        let now = report(
            &mut tracker,
            &[
                key(KeyCode::BTN_TOOL_PEN, 0),
                key(KeyCode::BTN_TOOL_RUBBER, 1),
            ],
        )
        .unwrap();
        assert!(now.in_range && now.button2);
        let now = report(
            &mut tracker,
            &[
                key(KeyCode::BTN_TOOL_RUBBER, 0),
                key(KeyCode::BTN_TOOL_PEN, 1),
            ],
        )
        .unwrap();
        assert!(now.in_range && !now.button2);
    }

    #[test]
    fn lower_button_is_button_1() {
        let mut tracker = PenTracker::default();
        let now = report(
            &mut tracker,
            &[key(KeyCode::BTN_TOOL_PEN, 1), key(KeyCode::BTN_STYLUS, 1)],
        )
        .unwrap();
        assert!(now.in_range && now.button1);
        assert!(report(&mut tracker, &[]).is_none(), "nothing changed");
    }
}
