//! Linux: offer files on the clipboard so Nautilus, Dolphin, Nemo and Thunar paste
//! real copies. X11: we own CLIPBOARD from a background thread and serve
//! `x-special/gnome-copied-files`, `text/uri-list` and text. Wayland: `wl-copy`.

use anyhow::{bail, Context, Result};
use std::path::PathBuf;

fn uris(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| crate::linux::file_uri(&std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
        .collect()
}

pub fn copy_files(paths: &[PathBuf]) -> Result<()> {
    let uris = uris(paths);
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && morph_core::components::find_on_path("wl-copy").is_some()
    {
        use std::io::Write;
        let mut child = std::process::Command::new("wl-copy")
            .args(["--type", "text/uri-list"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .context("running wl-copy")?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all((uris.join("\r\n") + "\r\n").as_bytes())?;
        return Ok(());
    }
    serve_x11(uris)
}

/// Own the CLIPBOARD selection and answer conversion requests until someone else
/// takes ownership.
fn serve_x11(uris: Vec<String>) -> Result<()> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;

    let (conn, screen) = x11rb::connect(None).context("connecting to X")?;
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id()?;
    conn.create_window(
        0,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )?;
    let atom = |n: &[u8]| -> Result<Atom> { Ok(conn.intern_atom(false, n)?.reply()?.atom) };
    let clipboard = atom(b"CLIPBOARD")?;
    let targets = atom(b"TARGETS")?;
    let gnome = atom(b"x-special/gnome-copied-files")?;
    let urilist = atom(b"text/uri-list")?;
    let utf8 = atom(b"UTF8_STRING")?;
    let kde_cut = atom(b"application/x-kde-cutselection")?;
    conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME)?;
    conn.flush()?;
    if conn.get_selection_owner(clipboard)?.reply()?.owner != win {
        bail!("couldn't take the clipboard");
    }
    let gnome_data = format!("copy\n{}", uris.join("\n"));
    let uri_data = uris.join("\r\n") + "\r\n";
    std::thread::spawn(move || {
        while let Ok(ev) = conn.wait_for_event() {
            match ev {
                Event::SelectionRequest(req) => {
                    let mut prop = req.property;
                    if req.target == targets {
                        let list = [targets, gnome, urilist, utf8, kde_cut];
                        let _ = conn.change_property32(
                            PropMode::REPLACE,
                            req.requestor,
                            prop,
                            AtomEnum::ATOM,
                            &list,
                        );
                    } else if req.target == gnome {
                        let _ = conn.change_property8(
                            PropMode::REPLACE,
                            req.requestor,
                            prop,
                            gnome,
                            gnome_data.as_bytes(),
                        );
                    } else if req.target == urilist {
                        let _ = conn.change_property8(
                            PropMode::REPLACE,
                            req.requestor,
                            prop,
                            urilist,
                            uri_data.as_bytes(),
                        );
                    } else if req.target == utf8 {
                        let _ = conn.change_property8(
                            PropMode::REPLACE,
                            req.requestor,
                            prop,
                            utf8,
                            uri_data.as_bytes(),
                        );
                    } else if req.target == kde_cut {
                        let _ = conn.change_property8(
                            PropMode::REPLACE,
                            req.requestor,
                            prop,
                            kde_cut,
                            b"0",
                        );
                    } else {
                        prop = x11rb::NONE;
                    }
                    let notify = SelectionNotifyEvent {
                        response_type: SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: req.time,
                        requestor: req.requestor,
                        selection: req.selection,
                        target: req.target,
                        property: prop,
                    };
                    let _ = conn.send_event(false, req.requestor, EventMask::NO_EVENT, notify);
                    let _ = conn.flush();
                }
                Event::SelectionClear(_) => break, // someone else copied
                _ => {}
            }
        }
    });
    Ok(())
}
