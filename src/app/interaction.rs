use tuirealm::ratatui::layout::Rect;

use super::messages::Message;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InteractionLayer {
    Base,
    Overlay,
    Dialog,
    ContextMenu,
}

impl InteractionLayer {
    fn priority(self) -> u8 {
        match self {
            Self::Base => 0,
            Self::Overlay => 1,
            Self::Dialog => 2,
            Self::ContextMenu => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum InteractionKind {
    Hover,
    LeftClick,
    RightClick,
    Scroll,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InteractionNode {
    pub rect: Rect,
    pub message: Message,
    pub layer: InteractionLayer,
    pub hoverable: bool,
    pub left_clickable: bool,
    pub right_clickable: bool,
    pub scrollable: bool,
}

impl InteractionNode {
    pub fn click(layer: InteractionLayer, rect: Rect, message: Message) -> Self {
        Self {
            rect,
            message,
            layer,
            hoverable: true,
            left_clickable: true,
            right_clickable: false,
            scrollable: false,
        }
    }

    pub fn task(layer: InteractionLayer, rect: Rect, message: Message) -> Self {
        Self {
            rect,
            message,
            layer,
            hoverable: true,
            left_clickable: true,
            right_clickable: true,
            scrollable: false,
        }
    }

    fn contains(&self, col: u16, row: u16) -> bool {
        col >= self.rect.x
            && col < self.rect.x + self.rect.width
            && row >= self.rect.y
            && row < self.rect.y + self.rect.height
    }

    fn supports(&self, kind: InteractionKind) -> bool {
        match kind {
            InteractionKind::Hover => self.hoverable,
            InteractionKind::LeftClick => self.left_clickable,
            InteractionKind::RightClick => self.right_clickable,
            InteractionKind::Scroll => self.scrollable,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct InteractionMap {
    nodes: Vec<InteractionNode>,
}

impl InteractionMap {
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    pub fn register(&mut self, node: InteractionNode) {
        self.nodes.push(node);
    }

    pub fn register_click(&mut self, layer: InteractionLayer, rect: Rect, message: Message) {
        self.register(InteractionNode::click(layer, rect, message));
    }

    pub fn register_task(&mut self, layer: InteractionLayer, rect: Rect, message: Message) {
        self.register(InteractionNode::task(layer, rect, message));
    }

    pub fn resolve_message(&self, col: u16, row: u16, kind: InteractionKind) -> Option<Message> {
        self.resolve_node(col, row, kind)
            .map(|node| node.message.clone())
    }

    pub fn resolve_node(
        &self,
        col: u16,
        row: u16,
        kind: InteractionKind,
    ) -> Option<&InteractionNode> {
        let mut best: Option<(usize, &InteractionNode)> = None;
        for (idx, node) in self.nodes.iter().enumerate() {
            if !node.contains(col, row) || !node.supports(kind) {
                continue;
            }
            match best {
                None => best = Some((idx, node)),
                Some((best_idx, best_node)) => {
                    let has_higher_layer = node.layer.priority() > best_node.layer.priority();
                    let same_layer_later_registration =
                        node.layer.priority() == best_node.layer.priority() && idx > best_idx;
                    if has_higher_layer || same_layer_later_registration {
                        best = Some((idx, node));
                    }
                }
            }
        }
        best.map(|(_, node)| node)
    }
}

#[cfg(test)]
mod tests {
    use super::{InteractionKind, InteractionLayer, InteractionMap};
    use crate::app::Message;
    use tuirealm::ratatui::layout::Rect;

    #[test]
    fn resolve_prefers_higher_layer() {
        let mut map = InteractionMap::default();
        let rect = Rect::new(10, 10, 5, 2);

        map.register_click(InteractionLayer::Base, rect, Message::ProjectListSelectUp);
        map.register_click(InteractionLayer::Dialog, rect, Message::DismissDialog);

        let message = map.resolve_message(11, 10, InteractionKind::LeftClick);
        assert_eq!(message, Some(Message::DismissDialog));
    }

    #[test]
    fn resolve_prefers_latest_within_same_layer() {
        let mut map = InteractionMap::default();
        let rect = Rect::new(4, 4, 4, 1);

        map.register_click(InteractionLayer::Base, rect, Message::ProjectListSelectUp);
        map.register_click(InteractionLayer::Base, rect, Message::ProjectListSelectDown);

        let message = map.resolve_message(5, 4, InteractionKind::LeftClick);
        assert_eq!(message, Some(Message::ProjectListSelectDown));
    }

    #[test]
    fn task_nodes_support_right_click() {
        let mut map = InteractionMap::default();
        let rect = Rect::new(0, 0, 10, 10);
        map.register_task(InteractionLayer::Base, rect, Message::SelectTask(1, 2));

        let message = map.resolve_message(2, 2, InteractionKind::RightClick);
        assert_eq!(message, Some(Message::SelectTask(1, 2)));
    }
}
