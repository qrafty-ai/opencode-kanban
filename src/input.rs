use crossterm::event::Event;

use crate::app::Message;

pub fn event_to_message(event: Event) -> Option<Message> {
    match event {
        Event::Key(key_event) => Some(Message::Key(key_event)),
        Event::Mouse(mouse_event) => Some(Message::Mouse(mouse_event)),
        Event::Resize(width, height) => Some(Message::Resize(width, height)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };

    #[test]
    fn test_event_to_message_key_event() {
        let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty());
        let event = Event::Key(key_event);
        let result = event_to_message(event);
        assert!(matches!(result, Some(Message::Key(_))));
    }

    #[test]
    fn test_event_to_message_mouse_event() {
        let mouse_event = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 20,
            modifiers: KeyModifiers::empty(),
        };
        let event = Event::Mouse(mouse_event);
        let result = event_to_message(event);
        assert!(matches!(result, Some(Message::Mouse(_))));
    }

    #[test]
    fn test_event_to_message_resize_event() {
        let event = Event::Resize(80, 24);
        let result = event_to_message(event);
        assert!(matches!(result, Some(Message::Resize(80, 24))));
    }

    #[test]
    fn test_event_to_message_other_event() {
        let event = Event::FocusGained;
        let result = event_to_message(event);
        assert_eq!(result, None);
    }
}
