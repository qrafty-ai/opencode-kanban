use crate::app::Message;
use crate::matching::{
    ascii_case_insensitive_subsequence, normalize_fuzzy_needle, recency_frequency_bonus,
    safe_fuzzy_indices,
};
use crate::types::CommandFrequency;
use chrono::Utc;
use nucleo::{Config, Matcher, Utf32Str};
use std::collections::HashMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandDef {
    pub id: &'static str,
    pub display_name: &'static str,
    pub keybinding: &'static str,
    pub message: Option<Message>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RankedCommand {
    pub command_idx: usize,
    pub score: f64,
    pub matched_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandPaletteState {
    pub query: String,
    pub selected_index: usize,
    pub filtered: Vec<RankedCommand>,
    pub frequencies: HashMap<String, CommandFrequency>,
}

impl CommandPaletteState {
    pub fn new(frequencies: HashMap<String, CommandFrequency>) -> Self {
        let mut state = Self {
            query: String::new(),
            selected_index: 0,
            filtered: Vec::new(),
            frequencies,
        };
        state.update_query();
        state
    }

    pub fn update_query(&mut self) {
        let commands = all_commands();
        let previous_len = self.filtered.len();
        self.filtered = rank_commands(&self.query, &commands, &self.frequencies);
        if self.filtered.is_empty() || self.filtered.len() < previous_len {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.filtered.len() - 1);
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected_index = 0;
            return;
        }

        let len = self.filtered.len() as isize;
        let next = (self.selected_index as isize + delta).rem_euclid(len);
        self.selected_index = next as usize;
    }

    pub fn selected_command_id(&self) -> Option<String> {
        let selected = self.filtered.get(self.selected_index)?;
        let commands = all_commands();
        Some(commands.get(selected.command_idx)?.id.to_string())
    }
}

pub fn rank_commands(
    query: &str,
    commands: &[CommandDef],
    frequencies: &HashMap<String, CommandFrequency>,
) -> Vec<RankedCommand> {
    let now = Utc::now();
    let mut ranked = Vec::with_capacity(commands.len());
    let normalized_query = normalize_fuzzy_needle(query);

    if normalized_query.is_empty() {
        for (idx, command) in commands.iter().enumerate() {
            ranked.push(RankedCommand {
                command_idx: idx,
                score: frequency_bonus(command.id, frequencies, now),
                matched_indices: Vec::new(),
            });
        }

        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.command_idx.cmp(&b.command_idx))
        });
        return ranked;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut query_buf = Vec::new();
    let query_utf32 = Utf32Str::new(normalized_query.as_str(), &mut query_buf);
    let mut display_name_buf = Vec::new();
    let mut matched_indices = Vec::new();

    for (idx, command) in commands.iter().enumerate() {
        matched_indices.clear();
        if !ascii_case_insensitive_subsequence(command.display_name, normalized_query.as_str()) {
            continue;
        }
        let display_name_utf32 = Utf32Str::new(command.display_name, &mut display_name_buf);
        if let Some(fuzzy_score) = safe_fuzzy_indices(
            &mut matcher,
            display_name_utf32,
            query_utf32,
            &mut matched_indices,
        ) {
            ranked.push(RankedCommand {
                command_idx: idx,
                score: f64::from(fuzzy_score) + frequency_bonus(command.id, frequencies, now),
                matched_indices: matched_indices
                    .iter()
                    .map(|index| *index as usize)
                    .collect(),
            });
        }
    }

    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.command_idx.cmp(&b.command_idx))
    });
    ranked
}

fn frequency_bonus(
    command_id: &str,
    frequencies: &HashMap<String, CommandFrequency>,
    now: chrono::DateTime<Utc>,
) -> f64 {
    let Some(freq) = frequencies.get(command_id) else {
        return 0.0;
    };

    recency_frequency_bonus(freq.use_count, &freq.last_used, now, 0.3, 0.7, 24.0, 100.0)
}

