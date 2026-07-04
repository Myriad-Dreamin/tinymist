//! GFM task list support
//!
//! This module provides utilities for working with GitHub Flavored Markdown
//! task lists (checkboxes).

pub use crate::ast::TaskListStatus;
use crate::ast::{ListItem, Node};

/// Creates a checked (completed) task list item
///
/// # Arguments
/// * `content` - The content of the task list item
///
/// # Returns
/// A task list item with a checked status
pub fn checked_task(content: Vec<Node>) -> ListItem {
    Node::task_list_item(TaskListStatus::Checked, content)
}

/// Creates an unchecked (pending) task list item
///
/// # Arguments
/// * `content` - The content of the task list item
///
/// # Returns
/// A task list item with an unchecked status
pub fn unchecked_task(content: Vec<Node>) -> ListItem {
    Node::task_list_item(TaskListStatus::Unchecked, content)
}

/// Creates a task list with multiple items
///
/// # Arguments
/// * `items` - A vector of tuples containing the status and content for each task
///
/// # Returns
/// An unordered list node containing task items
pub fn task_list(items: Vec<(TaskListStatus, Vec<Node>)>) -> Node {
    let list_items = items
        .into_iter()
        .map(|(status, content)| Node::task_list_item(status, content))
        .collect::<Vec<_>>();

    Node::UnorderedList(list_items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_task_returns_list_item() {
        let item = checked_task(vec![Node::Text("done".into())]);
        assert_eq!(
            item,
            ListItem::Task {
                status: TaskListStatus::Checked,
                content: vec![Node::Text("done".into())]
            }
        );
    }

    #[test]
    fn task_list_wraps_items_in_unordered_list() {
        let list = task_list(vec![
            (TaskListStatus::Checked, vec![Node::Text("a".into())]),
            (TaskListStatus::Unchecked, vec![Node::Text("b".into())]),
        ]);

        assert_eq!(
            list,
            Node::UnorderedList(vec![
                ListItem::Task {
                    status: TaskListStatus::Checked,
                    content: vec![Node::Text("a".into())]
                },
                ListItem::Task {
                    status: TaskListStatus::Unchecked,
                    content: vec![Node::Text("b".into())]
                }
            ])
        );
    }
}
