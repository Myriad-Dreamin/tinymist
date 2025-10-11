//! Utility functions for HTML writing.

/// Check if an HTML tag name is safe
///
/// Tag names should only contain letters, numbers, underscores, colons, and hyphens.
pub(crate) fn is_safe_tag_name(tag: &str) -> bool {
    !tag.is_empty()
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-')
}

/// Check if an HTML attribute name is safe
///
/// Attribute names should only contain letters, numbers, underscores, colons, dots, and hyphens.
pub(crate) fn is_safe_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '-' || c == '.')
}
