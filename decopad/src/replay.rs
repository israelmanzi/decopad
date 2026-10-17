//! Replay: a virtual pen that redraws Ink in the target app, on its own thread.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{Sender, channel};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventType, InputEvent, InputId, KeyCode,
    UinputAbsSetup,
};

/// The virtual pen's X device name prefix ("decopad pen stylus").
pub const X_DEVICE_PREFIX: &str = "decopad pen";
/// Axis units per screen pixel, so strokes keep sub-pixel precision.
const SUBPIXEL: f64 = 8.0;
const MAX_PRESSURE: i32 = 16383;
/// Pause before a stroke touches down and after it lifts, so a busy app
/// finishes setting up or closing one stroke before the next events arrive.
const STROKE_EDGE: Duration = Duration::from_millis(12);
/// Pressure never drops below this share, so the X driver always registers contact.
const PRESSURE_FLOOR: f32 = 0.15;

pub struct Job {
    /// Screen-space points `(x, y, pressure)` of each stroke.
    pub strokes: Vec<Vec<(f64, f64, f32)>>,
    /// Where the pen hovers when done, leaving the Pointer there.
    pub finish_at: (i32, i32),
    pub interval: Duration,
}

pub struct Replayer {
    jobs: Sender<Job>,
    /// Becomes readable when a Job is finished.
    pub done: UnixStream,
}

impl Replayer {
    pub fn new(screen: (u32, u32)) -> Result<Self> {
        let mut pen = VirtualPen::new(screen)?;
        let (jobs, inbox) = channel::<Job>();
        let (done, mut done_writer) = UnixStream::pair()?;
        done.set_nonblocking(true)?;
        thread::Builder::new()
            .name("replay".into())
            .spawn(move || {
                for job in inbox {
                    let started = std::time::Instant::now();
                    match pen.replay(&job) {
                        Ok(()) => eprintln!(
                            "decopad: replayed {} strokes, {} points in {:.2} s",
                            job.strokes.len(),
                            job.strokes.iter().map(Vec::len).sum::<usize>(),
                            started.elapsed().as_secs_f64()
                        ),
                        Err(error) => eprintln!("decopad: replay failed: {error:#}"),
                    }
                    let _ = done_writer.write_all(&[1]);
                }
            })?;
        Ok(Replayer { jobs, done })
    }

    pub fn start(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    /// Drains completion notices; returns how many Jobs finished.
    pub fn finished(&mut self) -> usize {
        let mut buffer = [0u8; 16];
        let mut count = 0;
        while let Ok(n @ 1..) = self.done.read(&mut buffer) {
            count += n;
        }
        count
    }
}

struct VirtualPen {
    device: VirtualDevice,
}

fn abs(axis: AbsoluteAxisCode, max: i32) -> UinputAbsSetup {
    UinputAbsSetup::new(axis, AbsInfo::new(0, 0, max, 0, 0, 0))
}

impl VirtualPen {
    fn new((width, height): (u32, u32)) -> Result<Self> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for key in [
            KeyCode::BTN_TOOL_PEN,
            KeyCode::BTN_TOUCH,
            KeyCode::BTN_STYLUS,
        ] {
            keys.insert(key);
        }
        let device = VirtualDevice::builder()?
            .name("decopad pen")
            // pid.codes test IDs: never mistaken for the real Tablet.
            .input_id(InputId::new(BusType::BUS_USB, 0x1209, 0xdec0, 1))
            .with_keys(&keys)?
            .with_absolute_axis(&abs(
                AbsoluteAxisCode::ABS_X,
                (f64::from(width) * SUBPIXEL) as i32 - 1,
            ))?
            .with_absolute_axis(&abs(
                AbsoluteAxisCode::ABS_Y,
                (f64::from(height) * SUBPIXEL) as i32 - 1,
            ))?
            .with_absolute_axis(&abs(AbsoluteAxisCode::ABS_PRESSURE, MAX_PRESSURE))?
            .build()?;
        Ok(VirtualPen { device })
    }

    fn emit(
        &mut self,
        x: f64,
        y: f64,
        pressure: Option<f32>,
        touch: Option<bool>,
        in_range: Option<bool>,
    ) -> Result<()> {
        let mut events = vec![
            InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_X.0,
                (x * SUBPIXEL).round() as i32,
            ),
            InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_Y.0,
                (y * SUBPIXEL).round() as i32,
            ),
        ];
        if let Some(p) = pressure {
            let p = PRESSURE_FLOOR + (1.0 - PRESSURE_FLOOR) * p.clamp(0.0, 1.0);
            events.push(InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_PRESSURE.0,
                (p * MAX_PRESSURE as f32) as i32,
            ));
        } else if touch == Some(false) {
            events.push(InputEvent::new(
                EventType::ABSOLUTE.0,
                AbsoluteAxisCode::ABS_PRESSURE.0,
                0,
            ));
        }
        if let Some(in_range) = in_range {
            events.push(InputEvent::new(
                EventType::KEY.0,
                KeyCode::BTN_TOOL_PEN.0,
                in_range as i32,
            ));
        }
        if let Some(touch) = touch {
            events.push(InputEvent::new(
                EventType::KEY.0,
                KeyCode::BTN_TOUCH.0,
                touch as i32,
            ));
        }
        self.device.emit(&events)?;
        Ok(())
    }

    /// Sends the strokes with the least pacing that apps reproduce exactly.
    /// Measured in Xournal++: with no gap between points the toolkit merges
    /// whole strokes into one line, and without pauses around each stroke a
    /// busy app (redrawing after the Popup stepped aside, or still handling
    /// the previous Commit) clips or merges stroke starts and ends.
    fn replay(&mut self, job: &Job) -> Result<()> {
        let pace = || thread::sleep(job.interval);
        for stroke in &job.strokes {
            let Some(&(x, y, pressure)) = stroke.first() else {
                continue;
            };
            self.emit(x, y, None, None, Some(true))?;
            thread::sleep(STROKE_EDGE);
            self.emit(x, y, Some(pressure), Some(true), None)?;
            pace();
            for &(x, y, pressure) in &stroke[1..] {
                self.emit(x, y, Some(pressure), None, None)?;
                pace();
            }
            let &(x, y, _) = stroke.last().unwrap();
            self.emit(x, y, None, Some(false), None)?;
            thread::sleep(STROKE_EDGE);
        }
        let (x, y) = (f64::from(job.finish_at.0), f64::from(job.finish_at.1));
        self.emit(x, y, None, None, Some(true))?;
        // Leaving range straight after the last move gets lost, and the X
        // driver then mishandles the first stroke of the next Replay.
        thread::sleep(STROKE_EDGE);
        self.emit(x, y, None, None, Some(false))?;
        Ok(())
    }
}
