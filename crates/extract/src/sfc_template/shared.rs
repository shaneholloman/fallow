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
        false,
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
        false,
    );
}

pub(super) fn merge_expression_usage_allow_dollar_refs(
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
        true,
    );
}

pub(super) fn merge_statement_usage_allow_dollar_refs(
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
        true,
    );
}

fn merge_snippet_usage(
    usage: &mut TemplateUsage,
    snippet: &str,
    kind: TemplateSnippetKind,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
    allow_dollar_prefixed_refs: bool,
) {
    usage.merge(analyze_template_snippet(
        snippet,
        kind,
        imported_bindings,
        locals,
        allow_dollar_prefixed_refs,
    ));
}

pub(super) fn merge_component_tag_usage(
    usage: &mut TemplateUsage,
    tag_name: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
    allow_kebab_case: bool,
) {
    let tag_name = tag_name.trim();
    if tag_name.is_empty() || imported_bindings.is_empty() {
        return;
    }

    if tag_name.contains('.') {
        merge_expression_usage(usage, tag_name, imported_bindings, locals);
        return;
    }

    mark_binding_used(usage, tag_name, imported_bindings, locals);

    if allow_kebab_case && tag_name.contains('-') {
        let camel = kebab_to_camel_case(tag_name);
        if !camel.is_empty() {
            mark_binding_used(usage, &camel, imported_bindings, locals);
            let pascal = uppercase_first(&camel);
            mark_binding_used(usage, &pascal, imported_bindings, locals);
        }
    }
}

fn mark_binding_used(
    usage: &mut TemplateUsage,
    binding: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) {
    if binding.is_empty()
        || locals.iter().any(|local| local == binding)
        || !imported_bindings.contains(binding)
    {
        return;
    }

    usage.used_bindings.insert(binding.to_string());
}

fn kebab_to_camel_case(source: &str) -> String {
    let mut camel = String::new();
    let mut uppercase_next = false;

    for ch in source.chars() {
        if ch == '-' {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            camel.extend(ch.to_uppercase());
            uppercase_next = false;
        } else {
            camel.push(ch);
        }
    }

    camel
}

fn uppercase_first(source: &str) -> String {
    let mut chars = source.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };

    let mut output = String::new();
    output.extend(first.to_uppercase());
    output.push_str(chars.as_str());
    output
}

pub(super) fn merge_pattern_binding_usage(
    usage: &mut TemplateUsage,
    pattern: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) -> Vec<String> {
    let mut bindings = Vec::new();
    collect_pattern_usage(usage, pattern, imported_bindings, locals, &mut bindings);
    bindings
}

fn collect_pattern_usage(
    usage: &mut TemplateUsage,
    pattern: &str,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
    bindings: &mut Vec<String>,
) {
    let pattern = trim_outer_parens(pattern.trim());
    let pattern = pattern.strip_prefix("...").unwrap_or(pattern).trim();
    if pattern.is_empty() {
        return;
    }

    if let Some(inner) = strip_wrapping(pattern, '{', '}') {
        for part in split_top_level(inner, ',') {
            let part = part.trim();
            if part.is_empty() || part == "..." {
                continue;
            }
            if let Some((_, rhs)) = split_top_level_once(part, ':') {
                collect_pattern_usage(usage, rhs, imported_bindings, locals, bindings);
                continue;
            }
            if let Some((lhs, rhs)) = split_top_level_once(part, '=') {
                merge_expression_usage(usage, rhs, imported_bindings, locals);
                collect_pattern_usage(usage, lhs, imported_bindings, locals, bindings);
                continue;
            }
            collect_pattern_usage(usage, part, imported_bindings, locals, bindings);
        }
        return;
    }

    if let Some(inner) = strip_wrapping(pattern, '[', ']') {
        for part in split_top_level(inner, ',') {
            collect_pattern_usage(usage, part.trim(), imported_bindings, locals, bindings);
        }
        return;
    }

    if pattern.contains(',') {
        for part in split_top_level(pattern, ',') {
            collect_pattern_usage(usage, part.trim(), imported_bindings, locals, bindings);
        }
        return;
    }

    if let Some((lhs, rhs)) = split_top_level_once(pattern, '=') {
        merge_expression_usage(usage, rhs, imported_bindings, locals);
        collect_pattern_usage(usage, lhs, imported_bindings, locals, bindings);
        return;
    }

    if let Some(ident) = valid_identifier(pattern) {
        bindings.push(ident.to_string());
    }
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
    use rustc_hash::FxHashSet;

    use super::{
        extract_pattern_binding_names, merge_component_tag_usage, merge_pattern_binding_usage,
    };
    use crate::template_usage::TemplateUsage;

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

    #[test]
    fn pattern_usage_tracks_default_initializer_references() {
        let mut usage = TemplateUsage::default();
        let imported_bindings = FxHashSet::from_iter(["fallbackItem".to_string()]);

        let locals = merge_pattern_binding_usage(
            &mut usage,
            "{ item = fallbackItem }",
            &imported_bindings,
            &[],
        );

        assert_eq!(locals, vec!["item"]);
        assert!(usage.used_bindings.contains("fallbackItem"));
    }

    #[test]
    fn component_tag_usage_marks_exact_binding_used() {
        let mut usage = TemplateUsage::default();
        let imported_bindings =
            FxHashSet::from_iter(["GreetingCard".to_string(), "AlertBox".to_string()]);

        merge_component_tag_usage(&mut usage, "GreetingCard", &imported_bindings, &[], false);

        assert!(usage.used_bindings.contains("GreetingCard"));
        assert!(!usage.used_bindings.contains("AlertBox"));
    }

    #[test]
    fn component_tag_usage_converts_kebab_case_for_vue() {
        let mut usage = TemplateUsage::default();
        let imported_bindings = FxHashSet::from_iter(["MyButton".to_string()]);

        merge_component_tag_usage(&mut usage, "my-button", &imported_bindings, &[], true);

        assert!(usage.used_bindings.contains("MyButton"));
    }

    #[test]
    fn component_tag_usage_respects_shadowing_locals() {
        let mut usage = TemplateUsage::default();
        let imported_bindings = FxHashSet::from_iter(["Item".to_string()]);

        merge_component_tag_usage(
            &mut usage,
            "Item",
            &imported_bindings,
            &["Item".to_string()],
            false,
        );

        assert!(usage.used_bindings.is_empty());
    }

    #[test]
    fn component_tag_usage_tracks_namespaced_members() {
        let mut usage = TemplateUsage::default();
        let imported_bindings = FxHashSet::from_iter(["icons".to_string()]);

        merge_component_tag_usage(&mut usage, "icons.Alert", &imported_bindings, &[], false);

        assert!(usage.used_bindings.contains("icons"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "icons");
        assert_eq!(usage.member_accesses[0].member, "Alert");
    }
}
