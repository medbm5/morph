//! Platform-independent ring state machine: drag detection, trigger-key taps,
//! ring switching, number/arrow keys and cancel.
//!
//! Platform backends translate OS events into [`Raw`] and act on [`Action`]s.

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Keys the machine cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Shift,
    Ctrl,
    Alt,
    Super,
    Esc,
    Enter,
    Digit(u8),
    Left,
    Right,
    Up,
    Down,
    Other,
}

impl Key {
    /// Parse a settings name: `Shift`, `Ctrl`, `Alt`, `Super`.
    pub fn parse(s: &str) -> Option<Key> {
        match s.to_ascii_lowercase().as_str() {
            "shift" => Some(Key::Shift),
            "ctrl" | "control" => Some(Key::Ctrl),
            "alt" | "option" => Some(Key::Alt),
            "super" | "win" | "meta" | "cmd" => Some(Key::Super),
            _ => None,
        }
    }
}

/// The three rings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RingKind {
    Convert,
    Tools,
    Actions,
}

/// A raw input event from a backend. Coordinates are physical screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Raw {
    ButtonDown { x: i32, y: i32 },
    ButtonUp { x: i32, y: i32 },
    Move { x: i32, y: i32 },
    KeyDown(Key),
    KeyUp(Key),
}

/// What the app should do.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Action {
    /// A drag passed the movement threshold (prefetch the selection now).
    DragStarted {
        x: i32,
        y: i32,
    },
    /// Open `ring` centred at the cursor.
    Open {
        ring: RingKind,
        x: i32,
        y: i32,
    },
    /// Switch the open ring.
    Switch {
        ring: RingKind,
    },
    /// Number key: pick segment `index` (0-based).
    Pick {
        index: u8,
    },
    Next,
    Prev,
    /// Enter: apply the hot segment.
    Apply,
    /// Esc: close the ring (the drag may continue).
    Cancel,
    /// Cursor moved while the ring is open.
    Moved {
        x: i32,
        y: i32,
    },
    /// Mouse released while the ring was open (the overlay receives the actual drop).
    Released {
        x: i32,
        y: i32,
    },
    /// Mouse released with no ring open: the drag is over.
    DragEnded,
}

/// Trigger-key mapping (configurable in Settings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Triggers {
    pub convert: Key,
    pub tools: Key,
    pub actions: Key,
}

impl Default for Triggers {
    fn default() -> Self {
        Self {
            convert: Key::Shift,
            tools: Key::Ctrl,
            actions: Key::Alt,
        }
    }
}

impl Triggers {
    pub fn ring_for(&self, key: Key) -> Option<RingKind> {
        if key == self.convert {
            Some(RingKind::Convert)
        } else if key == self.tools {
            Some(RingKind::Tools)
        } else if key == self.actions {
            Some(RingKind::Actions)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    Pressed { x: i32, y: i32 },
    Dragging,
    Open(RingKind),
}

/// Result of feeding one event.
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub actions: Vec<Action>,
    /// The backend should swallow this key (number/arrow/Enter keys while the ring is open).
    pub consume: bool,
}

pub struct Machine {
    pub triggers: Triggers,
    /// Pixels the mouse must move with the button held before it counts as a drag.
    pub drag_threshold: i32,
    /// Longest key press that still counts as a tap.
    pub tap_max: Duration,
    state: State,
    /// Trigger key currently held: (key, pressed at, no other key pressed meanwhile).
    held: Option<(Key, Instant, bool)>,
    /// Last known cursor position.
    pos: (i32, i32),
}

impl Default for Machine {
    fn default() -> Self {
        Self::new(Triggers::default())
    }
}

impl Machine {
    pub fn new(triggers: Triggers) -> Self {
        Self {
            triggers,
            drag_threshold: 6,
            tap_max: Duration::from_millis(600),
            state: State::Idle,
            held: None,
            pos: (0, 0),
        }
    }

    pub fn cursor(&self) -> (i32, i32) {
        self.pos
    }

    fn update_pos(&mut self, x: i32, y: i32) -> State {
        self.pos = (x, y);
        self.state
    }

    pub fn is_dragging(&self) -> bool {
        matches!(self.state, State::Dragging | State::Open(_))
    }

