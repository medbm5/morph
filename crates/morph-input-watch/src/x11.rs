//! X11 backend: XInput2 raw events on the root window (no grabs).
//!
//! Raw events reach us whatever window has focus, without grabbing the keyboard
//! or pointer. A drag only counts as a file drag while some client owns the
//! `XdndSelection` (the XDND protocol's drag data selection).

use crate::machine::{Action, Key, Machine, Raw};
use crate::Sink;
use anyhow::{Context, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use x11rb::connection::Connection;
use x11rb::protocol::xinput::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{Atom, ConnectionExt as _, Window};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

pub fn start(machine: Arc<Mutex<Machine>>, sink: Sink) -> Result<Box<dyn FnOnce() + Send>> {
    let (conn, screen_num) = x11rb::connect(None).context("connecting to the X server")?;
    let root = conn.setup().roots[screen_num].root;
    conn.xinput_xi_query_version(2, 0)?
        .reply()
        .context("XInput 2 is not available")?;

    let mask = xinput::XIEventMask::RAW_KEY_PRESS
        | xinput::XIEventMask::RAW_KEY_RELEASE
        | xinput::XIEventMask::RAW_BUTTON_PRESS
        | xinput::XIEventMask::RAW_BUTTON_RELEASE
        | xinput::XIEventMask::RAW_MOTION;
    conn.xinput_xi_select_events(
        root,
        &[xinput::EventMask {
            deviceid: xinput::Device::ALL_MASTER.into(),
            mask: vec![mask.into()],
        }],
    )?
    .check()
    .context("selecting XInput2 raw events")?;

    let xdnd = conn.intern_atom(false, b"XdndSelection")?.reply()?.atom;
    let keymap = Keymap::load(&conn)?;
    conn.flush()?;

    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    std::thread::Builder::new()
        .name("morph-input-x11".into())
        .spawn(move || {
            if let Err(e) = run(&conn, root, xdnd, &keymap, &machine, &sink, &stop2) {
                log::error!("X11 input watcher stopped: {e:#}");
            }
        })?;
    Ok(Box::new(move || stop.store(true, Ordering::Relaxed)))
}

fn run(
    conn: &RustConnection,
    root: Window,
    xdnd: Atom,
    keymap: &Keymap,
    machine: &Mutex<Machine>,
    sink: &Sink,
    stop: &AtomicBool,
) -> Result<()> {
    while !stop.load(Ordering::Relaxed) {
        let event = conn.wait_for_event()?;
        let raw = match event {
            Event::XinputRawKeyPress(e) => Raw::KeyDown(keymap.key(e.detail as u8)),
            Event::XinputRawKeyRelease(e) => Raw::KeyUp(keymap.key(e.detail as u8)),
            Event::XinputRawButtonPress(e) if e.detail == 1 => {
                let (x, y) = pointer(conn, root)?;
                Raw::ButtonDown { x, y }
            }
            Event::XinputRawButtonRelease(e) if e.detail == 1 => {
                let (x, y) = pointer(conn, root)?;
                Raw::ButtonUp { x, y }
            }
            Event::XinputRawMotion(_) => {
                // Only track motion while the button is held or the ring is open.
                if !machine.lock().unwrap().is_dragging() && !button_held(conn, root)? {
                    continue;
                }
                let (x, y) = pointer(conn, root)?;
                Raw::Move { x, y }
            }
            _ => continue,
        };
        let out = {
            let mut m = machine.lock().unwrap();
            let out = m.feed(raw, Instant::now());
            if out.actions.iter().any(|a| matches!(a, Action::Open { .. }))
                && !xdnd_active(conn, xdnd)?
            {
                // Not a drag-and-drop (window move, text selection, rubber band): no ring.
                m.close();
                continue;
            }
            out
        };
        for a in out.actions {
            sink(a);
        }
    }
    Ok(())
}

fn pointer(conn: &RustConnection, root: Window) -> Result<(i32, i32)> {
    let r = conn.query_pointer(root)?.reply()?;
    Ok((r.root_x as i32, r.root_y as i32))
}

fn button_held(conn: &RustConnection, root: Window) -> Result<bool> {
    let r = conn.query_pointer(root)?.reply()?;
    Ok(u16::from(r.mask) & u16::from(x11rb::protocol::xproto::KeyButMask::BUTTON1) != 0)
}

/// Is an XDND drag in progress? The drag source owns `XdndSelection` while dragging.
pub fn xdnd_active(conn: &RustConnection, xdnd: Atom) -> Result<bool> {
    Ok(conn.get_selection_owner(xdnd)?.reply()?.owner != x11rb::NONE)
}

/// Keycode → our keys, via the server's keysym table.
struct Keymap {
    min: u8,
    per: usize,
    syms: Vec<u32>,
}

impl Keymap {
    fn load(conn: &RustConnection) -> Result<Self> {
        let setup = conn.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let r = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        Ok(Self {
            min,
            per: r.keysyms_per_keycode as usize,
            syms: r.keysyms,
        })
    }

    fn key(&self, code: u8) -> Key {
        let i = (code.saturating_sub(self.min)) as usize * self.per;
        let sym = self.syms.get(i).copied().unwrap_or(0);
        match sym {
            0xffe1 | 0xffe2 => Key::Shift,
            0xffe3 | 0xffe4 => Key::Ctrl,
            0xffe9 | 0xffea | 0xfe03 => Key::Alt,
            0xffeb | 0xffec => Key::Super,
            0xff1b => Key::Esc,
            0xff0d | 0xff8d => Key::Enter,
            0x31..=0x39 => Key::Digit((sym - 0x30) as u8),
            0xffb1..=0xffb9 => Key::Digit((sym - 0xffb0) as u8),
            0xff51 => Key::Left,
            0xff52 => Key::Up,
            0xff53 => Key::Right,
            0xff54 => Key::Down,
            _ => Key::Other,
        }
    }
}
