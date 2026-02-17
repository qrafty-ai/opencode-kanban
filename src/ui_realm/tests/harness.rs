//! Shared tui-realm test harness primitives.
//!
//! This module keeps UI tests independent from a real TTY by using an in-memory
//! terminal backend and a queue-driven event port for `Application::tick()`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MediaKeyCode, MouseEvent, MouseEventKind};
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::listener::{EventListenerCfg, ListenerResult, Poll};
use tuirealm::tui::backend::TestBackend;
use tuirealm::tui::buffer::Buffer;
use tuirealm::tui::widgets::Paragraph;
use tuirealm::{
    Application, AttrValue, Attribute, Component, Event, Frame, MockComponent, NoUserEvent,
    PollStrategy, Props, State, StateValue,
    event::{Key as RealmKey, KeyEvent as RealmKeyEvent, KeyModifiers as RealmKeyModifiers},
};

/// In-memory terminal that captures rendered content without opening a real TTY.
pub struct MockTerminal {
    terminal: tuirealm::tui::Terminal<TestBackend>,
}

impl MockTerminal {
    pub fn new(width: u16, height: u16) -> Self {
        let backend = TestBackend::new(width, height);
        let terminal = tuirealm::tui::Terminal::new(backend)
            .expect("mock terminal should initialize with TestBackend");
        Self { terminal }
    }

    pub fn draw<F>(&mut self, draw_fn: F)
    where
        F: FnOnce(&mut Frame<'_>),
    {
        self.terminal
            .draw(draw_fn)
            .expect("mock terminal draw should succeed");
    }

    pub fn buffer_as_string(&self) -> String {
        buffer_to_string(self.terminal.backend().buffer())
    }
}

/// Asserts that the terminal buffer contains `needle`.
pub fn assert_buffer_contains(terminal: &MockTerminal, needle: &str) {
    let rendered = terminal.buffer_as_string();
    assert!(
        rendered.contains(needle),
        "expected buffer to contain {needle:?}, got:\n{rendered}"
    );
}

/// Events accepted by the queue-driven test event source.
#[derive(Debug, Clone, PartialEq)]
pub enum InjectedEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Tick,
}

/// Queue-backed `Poll` implementation that can be fed by tests.
#[derive(Clone, Default)]
pub struct EventDriver {
    queue: Arc<Mutex<VecDeque<Event<NoUserEvent>>>>,
    history: Arc<Mutex<Vec<InjectedEvent>>>,
}

impl EventDriver {
    /// Builds an application listener config backed by this driver.
    pub fn listener_cfg(&self) -> EventListenerCfg<NoUserEvent> {
        EventListenerCfg::default()
            .poll_timeout(Duration::from_millis(5))
            .port(Box::new(self.clone()), Duration::from_millis(1))
    }

    pub fn send_key_event(&self, key: KeyEvent) {
        self.record(InjectedEvent::Key(key));
        self.push(Event::Keyboard(RealmKeyEvent::new(
            map_key_code(key.code),
            map_key_modifiers(key.modifiers),
        )));
    }

    pub fn send_mouse_event(&self, mouse: MouseEvent) {
        self.record(InjectedEvent::Mouse(mouse));
        // tui-realm 1.9 maps crossterm mouse input to Event::None.
        self.push(Event::None);
    }

    pub fn send_tick(&self) {
        self.record(InjectedEvent::Tick);
        self.push(Event::Tick);
    }

    pub fn history(&self) -> Vec<InjectedEvent> {
        self.history
            .lock()
            .expect("event history lock should not be poisoned")
            .clone()
    }

    fn push(&self, event: Event<NoUserEvent>) {
        self.queue
            .lock()
            .expect("event queue lock should not be poisoned")
            .push_back(event);
    }

    fn record(&self, event: InjectedEvent) {
        self.history
            .lock()
            .expect("event history lock should not be poisoned")
            .push(event);
    }
}

impl Poll<NoUserEvent> for EventDriver {
    fn poll(&mut self) -> ListenerResult<Option<Event<NoUserEvent>>> {
        Ok(self
            .queue
            .lock()
            .expect("event queue lock should not be poisoned")
            .pop_front())
    }
}

/// Convenience helper for injecting plain key presses with no modifiers.
pub fn send_keys(driver: &EventDriver, keys: &[KeyCode]) {
    for key in keys {
        driver.send_key_event(KeyEvent::new(*key, KeyModifiers::NONE));
    }
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            output.push_str(buffer.get(x, y).symbol());
        }
        if y + 1 < buffer.area.height {
            output.push('\n');
        }
    }
    output
}

