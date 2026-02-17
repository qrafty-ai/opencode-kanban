use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use crossterm::event::{
    KeyCode as CrosstermKeyCode, KeyEvent as CrosstermKeyEvent,
    KeyModifiers as CrosstermKeyModifiers, MouseButton as CrosstermMouseButton,
    MouseEvent as CrosstermMouseEvent, MouseEventKind as CrosstermMouseEventKind,
};
use tuirealm::{
    Application, AttrValue, Attribute, Component, Event, EventListenerCfg, Frame, MockComponent,
    NoUserEvent, Props, State,
    command::{Cmd, CmdResult},
    event::{
        Key as RealmKey, KeyEvent as RealmKeyEvent, KeyModifiers as RealmKeyModifiers,
        MouseButton as RealmMouseButton, MouseEvent as RealmMouseEvent,
        MouseEventKind as RealmMouseEventKind,
    },
    ratatui::layout::Rect,
};

use crate::{
    app::{App, Message},
    ui,
};

pub type SharedApp = Arc<Mutex<App>>;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RootId {
    Root,
}

pub fn init_application(app: SharedApp) -> Result<Application<RootId, Message, NoUserEvent>> {
    let mut application: Application<RootId, Message, NoUserEvent> = Application::init(
        EventListenerCfg::default()
            .crossterm_input_listener(Duration::from_millis(20), 3)
            .poll_timeout(Duration::from_millis(10))
            .tick_interval(Duration::from_millis(500)),
    );

    application
        .mount(RootId::Root, Box::new(RootComponent::new(app)), Vec::new())
        .context("failed to mount tui-realm root component")?;

    application
        .active(&RootId::Root)
        .context("failed to activate tui-realm root component")?;

    Ok(application)
}

pub fn apply_message(shared_app: &SharedApp, message: Message) -> Result<()> {
    let mut app = lock_app(shared_app)?;
    app.update(message)
}

pub fn should_quit(shared_app: &SharedApp) -> Result<bool> {
    let app = lock_app(shared_app)?;
    Ok(app.should_quit())
}

fn lock_app(shared_app: &SharedApp) -> Result<MutexGuard<'_, App>> {
    shared_app
        .lock()
        .map_err(|_| anyhow!("failed to lock app state"))
}

struct RootComponent {
    props: Props,
    app: SharedApp,
}

impl RootComponent {
    fn new(app: SharedApp) -> Self {
        Self {
            props: Props::default(),
            app,
        }
    }
}

impl MockComponent for RootComponent {
    fn view(&mut self, frame: &mut Frame, _area: Rect) {
        if let Ok(mut app) = self.app.lock() {
            ui::render(frame, &mut app);
        }
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Message, NoUserEvent> for RootComponent {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Message> {
        match ev {
            Event::Keyboard(key) => Some(Message::Key(convert_key_event(key))),
            Event::Mouse(mouse) => Some(Message::Mouse(convert_mouse_event(mouse))),
            Event::WindowResize(width, height) => Some(Message::Resize(width, height)),
            Event::Tick => Some(Message::Tick),
            _ => None,
        }
    }
}

fn convert_key_event(key: RealmKeyEvent) -> CrosstermKeyEvent {
    CrosstermKeyEvent::new(
        convert_key_code(key.code),
        convert_key_modifiers(key.modifiers),
    )
}

fn convert_key_code(key: RealmKey) -> CrosstermKeyCode {
    match key {
        RealmKey::Backspace => CrosstermKeyCode::Backspace,
        RealmKey::Enter => CrosstermKeyCode::Enter,
        RealmKey::Left => CrosstermKeyCode::Left,
        RealmKey::Right => CrosstermKeyCode::Right,
        RealmKey::Up => CrosstermKeyCode::Up,
        RealmKey::Down => CrosstermKeyCode::Down,
        RealmKey::Home => CrosstermKeyCode::Home,
        RealmKey::End => CrosstermKeyCode::End,
        RealmKey::PageUp => CrosstermKeyCode::PageUp,
        RealmKey::PageDown => CrosstermKeyCode::PageDown,
        RealmKey::Tab => CrosstermKeyCode::Tab,
        RealmKey::BackTab => CrosstermKeyCode::BackTab,
        RealmKey::Delete => CrosstermKeyCode::Delete,
        RealmKey::Insert => CrosstermKeyCode::Insert,
        RealmKey::Function(index) => CrosstermKeyCode::F(index),
        RealmKey::Char(ch) => CrosstermKeyCode::Char(ch),
        RealmKey::Esc => CrosstermKeyCode::Esc,
        _ => CrosstermKeyCode::Null,
    }
}

fn convert_key_modifiers(modifiers: RealmKeyModifiers) -> CrosstermKeyModifiers {
    let mut converted = CrosstermKeyModifiers::empty();
    if modifiers.contains(RealmKeyModifiers::SHIFT) {
        converted.insert(CrosstermKeyModifiers::SHIFT);
    }
    if modifiers.contains(RealmKeyModifiers::CONTROL) {
        converted.insert(CrosstermKeyModifiers::CONTROL);
    }
    if modifiers.contains(RealmKeyModifiers::ALT) {
        converted.insert(CrosstermKeyModifiers::ALT);
    }
    converted
}

fn convert_mouse_event(mouse: RealmMouseEvent) -> CrosstermMouseEvent {
    CrosstermMouseEvent {
        kind: convert_mouse_kind(mouse.kind),
        column: mouse.column,
        row: mouse.row,
        modifiers: convert_key_modifiers(mouse.modifiers),
    }
}

fn convert_mouse_kind(kind: RealmMouseEventKind) -> CrosstermMouseEventKind {
    match kind {
        RealmMouseEventKind::Down(button) => CrosstermMouseEventKind::Down(convert_button(button)),
        RealmMouseEventKind::Up(button) => CrosstermMouseEventKind::Up(convert_button(button)),
        RealmMouseEventKind::Drag(button) => CrosstermMouseEventKind::Drag(convert_button(button)),
        RealmMouseEventKind::Moved => CrosstermMouseEventKind::Moved,
        RealmMouseEventKind::ScrollDown => CrosstermMouseEventKind::ScrollDown,
        RealmMouseEventKind::ScrollUp => CrosstermMouseEventKind::ScrollUp,
        RealmMouseEventKind::ScrollLeft => CrosstermMouseEventKind::ScrollUp,
        RealmMouseEventKind::ScrollRight => CrosstermMouseEventKind::ScrollDown,
    }
}

fn convert_button(button: RealmMouseButton) -> CrosstermMouseButton {
    match button {
        RealmMouseButton::Left => CrosstermMouseButton::Left,
        RealmMouseButton::Right => CrosstermMouseButton::Right,
        RealmMouseButton::Middle => CrosstermMouseButton::Middle,
    }
}