pub fn all_commands() -> Vec<CommandDef> {
    vec![
        CommandDef {
            id: "switch_project",
            display_name: "Switch Project",
            keybinding: "Ctrl-p",
            message: Some(Message::OpenProjectList),
        },
        CommandDef {
            id: "settings",
            display_name: "Open Settings",
            keybinding: "-",
            message: Some(Message::ExecuteCommand("settings".to_string())),
        },
        CommandDef {
            id: "toggle_view",
            display_name: "Toggle Detail/Kanban View",
            keybinding: "v",
            message: None,
        },
        CommandDef {
            id: "new_task",
            display_name: "New Task",
            keybinding: "n",
            message: Some(Message::OpenNewTaskDialog),
        },
        CommandDef {
            id: "edit_task",
            display_name: "Edit Selected Task",
            keybinding: "e",
            message: Some(Message::OpenEditTaskDialog),
        },
        CommandDef {
            id: "archive_task",
            display_name: "Archive Selected Task",
            keybinding: "a",
            message: Some(Message::OpenArchiveTaskDialog),
        },
        CommandDef {
            id: "open_archive_view",
            display_name: "Open Archive View",
            keybinding: "A",
            message: Some(Message::OpenArchiveView),
        },
        CommandDef {
            id: "attach_task",
            display_name: "Attach Selected Task",
            keybinding: "Enter",
            message: Some(Message::AttachSelectedTask),
        },
        CommandDef {
            id: "open_in_new_terminal",
            display_name: "Open Selected Task in New Terminal",
            keybinding: "o",
            message: Some(Message::OpenSelectedTaskInNewTerminal),
        },
        CommandDef {
            id: "cycle_todo_visualization",
            display_name: "Cycle Todo Visualization",
            keybinding: "t",
            message: Some(Message::CycleTodoVisualization),
        },
        CommandDef {
            id: "add_category",
            display_name: "Add Category",
            keybinding: "c",
            message: Some(Message::OpenAddCategoryDialog),
        },
        CommandDef {
            id: "rename_category",
            display_name: "Rename Category",
            keybinding: "r",
            message: Some(Message::OpenRenameCategoryDialog),
        },
        CommandDef {
            id: "delete_category",
            display_name: "Delete Category",
            keybinding: "x",
            message: Some(Message::OpenDeleteCategoryDialog),
        },
        CommandDef {
            id: "delete_task",
            display_name: "Delete Task",
            keybinding: "D",
            message: Some(Message::OpenDeleteTaskDialog),
        },
        CommandDef {
            id: "move_task_left",
            display_name: "Move Task/Category Left",
            keybinding: "H",
            message: Some(Message::MoveTaskLeft),
        },
        CommandDef {
            id: "move_task_right",
            display_name: "Move Task/Category Right",
            keybinding: "L",
            message: Some(Message::MoveTaskRight),
        },
        CommandDef {
            id: "move_task_up",
            display_name: "Move Task Up",
            keybinding: "K",
            message: Some(Message::MoveTaskUp),
        },
        CommandDef {
            id: "move_task_down",
            display_name: "Move Task Down",
            keybinding: "J",
            message: Some(Message::MoveTaskDown),
        },
        CommandDef {
            id: "toggle_category_edit_mode",
            display_name: "Toggle Category Edit Mode",
            keybinding: "Ctrl+g",
            message: Some(Message::ToggleCategoryEditMode),
        },
        CommandDef {
            id: "navigate_left",
            display_name: "Navigate Left",
            keybinding: "h / ←",
            message: Some(Message::NavigateLeft),
        },
        CommandDef {
            id: "navigate_right",
            display_name: "Navigate Right",
            keybinding: "l / →",
            message: Some(Message::NavigateRight),
        },
        CommandDef {
            id: "select_up",
            display_name: "Select Up",
            keybinding: "k / ↑",
            message: Some(Message::SelectUp),
        },
        CommandDef {
            id: "select_down",
            display_name: "Select Down",
            keybinding: "j / ↓",
            message: Some(Message::SelectDown),
        },
        CommandDef {
            id: "help",
            display_name: "Help",
            keybinding: "?",
            message: None,
        },
        CommandDef {
            id: "quit",
            display_name: "Quit",
            keybinding: "q",
            message: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn test_commands() -> Vec<CommandDef> {
        vec![
            CommandDef {
                id: "alpha_task",
                display_name: "Alpha Task",
                keybinding: "a",
                message: None,
            },
            CommandDef {
                id: "beta_task",
                display_name: "Beta Task",
                keybinding: "b",
                message: None,
            },
            CommandDef {
                id: "close_panel",
                display_name: "Close Panel",
                keybinding: "cp",
                message: None,
            },
            CommandDef {
                id: "archive_item",
                display_name: "Archive Item",
                keybinding: "x",
                message: None,
            },
        ]
    }

    fn freq(command_id: &str, use_count: i64, hours_ago: i64) -> CommandFrequency {
        CommandFrequency {
            command_id: command_id.to_string(),
            use_count,
            last_used: (Utc::now() - Duration::hours(hours_ago)).to_rfc3339(),
        }
    }

    #[test]
    fn test_command_count() {
        let commands = all_commands();
        assert_eq!(
            commands.len(),
            25,
            "Expected 25 commands, found {}",
            commands.len()
        );
    }

    #[test]
    fn test_unique_ids() {
        let commands = all_commands();
        let ids: Vec<&str> = commands.iter().map(|c| c.id).collect();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();
        assert_eq!(ids.len(), sorted_ids.len(), "Duplicate command IDs found");
    }

    #[test]
    fn test_all_have_display_names() {
        for cmd in all_commands() {
            assert!(
                !cmd.display_name.is_empty(),
                "Command '{}' has empty display name",
                cmd.id
            );
        }
    }

    #[test]
    fn test_all_have_keybindings() {
        for cmd in all_commands() {
            assert!(
                !cmd.keybinding.is_empty(),
                "Command '{}' has empty keybinding",
                cmd.id
            );
        }
    }

    #[test]
    fn test_empty_query_returns_all_commands() {
        let commands = test_commands();
        let mut frequencies = HashMap::new();
        frequencies.insert("beta_task".to_string(), freq("beta_task", 10, 1));
        frequencies.insert("alpha_task".to_string(), freq("alpha_task", 1, 120));

        let ranked = rank_commands("", &commands, &frequencies);

        assert_eq!(ranked.len(), commands.len());
        assert_eq!(commands[ranked[0].command_idx].id, "beta_task");
    }

    #[test]
    fn test_fuzzy_match_filters_correctly() {
        let commands = test_commands();
        let ranked = rank_commands("alp", &commands, &HashMap::new());

        assert_eq!(ranked.len(), 1);
        assert_eq!(commands[ranked[0].command_idx].id, "alpha_task");
    }

    #[test]
    fn test_fuzzy_match_normalized_query_prevents_matcher_panic() {
        let commands = test_commands();

        let ranked_upper = rank_commands("ALP", &commands, &HashMap::new());
        assert_eq!(ranked_upper.len(), 1);
        assert_eq!(commands[ranked_upper[0].command_idx].id, "alpha_task");

        let ranked_controls = rank_commands("A\nL\tP", &commands, &HashMap::new());
        assert_eq!(ranked_controls.len(), 1);
        assert_eq!(commands[ranked_controls[0].command_idx].id, "alpha_task");
    }

    #[test]
    fn test_no_matches_returns_empty() {
        let commands = test_commands();
        let ranked = rank_commands("zzz", &commands, &HashMap::new());
        assert!(ranked.is_empty());
    }

    #[test]
    fn test_frequency_boost_breaks_ties() {
        let commands = vec![
            CommandDef {
                id: "task_one",
                display_name: "Task One",
                keybinding: "a",
                message: None,
            },
            CommandDef {
                id: "task_two",
                display_name: "Task Two",
                keybinding: "b",
                message: None,
            },
        ];
        let mut frequencies = HashMap::new();
        frequencies.insert("task_two".to_string(), freq("task_two", 40, 1));
        frequencies.insert("task_one".to_string(), freq("task_one", 1, 240));

        let ranked = rank_commands("task", &commands, &frequencies);
        assert_eq!(commands[ranked[0].command_idx].id, "task_two");
    }

    #[test]
    fn test_recency_boost_applied() {
        let commands = vec![
            CommandDef {
                id: "cmd_old",
                display_name: "Task Alpha",
                keybinding: "a",
                message: None,
            },
            CommandDef {
                id: "cmd_recent",
                display_name: "Task Beta",
                keybinding: "b",
                message: None,
            },
        ];
        let mut frequencies = HashMap::new();
        frequencies.insert("cmd_old".to_string(), freq("cmd_old", 5, 240));
        frequencies.insert("cmd_recent".to_string(), freq("cmd_recent", 5, 1));

        let ranked = rank_commands("task", &commands, &frequencies);
        assert_eq!(commands[ranked[0].command_idx].id, "cmd_recent");
    }

    #[test]
    fn test_fuzzy_score_dominates_frequency() {
        let commands = vec![
            CommandDef {
                id: "exact",
                display_name: "Open Settings",
                keybinding: "s",
                message: None,
            },
            CommandDef {
                id: "weak",
                display_name: "Select Down",
                keybinding: "j",
                message: None,
            },
        ];
        let mut frequencies = HashMap::new();
        frequencies.insert("weak".to_string(), freq("weak", 500, 1));

        let ranked = rank_commands("openset", &commands, &frequencies);
        assert_eq!(commands[ranked[0].command_idx].id, "exact");
    }

    #[test]
    fn test_matched_indices_returned() {
        let commands = test_commands();
        let ranked = rank_commands("at", &commands, &HashMap::new());
        let alpha = ranked
            .iter()
            .find(|item| commands[item.command_idx].id == "alpha_task")
            .expect("alpha_task should be matched");

        assert!(!alpha.matched_indices.is_empty());
    }

    #[test]
    fn test_selection_resets_to_zero_when_filtering_reduces_results() {
        let mut state = CommandPaletteState::new(HashMap::new());
        state.query = "task".to_string();
        state.update_query();
        state.selected_index = 5;
        assert_eq!(state.selected_index, 5);

        state.query = "new".to_string();
        state.update_query();
        assert_eq!(state.selected_index, 0);
    }
}