fn map_key_code(code: KeyCode) -> RealmKey {
    match code {
        KeyCode::Backspace => RealmKey::Backspace,
        KeyCode::Enter => RealmKey::Enter,
        KeyCode::Left => RealmKey::Left,
        KeyCode::Right => RealmKey::Right,
        KeyCode::Up => RealmKey::Up,
        KeyCode::Down => RealmKey::Down,
        KeyCode::Home => RealmKey::Home,
        KeyCode::End => RealmKey::End,
        KeyCode::PageUp => RealmKey::PageUp,
        KeyCode::PageDown => RealmKey::PageDown,
        KeyCode::Tab => RealmKey::Tab,
        KeyCode::BackTab => RealmKey::BackTab,
        KeyCode::Delete => RealmKey::Delete,
        KeyCode::Insert => RealmKey::Insert,
        KeyCode::F(index) => RealmKey::Function(index),
        KeyCode::Char(ch) => RealmKey::Char(ch),
        KeyCode::Null => RealmKey::Null,
        KeyCode::Esc => RealmKey::Esc,
        KeyCode::CapsLock => RealmKey::CapsLock,
        KeyCode::ScrollLock => RealmKey::ScrollLock,
        KeyCode::NumLock => RealmKey::NumLock,
        KeyCode::PrintScreen => RealmKey::PrintScreen,
        KeyCode::Pause => RealmKey::Pause,
        KeyCode::Menu => RealmKey::Menu,
        KeyCode::KeypadBegin => RealmKey::KeypadBegin,
        KeyCode::Media(media) => RealmKey::Media(map_media_key_code(media)),
        KeyCode::Modifier(_) => RealmKey::Null,
    }
}

fn map_media_key_code(media: MediaKeyCode) -> tuirealm::event::MediaKeyCode {
    match media {
        MediaKeyCode::Play => tuirealm::event::MediaKeyCode::Play,
        MediaKeyCode::Pause => tuirealm::event::MediaKeyCode::Pause,
        MediaKeyCode::PlayPause => tuirealm::event::MediaKeyCode::PlayPause,
        MediaKeyCode::Reverse => tuirealm::event::MediaKeyCode::Reverse,
        MediaKeyCode::Stop => tuirealm::event::MediaKeyCode::Stop,
        MediaKeyCode::FastForward => tuirealm::event::MediaKeyCode::FastForward,
        MediaKeyCode::Rewind => tuirealm::event::MediaKeyCode::Rewind,
        MediaKeyCode::TrackNext => tuirealm::event::MediaKeyCode::TrackNext,
        MediaKeyCode::TrackPrevious => tuirealm::event::MediaKeyCode::TrackPrevious,
        MediaKeyCode::Record => tuirealm::event::MediaKeyCode::Record,
        MediaKeyCode::LowerVolume => tuirealm::event::MediaKeyCode::LowerVolume,
        MediaKeyCode::RaiseVolume => tuirealm::event::MediaKeyCode::RaiseVolume,
        MediaKeyCode::MuteVolume => tuirealm::event::MediaKeyCode::MuteVolume,
    }
}

fn map_key_modifiers(modifiers: KeyModifiers) -> RealmKeyModifiers {
    let mut mapped = RealmKeyModifiers::NONE;
    if modifiers.contains(KeyModifiers::SHIFT) {
        mapped.insert(RealmKeyModifiers::SHIFT);
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        mapped.insert(RealmKeyModifiers::CONTROL);
    }
    if modifiers.contains(KeyModifiers::ALT) {
        mapped.insert(RealmKeyModifiers::ALT);
    }
    mapped
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum HarnessComponentId {
    Example,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HarnessMsg {
    Submitted(u16),
    Ticked,
}

pub struct ExampleComponent {
    props: Props,
    label: String,
    submits: u16,
}

impl Default for ExampleComponent {
    fn default() -> Self {
        Self {
            props: Props::default(),
            label: "harness-ready".to_string(),
            submits: 0,
        }
    }
}

impl MockComponent for ExampleComponent {
    fn view(&mut self, frame: &mut Frame, area: tuirealm::tui::layout::Rect) {
        let text = format!("{} | submits: {}", self.label, self.submits);
        frame.render_widget(Paragraph::new(text), area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.submits))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<HarnessMsg, NoUserEvent> for ExampleComponent {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<HarnessMsg> {
        match ev {
            Event::Keyboard(RealmKeyEvent {
                code: RealmKey::Enter,
                ..
            }) => {
                self.submits += 1;
                Some(HarnessMsg::Submitted(self.submits))
            }
            Event::Tick => Some(HarnessMsg::Ticked),
            _ => None,
        }
    }
}

#[test]
fn example_test() {
    let driver = EventDriver::default();
    let mut app: Application<HarnessComponentId, HarnessMsg, NoUserEvent> =
        Application::init(driver.listener_cfg());

    let mut component = ExampleComponent::default();
    component.label = "harness-ready".to_string();
    app.mount(HarnessComponentId::Example, Box::new(component), vec![])
        .expect("component should mount");
    app.active(&HarnessComponentId::Example)
        .expect("component should receive focus");

    let mut terminal = MockTerminal::new(40, 3);
    terminal.draw(|frame| {
        app.view(&HarnessComponentId::Example, frame, frame.size());
    });
    assert_buffer_contains(&terminal, "harness-ready");

    send_keys(&driver, &[KeyCode::Enter]);
    driver.send_mouse_event(MouseEvent {
        kind: MouseEventKind::Moved,
        column: 2,
        row: 1,
        modifiers: KeyModifiers::NONE,
    });
    driver.send_tick();

    let messages = app
        .tick(PollStrategy::UpTo(8))
        .expect("event tick should succeed");
    assert!(messages.contains(&HarnessMsg::Submitted(1)));
    assert!(messages.contains(&HarnessMsg::Ticked));

    terminal.draw(|frame| {
        app.view(&HarnessComponentId::Example, frame, frame.size());
    });
    assert_buffer_contains(&terminal, "submits: 1");

    let history = driver.history();
    assert!(
        history
            .iter()
            .any(|event| matches!(event, InjectedEvent::Mouse(_))),
        "mouse events should be recorded in driver history"
    );
}