    pub fn open_ring(&self) -> Option<RingKind> {
        match self.state {
            State::Open(r) => Some(r),
            _ => None,
        }
    }

    /// Close the ring without ending the drag (e.g. the drag carried no files).
    pub fn close(&mut self) {
        if let State::Open(_) = self.state {
            self.state = State::Dragging;
        }
    }

    /// Forget everything (e.g. the drop was handled by the overlay).
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.held = None;
    }

    pub fn feed(&mut self, raw: Raw, now: Instant) -> Output {
        let mut out = Output::default();
        match raw {
            Raw::ButtonDown { x, y } => {
                self.pos = (x, y);
                self.state = State::Pressed { x, y };
                self.held = None;
            }
            Raw::Move { x, y } => match self.update_pos(x, y) {
                State::Pressed { x: ox, y: oy } => {
                    let (dx, dy) = (x - ox, y - oy);
                    if dx * dx + dy * dy >= self.drag_threshold * self.drag_threshold {
                        self.state = State::Dragging;
                        out.actions.push(Action::DragStarted { x: ox, y: oy });
                    }
                }
                State::Open(_) => out.actions.push(Action::Moved { x, y }),
                _ => {}
            },
            Raw::ButtonUp { x, y } => {
                match self.state {
                    State::Open(_) => out.actions.push(Action::Released { x, y }),
                    State::Dragging => out.actions.push(Action::DragEnded),
                    _ => {}
                }
                self.state = State::Idle;
                self.held = None;
            }
            Raw::KeyDown(key) => {
                if !self.is_dragging() {
                    return out;
                }
                if self.triggers.ring_for(key).is_some() {
                    match self.held {
                        // Auto-repeat of the held key: ignore.
                        Some((k, _, _)) if k == key => {}
                        // A second trigger while one is held: neither is a tap.
                        Some((k, t, _)) => self.held = Some((k, t, false)),
                        None => self.held = Some((key, now, true)),
                    }
                    return out;
                }
                if let Some((k, t, _)) = self.held {
                    self.held = Some((k, t, false));
                }
                if let State::Open(_) = self.state {
                    let action = match key {
                        Key::Digit(d @ 1..=9) => Some(Action::Pick { index: d - 1 }),
                        Key::Right | Key::Down => Some(Action::Next),
                        Key::Left | Key::Up => Some(Action::Prev),
                        Key::Enter => Some(Action::Apply),
                        Key::Esc => {
                            self.state = State::Dragging;
                            Some(Action::Cancel)
                        }
                        _ => None,
                    };
                    if let Some(a) = action {
                        // Esc passes through so the OS drag can be cancelled too.
                        out.consume = key != Key::Esc;
                        out.actions.push(a);
                    }
                }
            }
            Raw::KeyUp(key) => {
                let Some((held, at, clean)) = self.held else {
                    return out;
                };
                if held != key {
                    return out;
                }
                self.held = None;
                if !clean || now.duration_since(at) > self.tap_max || !self.is_dragging() {
                    return out;
                }
                let ring = self.triggers.ring_for(key).expect("held keys are triggers");
                match self.state {
                    State::Dragging => {
                        self.state = State::Open(ring);
                        let (x, y) = self.pos;
                        out.actions.push(Action::Open { ring, x, y });
                    }
                    State::Open(current) if current != ring => {
                        self.state = State::Open(ring);
                        out.actions.push(Action::Switch { ring });
                    }
                    _ => {}
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(m: &mut Machine, t: Instant) {
        m.feed(Raw::ButtonDown { x: 100, y: 100 }, t);
        let o = m.feed(Raw::Move { x: 110, y: 104 }, t);
        assert_eq!(o.actions, vec![Action::DragStarted { x: 100, y: 100 }]);
    }

    fn tap(m: &mut Machine, k: Key, t: Instant) -> Vec<Action> {
        m.feed(Raw::KeyDown(k), t);
        m.feed(Raw::KeyUp(k), t + Duration::from_millis(80)).actions
    }

    #[test]
    fn tap_opens_switches_and_drop_releases() {
        let t = Instant::now();
        let mut m = Machine::default();
        drag(&mut m, t);
        assert_eq!(
            tap(&mut m, Key::Shift, t),
            vec![Action::Open {
                ring: RingKind::Convert,
                x: 110,
                y: 104
            }]
        );
        assert_eq!(
            tap(&mut m, Key::Ctrl, t),
            vec![Action::Switch {
                ring: RingKind::Tools
            }]
        );
        assert!(tap(&mut m, Key::Ctrl, t).is_empty(), "same ring: no-op");
        assert_eq!(
            m.feed(Raw::Move { x: 300, y: 300 }, t).actions,
            vec![Action::Moved { x: 300, y: 300 }]
        );
        assert_eq!(
            m.feed(Raw::ButtonUp { x: 300, y: 300 }, t).actions,
            vec![Action::Released { x: 300, y: 300 }]
        );
        assert!(!m.is_dragging());
    }

    #[test]
    fn no_drag_no_ring() {
        let t = Instant::now();
        let mut m = Machine::default();
        assert!(tap(&mut m, Key::Shift, t).is_empty());
        m.feed(Raw::ButtonDown { x: 0, y: 0 }, t);
        m.feed(Raw::Move { x: 2, y: 2 }, t); // under threshold
        assert!(tap(&mut m, Key::Shift, t).is_empty());
    }

    #[test]
    fn held_key_or_chord_is_not_a_tap() {
        let t = Instant::now();
        let mut m = Machine::default();
        drag(&mut m, t);
        m.feed(Raw::KeyDown(Key::Shift), t);
        assert!(m
            .feed(Raw::KeyUp(Key::Shift), t + Duration::from_secs(2))
            .actions
            .is_empty());
        m.feed(Raw::KeyDown(Key::Ctrl), t);
        m.feed(Raw::KeyDown(Key::Other), t);
        assert!(m.feed(Raw::KeyUp(Key::Ctrl), t).actions.is_empty());
    }

    #[test]
    fn key_held_before_the_drag_does_not_open() {
        let t = Instant::now();
        let mut m = Machine::default();
        m.feed(Raw::KeyDown(Key::Shift), t);
        drag(&mut m, t);
        assert!(m.feed(Raw::KeyUp(Key::Shift), t).actions.is_empty());
    }

    #[test]
    fn digits_arrows_enter_consumed_esc_cancels() {
        let t = Instant::now();
        let mut m = Machine::default();
        drag(&mut m, t);
        tap(&mut m, Key::Alt, t);
        let o = m.feed(Raw::KeyDown(Key::Digit(3)), t);
        assert_eq!(o.actions, vec![Action::Pick { index: 2 }]);
        assert!(o.consume);
        assert_eq!(
            m.feed(Raw::KeyDown(Key::Right), t).actions,
            vec![Action::Next]
        );
        assert_eq!(
            m.feed(Raw::KeyDown(Key::Enter), t).actions,
            vec![Action::Apply]
        );
        let esc = m.feed(Raw::KeyDown(Key::Esc), t);
        assert_eq!(esc.actions, vec![Action::Cancel]);
        assert!(!esc.consume);
        assert_eq!(m.open_ring(), None);
        // Still dragging: another tap reopens.
        assert_eq!(
            tap(&mut m, Key::Shift, t),
            vec![Action::Open {
                ring: RingKind::Convert,
                x: 110,
                y: 104
            }]
        );
    }

    #[test]
    fn digits_without_ring_pass_through() {
        let t = Instant::now();
        let mut m = Machine::default();
        drag(&mut m, t);
        let o = m.feed(Raw::KeyDown(Key::Digit(1)), t);
        assert!(o.actions.is_empty() && !o.consume);
    }

    #[test]
    fn custom_triggers() {
        let t = Instant::now();
        let mut m = Machine::new(Triggers {
            convert: Key::Super,
            tools: Key::Shift,
            actions: Key::Alt,
        });
        drag(&mut m, t);
        assert_eq!(
            tap(&mut m, Key::Shift, t),
            vec![Action::Open {
                ring: RingKind::Tools,
                x: 110,
                y: 104
            }]
        );
        assert!(tap(&mut m, Key::Ctrl, t).is_empty());
    }
}
