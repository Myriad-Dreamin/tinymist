//! GFM task list support
//!
//! This module provides utilities for working with GitHub Flavored Markdown
//! task lists (checkboxes).

use crate::ast::Node;
pub use crate::ast::TaskListStatus;

/// Creates a checked (completed) task list item
///
/// # Arguments
/// * `content` - The content of the task list item
///
/// # Returns
/// A task list item node with a checked status
pub fn checked_task(content: Vec<Node>) -> Node {
    Node::task_list_item(TaskListStatus::Checked, content)
}

/// Creates an unchecked (pending) task list item
///
/// # Arguments
/// * `content` - The content of the task list item
///
/// # Returns
/// A task list item node with an unchecked status
pub fn unchecked_task(content: Vec<Node>) -> Node {
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
        .map(|(status, content)| match status {
            TaskListStatus::Checked => checked_task(content),
            TaskListStatus::Unchecked => unchecked_task(content),
        })
        .collect::<Vec<_>>();

    Node::Document(list_items)
}
