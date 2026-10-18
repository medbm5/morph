//! Windows backend: low-level mouse and keyboard hooks on a dedicated thread.

use crate::machine::{Action, Key, Machine, Raw};
use crate::Sink;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Marks keystrokes we inject ourselves so the hook ignores them.
const OUR_MARK: usize = 0x4D4F_5250; // "MORP"
/// Unassigned virtual key used to stop a lone Alt tap from activating menus.
const VK_DUMMY: u16 = 0xE8;

struct Shared {
    machine: Arc<Mutex<Machine>>,
    sink: Sink,
}

// Hook procedures are free functions, so the state lives in a global. One watcher per process.
static SHARED: OnceLock<Mutex<Option<Shared>>> = OnceLock::new();

fn shared() -> &'static Mutex<Option<Shared>> {
    SHARED.get_or_init(|| Mutex::new(None))
}

pub fn start(machine: Arc<Mutex<Machine>>, sink: Sink) -> anyhow::Result<Box<dyn FnOnce() + Send>> {
    *shared().lock().unwrap() = Some(Shared { machine, sink });
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("morph-input-hooks".into())
        .spawn(move || unsafe {
            let module = GetModuleHandleW(None).ok();
            let hinst = module.map(|m| windows::Win32::Foundation::HINSTANCE(m.0));
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0);
            let ms = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), hinst, 0);
            match (&kb, &ms) {
                (Ok(_), Ok(_)) => {
                    let _ = tx.send(Ok(GetCurrentThreadId()));
                }
                _ => {
                    let _ = tx.send(Err(anyhow::anyhow!(
                        "SetWindowsHookEx failed: {:?} {:?}",
                        kb.as_ref().err(),
                        ms.as_ref().err()
                    )));
                    return;
                }
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if let Ok(h) = kb {
                let _ = UnhookWindowsHookEx(h);
            }
            if let Ok(h) = ms {
                let _ = UnhookWindowsHookEx(h);
            }
        })?;
    let tid = rx.recv()??;
    Ok(Box::new(move || unsafe {
        let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        *shared().lock().unwrap() = None;
    }))
}

fn map_vk(vk: u32) -> Key {
    match vk {
        0x10 | 0xA0 | 0xA1 => Key::Shift,
        0x11 | 0xA2 | 0xA3 => Key::Ctrl,
        0x12 | 0xA4 | 0xA5 => Key::Alt,
        0x5B | 0x5C => Key::Super,
        0x1B => Key::Esc,
        0x0D => Key::Enter,
        0x31..=0x39 => Key::Digit((vk - 0x30) as u8),
        0x61..=0x69 => Key::Digit((vk - 0x60) as u8),
        0x25 => Key::Left,
        0x26 => Key::Up,
        0x27 => Key::Right,
        0x28 => Key::Down,
        _ => Key::Other,
    }
}

/// Feed one event; returns whether to swallow it.
fn feed(raw: Raw) -> bool {
    let guard = shared().lock().unwrap();
    let Some(s) = guard.as_ref() else {
        return false;
    };
    let (out, dragging) = {
        let mut m = s.machine.lock().unwrap();
        let out = m.feed(raw, Instant::now());
        // A window being moved or resized is not a file drag: never open the ring.
        if out.actions.iter().any(|a| matches!(a, Action::Open { .. })) && in_move_size() {
            m.close();
            return false;
        }
        (out, m.is_dragging())
    };
    if dragging && raw == Raw::KeyDown(Key::Alt) {
        suppress_alt_menu();
    }
    for a in out.actions {
        (s.sink)(a);
    }
    out.consume
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        if kb.dwExtraInfo != OUR_MARK {
            let key = map_vk(kb.vkCode);
            let raw = match wparam.0 as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => Some(Raw::KeyDown(key)),
                WM_KEYUP | WM_SYSKEYUP => Some(Raw::KeyUp(key)),
                _ => None,
            };
            if let Some(raw) = raw {
                if feed(raw) {
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let ms = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let (x, y) = (ms.pt.x, ms.pt.y);
        let raw = match wparam.0 as u32 {
            WM_LBUTTONDOWN => Some(Raw::ButtonDown { x, y }),
            WM_LBUTTONUP => Some(Raw::ButtonUp { x, y }),
            WM_MOUSEMOVE => Some(Raw::Move { x, y }),
            _ => None,
        };
        if let Some(raw) = raw {
            feed(raw);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Is any thread in a window move/resize loop (title-bar drag)?
fn in_move_size() -> bool {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let hwnd = WindowFromPoint(pt);
        let fg = GetForegroundWindow();
        [hwnd, fg]
            .into_iter()
            .filter(|h| !h.is_invalid())
            .any(|h: HWND| {
                let tid = GetWindowThreadProcessId(h, None);
                let mut info = GUITHREADINFO {
                    cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                    ..Default::default()
                };
                GetGUIThreadInfo(tid, &mut info).is_ok() && info.flags.contains(GUI_INMOVESIZE)
            })
    }
}

/// A lone Alt press/release activates the focused window's menu bar. Inject an
/// unassigned key in between so the release isn't seen as a lone Alt tap.
fn suppress_alt_menu() {
    let key = |flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(VK_DUMMY),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: OUR_MARK,
            },
        },
    };
    let inputs = [key(Default::default()), key(KEYEVENTF_KEYUP)];
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}
