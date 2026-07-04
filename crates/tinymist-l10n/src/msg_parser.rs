use std::borrow::Cow;

/// Parses a message string, replacing placeholders with values provided by `arg_provider`.
pub fn parse_message<'a>(
    message: &str,
    arg_provider: impl Fn(&str) -> Option<Cow<'a, str>> + 'a,
) -> String {
    let mut result = String::with_capacity(message.len());
    let mut chars = message.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '{' => {
                // Check for escaped brace {{
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume second {
                    result.push('{');
                    continue;
                }

                // Parse placeholder
                match collect_until_close_brace(&mut chars) {
                    PlaceholderResult::Valid(arg_name) => {
                        // Found closing brace
                        if !arg_name.is_empty()
                            && let Some(val) = arg_provider(&arg_name)
                        {
                            result.push_str(&val);
                        } else {
                            // Placeholder not found, output as-is
                            result.push('{');
                            result.push_str(&arg_name);
                            result.push('}');
                        }
                    }
                    PlaceholderResult::Incomplete(arg_name) => {
                        // No closing brace found, output what we have
                        result.push('{');
                        result.push_str(&arg_name);
                    }
                    PlaceholderResult::Empty => {
                        // Iterator exhausted before collecting anything
                        result.push('{');
                    }
                }
            }
            '}' => {
                // Check for escaped brace }}
                if chars.peek() == Some(&'}') {
                    chars.next(); // consume second }
                    result.push('}');
                } else {
                    // Unescaped closing brace outside placeholder
                    result.push(c);
                }
            }
            c => result.push(c),
        }
    }

    result
}

/// Result of parsing a placeholder.
enum PlaceholderResult {
    /// Found a valid placeholder with closing brace
    Valid(String),
    /// No closing brace found, but collected some content
    Incomplete(String),
    /// Iterator exhausted before collecting anything
    Empty,
}

/// Helper: collects characters until closing brace, handling escaped braces.
/// Returns the result of parsing the placeholder.
fn collect_until_close_brace(
    chars: &mut std::iter::Peekable<std::str::Chars>,
) -> PlaceholderResult {
    let mut arg_name = String::new();

    while let Some(&next_char) = chars.peek() {
        match next_char {
            '}' => {
                // Consume the first }
                chars.next();
                // Check if this is escaped (}})
                if chars.peek() == Some(&'}') {
                    // Escaped: consume second } and add one } to arg_name
                    chars.next();
                    arg_name.push('}');
                } else {
                    // Regular closing brace found
                    return PlaceholderResult::Valid(arg_name);
                }
            }
            _ => {
                arg_name.push(chars.next().unwrap());
            }
        }
    }

    // No closing brace found
    if arg_name.is_empty() {
        PlaceholderResult::Empty
    } else {
        PlaceholderResult::Incomplete(arg_name)
    }
}
