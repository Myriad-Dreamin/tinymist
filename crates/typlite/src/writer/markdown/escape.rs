pub(super) fn escape_markdown_text(text: &str, escape_special_chars: bool) -> String {
    if !escape_special_chars || text.is_empty() {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '*' => out.push_str("\\*"),
            '_' => out.push_str("\\_"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '>' => out.push_str("\\>"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn escape_markdown_url(url: &str) -> String {
    // The existing snapshots don't require URL escaping beyond raw output.
    url.to_string()
}

pub(super) fn indent_multiline(text: &str, indent: usize) -> String {
    if indent == 0 || text.is_empty() {
        return text.to_string();
    }
    let prefix = " ".repeat(indent);
    text.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn append_to_last_line(out: &mut String, suffix: &str) {
    if out.ends_with('\n') {
        out.pop();
        if out.ends_with('\r') {
            out.pop();
        }
        out.push_str(suffix);
        out.push('\n');
        return;
    }
    out.push_str(suffix);
}

fn max_consecutive_backticks(text: &str) -> usize {
    let mut max_run = 0usize;
    let mut current = 0usize;
    for ch in text.chars() {
        if ch == '`' {
            current += 1;
            max_run = max_run.max(current);
        } else {
            current = 0;
        }
    }
    max_run
}

pub(super) fn render_inline_code(code: &str) -> String {
    let ticks = max_consecutive_backticks(code);
    let fence = "`".repeat((ticks + 1).max(1));
    format!("{fence}{code}{fence}")
}

pub(super) fn indent_code_fence_block(
    language: Option<&str>,
    content: &str,
    indent: usize,
) -> String {
    let max_ticks = max_consecutive_backticks(content);
    let fence = "`".repeat((max_ticks + 1).max(3));
    let mut out = String::new();
    out.push_str(&" ".repeat(indent));
    out.push_str(&fence);
    if let Some(lang) = language
        && !lang.is_empty()
    {
        out.push_str(lang);
    }
    out.push('\n');

    let mut content = content.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    out.push_str(&indent_multiline(content.trim_end_matches('\n'), indent));
    out.push('\n');
    out.push_str(&" ".repeat(indent));
    out.push_str(&fence);
    out
}
