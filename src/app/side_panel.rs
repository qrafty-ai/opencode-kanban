use std::collections::HashSet;

use uuid::Uuid;

use super::SidePanelRow;
use crate::types::{Category, Task};

pub(crate) fn sorted_categories_with_indexes(categories: &[Category]) -> Vec<(usize, &Category)> {
    let mut out: Vec<(usize, &Category)> = categories.iter().enumerate().collect();
    out.sort_by_key(|(_, category)| category.position);
    out
}

pub(crate) fn side_panel_rows_from(
    categories: &[Category],
    tasks: &[Task],
    collapsed_categories: &HashSet<Uuid>,
) -> Vec<SidePanelRow> {
    let mut rows: Vec<SidePanelRow> = Vec::new();
    for (column_index, category) in sorted_categories_with_indexes(categories) {
        let mut category_tasks: Vec<Task> = tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .cloned()
            .collect();
        category_tasks.sort_by_key(|task| task.position);

        let collapsed = collapsed_categories.contains(&category.id);
        let total_tasks = category_tasks.len();
        let visible_tasks = if collapsed { 0 } else { total_tasks };

        rows.push(SidePanelRow::CategoryHeader {
            column_index,
            category_id: category.id,
            category_name: category.name.clone(),
            category_color: category.color.clone(),
            total_tasks,
            visible_tasks,
            collapsed,
        });

        if collapsed {
            continue;
        }

        for (index_in_column, task) in category_tasks.into_iter().enumerate() {
            rows.push(SidePanelRow::Task {
                column_index,
                index_in_column,
                category_id: category.id,
                task: Box::new(task),
            });
        }
    }
    rows
}

pub(crate) fn selected_task_from_side_panel_rows(
    rows: &[SidePanelRow],
    selected_row: usize,
) -> Option<Task> {
    if rows.is_empty() {
        return None;
    }
    let selected_row = selected_row.min(rows.len().saturating_sub(1));
    match rows.get(selected_row) {
        Some(SidePanelRow::Task { task, .. }) => Some(task.as_ref().clone()),
        _ => None,
    }
}
