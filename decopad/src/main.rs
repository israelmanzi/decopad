//! decopad: turns the XP-Pen Deco 640 into a keyboard companion.
//! See docs/CONTEXT.md for the vocabulary used here.

mod actions;
mod config;
mod daemon;
mod device;
mod ink;
mod keys;
mod mode;
mod pen;
mod popup;
mod preview;
mod render;
mod replay;
mod xconn;
mod xmap;

use std::fs::File;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use inotify::{Inotify, WatchMask};
use nix::fcntl::{Flock, FlockArg};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use daemon::Daemon;
use xconn::XConn;

/// Holds an exclusive lock for as long as this decopad runs.
fn single_instance() -> Result<Flock<File>> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let file = File::create(dir.join("decopad.lock"))?;
    Flock::lock(file, FlockArg::LockExclusiveNonblock)
        .map_err(|_| anyhow::anyhow!("decopad is already running"))
}

fn watch(inotify: &mut Inotify, config_dir: &Path) -> Result<()> {
    inotify
        .watches()
        .add(
            config_dir,
            WatchMask::CLOSE_WRITE | WatchMask::MOVED_TO | WatchMask::CREATE,
        )
        .with_context(|| format!("watching {}", config_dir.display()))?;
    // New nodes appear on CREATE; the seat ACL that makes them readable lands on ATTRIB.
    inotify
        .watches()
        .add("/dev/input", WatchMask::CREATE | WatchMask::ATTRIB)
        .context("watching /dev/input")?;
    Ok(())
}

/// Milliseconds until `deadline`, rounded up so it has passed on wake-up.
fn timeout_until(deadline: Option<Instant>) -> PollTimeout {
    match deadline {
        None => PollTimeout::NONE,
        Some(deadline) => {
            let ms = deadline
                .saturating_duration_since(Instant::now())
                .as_micros()
                .div_ceil(1000);
            PollTimeout::try_from(ms.min(60_000) as i32).unwrap_or(PollTimeout::NONE)
        }
    }
}

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--default-config") {
        print!("{}", config::DEFAULT);
        return Ok(());
    }
    let _lock = single_instance()?;
    let config_dir = config::dir()?;
    let config_path = config_dir.join(config::FILE_NAME);
    let config = config::load_or_create(&config_path)?;
    eprintln!("decopad: config {}", config_path.display());

    let mut inotify = Inotify::init()?;
    watch(&mut inotify, &config_dir)?;

    let mut daemon = Daemon::new(config_path, config, XConn::connect()?)?;
    if daemon.tablet.is_none() {
        eprintln!("decopad: waiting for the tablet");
    }

    let mut buffer = [0u8; 4096];
    loop {
        for event in daemon.x.events()? {
            daemon.on_x_event(event)?;
        }
        daemon.sync_key_grab()?;
        daemon.present()?;
        // Requests that wait for a reply can pull events off the socket into
        // x11rb's queue, where poll() cannot see them; handle those first.
        let late = daemon.x.events()?;
        if !late.is_empty() {
            for event in late {
                daemon.on_x_event(event)?;
            }
            continue;
        }

        let (inotify_ready, replay_ready, tablet_ready) = {
            let mut fds = vec![
                PollFd::new(inotify.as_fd(), PollFlags::POLLIN),
                PollFd::new(daemon.x.conn.stream().as_fd(), PollFlags::POLLIN),
                PollFd::new(daemon.replayer.done.as_fd(), PollFlags::POLLIN),
            ];
            if let Some(tablet) = &daemon.tablet {
                fds.push(PollFd::new(tablet.pen.as_fd(), PollFlags::POLLIN));
                fds.push(PollFd::new(tablet.keyboard.as_fd(), PollFlags::POLLIN));
                if let Some(mouse) = &tablet.mouse {
                    fds.push(PollFd::new(mouse.as_fd(), PollFlags::POLLIN));
                }
            }
            match poll(&mut fds, timeout_until(daemon.next_deadline())) {
                Err(nix::errno::Errno::EINTR) => continue,
                result => result?,
            };
            let ready = |fd: &PollFd| fd.revents().is_some_and(|r| !r.is_empty());
            (ready(&fds[0]), ready(&fds[2]), fds[3..].iter().any(ready))
        };

        if inotify_ready {
            let mut config_changed = false;
            let mut devices_changed = false;
            for event in inotify.read_events(&mut buffer)? {
                match event.name.and_then(|n| n.to_str()) {
                    Some(config::FILE_NAME) => config_changed = true,
                    Some(name) if name.starts_with("event") => devices_changed = true,
                    _ => {}
                }
            }
            if config_changed {
                daemon.reload_config();
            }
            if devices_changed && daemon.tablet.is_none() {
                daemon.attach();
            }
        }
        if replay_ready {
            daemon.on_replay_done()?;
        }
        if tablet_ready && !daemon.read_tablet()? {
            daemon.detach();
        }
        daemon.run_due()?;
    }
}
