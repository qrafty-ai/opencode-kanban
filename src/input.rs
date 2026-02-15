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
