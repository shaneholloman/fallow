use rustc_hash::FxHashSet;

use crate::template_usage::{TemplateSnippetKind, TemplateUsage, analyze_template_snippet};

pub(super) fn merge_expression_usage(
    usage: &mut TemplateUsage,
    snippet: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) {
    merge_snippet_usage(
        usage,
        snippet,
        TemplateSnippetKind::Expression,
        imported_bindings,
        locals,
    );
}

pub(super) fn merge_statement_usage(
    usage: &mut TemplateUsage,
    snippet: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) {
    merge_snippet_usage(
        usage,
        snippet,
        TemplateSnippetKind::Statement,
        imported_bindings,
        locals,
    );
}

fn merge_snippet_usage(
    usage: &mut TemplateUsage,
    snippet: &str,
    kind: TemplateSnippetKind,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) {
    usage.merge(analyze_template_snippet(
        snippet,
        kind,
        imported_bindings,
        locals,
    ));
}

pub(super) fn extract_pattern_binding_names(pattern: &str) -> Vec<String> {
    let pattern = trim_outer_parens(pattern.trim());
    let pattern = pattern.strip_prefix("...").unwrap_or(pattern).trim();
    if pattern.is_empty() {
        return Vec::new();
    }

    if let Some(inner) = strip_wrapping(pattern, '{', '}') {
        return split_top_level(inner, ',')
            .into_iter()
            .flat_map(|part| {
                let part = part.trim();
                if part.is_empty() || part == "..." {
                    return Vec::new();
                }
                if let Some((_, rhs)) = split_top_level_once(part, ':') {
                    return extract_pattern_binding_names(rhs);
                }
                if let Some((lhs, _)) = split_top_level_once(part, '=') {
                    return extract_pattern_binding_names(lhs);
                }
                extract_pattern_binding_names(part)
            })
            .collect();
    }

    if let Some(inner) = strip_wrapping(pattern, '[', ']') {
        return split_top_level(inner, ',')
            .into_iter()
            .flat_map(|part| extract_pattern_binding_names(part.trim()))
            .collect();
    }

    if pattern.contains(',') {
        return split_top_level(pattern, ',')
            .into_iter()
            .flat_map(|part| extract_pattern_binding_names(part.trim()))
            .collect();
    }

    if let Some((lhs, _)) = split_top_level_once(pattern, '=') {
        return extract_pattern_binding_names(lhs);
    }

    valid_identifier(pattern)
        .map(|ident| vec![ident.to_string()])
        .unwrap_or_default()
}

fn split_top_level(source: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for (idx, ch) in source.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ if in_single || in_double || in_backtick => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                parts.push(&source[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&source[start..]);
    parts
}

fn split_top_level_once(source: &str, delimiter: char) -> Option<(&str, &str)> {
    let mut depth = 0_i32;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut escape = false;

    for (idx, ch) in source.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        match ch {
            '\\' if in_single || in_double || in_backtick => {
                escape = true;
            }
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ if in_single || in_double || in_backtick => {}
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if ch == delimiter && depth == 0 => {
                let rhs = &source[idx + ch.len_utf8()..];
                return Some((&source[..idx], rhs));
            }
            _ => {}
        }
    }
    None
}

fn strip_wrapping(source: &str, open: char, close: char) -> Option<&str> {
    source
        .strip_prefix(open)
        .and_then(|inner| inner.strip_suffix(close))
}

fn trim_outer_parens(source: &str) -> &str {
    source
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(source)
}

fn valid_identifier(source: &str) -> Option<&str> {
    let mut chars = source.chars();
    let first = chars.next()?;
    if !matches!(first, 'A'..='Z' | 'a'..='z' | '_' | '$') {
        return None;
    }
    chars
        .all(|ch| matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '$'))
        .then_some(source)
}

#[cfg(test)]
mod tests {
    use super::extract_pattern_binding_names;

    #[test]
    fn extracts_nested_object_pattern_bindings() {
        assert_eq!(
            extract_pattern_binding_names("{ item: { id, label }, count = 0 }"),
            vec!["id", "label", "count"],
        );
    }

    #[test]
    fn extracts_array_pattern_bindings() {
        assert_eq!(
            extract_pattern_binding_names("[first, , { value: second }, ...rest]"),
            vec!["first", "second", "rest"],
        );
    }

    #[test]
    fn extracts_comma_separated_parameters() {
        assert_eq!(
            extract_pattern_binding_names("item, index = 0"),
            vec!["item", "index"],
        );
    }
}
