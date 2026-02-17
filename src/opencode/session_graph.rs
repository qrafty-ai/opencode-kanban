use crate::types::SessionStatus;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct SessionGraph {
    pub sessions: HashMap<String, SessionStatus>,
    pub parent_child_map: HashMap<String, Vec<String>>,
    pub all_sessions: HashSet<String>,
}

impl SessionGraph {
    pub fn new(sessions: HashMap<String, SessionStatus>) -> Self {
        let all_sessions: HashSet<String> = sessions.keys().cloned().collect();

        Self {
            sessions,
            parent_child_map: HashMap::new(),
            all_sessions,
        }
    }

    pub fn get_session(&self, id: &str) -> Option<&SessionStatus> {
        self.sessions.get(id)
    }

    pub fn children(&self, id: &str) -> &[String] {
        self.parent_child_map
            .get(id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn descendants(&self, root_id: &str) -> Vec<(String, usize)> {
        let mut result = Vec::new();
        self.collect_descendants(root_id, 0, &mut result);
        result
    }

    fn collect_descendants(&self, id: &str, depth: usize, result: &mut Vec<(String, usize)>) {
        if let Some(children) = self.parent_child_map.get(id) {
            for child in children {
                result.push((child.clone(), depth + 1));
                self.collect_descendants(child, depth + 1, result);
            }
        }
    }
}
