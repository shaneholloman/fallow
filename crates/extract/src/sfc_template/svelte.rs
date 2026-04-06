use std::sync::LazyLock;

use rustc_hash::FxHashSet;

use crate::template_usage::TemplateUsage;

use super::scanners::scan_curly_section;
use super::shared::{extract_pattern_binding_names, merge_expression_usage, merge_statement_usage};

static STYLE_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<style\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</style>"#)
        .expect("valid regex")
});

static SCRIPT_BLOCK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<script\b(?:[^>"']|"[^"]*"|'[^']*')*>(?P<body>[\s\S]*?)</script>"#)
        .expect("valid regex")
});

static HTML_COMMENT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<!--.*?-->").expect("valid regex"));

static SVELTE_EACH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?is)^#each\s+(?P<iterable>.+?)\s+as\s+(?P<bindings>.+?)(?:\s*\((?P<key>.+)\))?$",
    )
    .expect("valid regex")
});

static SVELTE_AWAIT_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?is)^#await\s+(?P<expr>.+)$").expect("valid regex"));

static SVELTE_THEN_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:then(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_CATCH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^:catch(?:\s+(?P<binding>.+))?$").expect("valid regex")
});

static SVELTE_SNIPPET_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)^#snippet\s+[A-Za-z_$][\w$]*\s*\((?P<params>.*)\)\s*$")
        .expect("valid regex")
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum SvelteBlockKind {
    Root,
    If,
    Each,
    Await,
    Key,
    Snippet,
}

#[derive(Debug, Clone)]
struct SvelteScopeFrame {
    kind: SvelteBlockKind,
    locals: Vec<String>,
}

pub(super) fn collect_template_usage(
    source: &str,
    imported_bindings: &FxHashSet<String>,
) -> TemplateUsage {
    if imported_bindings.is_empty() {
        return TemplateUsage::default();
    }

    let markup = strip_non_template_content(source);
    if markup.is_empty() {
        return TemplateUsage::default();
    }

    let mut usage = TemplateUsage::default();
    let mut scopes = vec![SvelteScopeFrame {
        kind: SvelteBlockKind::Root,
        locals: Vec::new(),
    }];

    let bytes = markup.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
            continue;
        }

        let Some((tag, next_index)) = scan_curly_section(&markup, index, 1, 1) else {
            break;
        };
        apply_tag(tag.trim(), imported_bindings, &mut scopes, &mut usage);
        index = next_index;
    }

    usage
}

fn strip_non_template_content(source: &str) -> String {
    let mut hidden_ranges: Vec<(usize, usize)> = Vec::new();
    hidden_ranges.extend(
        HTML_COMMENT_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        SCRIPT_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.extend(
        STYLE_BLOCK_RE
            .find_iter(source)
            .map(|m| (m.start(), m.end())),
    );
    hidden_ranges.sort_unstable_by_key(|range| range.0);

    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(hidden_ranges.len());
    for (start, end) in hidden_ranges {
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut visible = String::new();
    let mut cursor = 0;
    for (start, end) in merged {
        if cursor < start {
            visible.push_str(&source[cursor..start]);
        }
        cursor = end;
    }
    if cursor < source.len() {
        visible.push_str(&source[cursor..]);
    }
    visible
}

fn apply_tag(
    tag: &str,
    imported_bindings: &FxHashSet<String>,
    scopes: &mut Vec<SvelteScopeFrame>,
    usage: &mut TemplateUsage,
) {
    if tag.is_empty() {
        return;
    }

    if let Some(rest) = tag.strip_prefix('/') {
        pop_scope(scopes, rest.trim());
        return;
    }

    if let Some(expr) = tag.strip_prefix("#if") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::If,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_EACH_RE.captures(tag) {
        let iterable = captures.name("iterable").map_or("", |m| m.as_str()).trim();
        let bindings = captures.name("bindings").map_or("", |m| m.as_str()).trim();
        let each_locals = extract_pattern_binding_names(bindings);
        let current = current_locals(scopes);
        merge_expression_usage(usage, iterable, imported_bindings, &current);
        if let Some(key) = captures.name("key").map(|m| m.as_str().trim())
            && !key.is_empty()
        {
            let mut key_locals = current;
            key_locals.extend(each_locals.iter().cloned());
            merge_expression_usage(usage, key, imported_bindings, &key_locals);
        }
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Each,
            locals: each_locals,
        });
        return;
    }

    if let Some(captures) = SVELTE_AWAIT_RE.captures(tag) {
        let expr = captures.name("expr").map_or("", |m| m.as_str()).trim();
        merge_expression_usage(usage, expr, imported_bindings, &current_locals(scopes));
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Await,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_THEN_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(captures) = SVELTE_CATCH_RE.captures(tag) {
        if let Some(frame) = scopes
            .iter_mut()
            .rev()
            .find(|frame| matches!(frame.kind, SvelteBlockKind::Await))
        {
            frame.locals = captures
                .name("binding")
                .map(|m| extract_pattern_binding_names(m.as_str()))
                .unwrap_or_default();
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("#key") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Key,
            locals: Vec::new(),
        });
        return;
    }

    if let Some(captures) = SVELTE_SNIPPET_RE.captures(tag) {
        let params = captures.name("params").map_or("", |m| m.as_str());
        scopes.push(SvelteScopeFrame {
            kind: SvelteBlockKind::Snippet,
            locals: extract_pattern_binding_names(params),
        });
        return;
    }

    if let Some(expr) = tag.strip_prefix("@html") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix("@render") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(stmt) = tag.strip_prefix("@const") {
        let locals = current_locals(scopes);
        merge_statement_usage(usage, stmt.trim(), imported_bindings, &locals);
        if let Some(lhs) = stmt.split_once('=').map(|(lhs, _)| lhs.trim()) {
            let new_bindings = extract_pattern_binding_names(lhs);
            if let Some(frame) = scopes.last_mut() {
                frame.locals.extend(new_bindings);
            }
        }
        return;
    }

    if let Some(expr) = tag.strip_prefix("@debug") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if let Some(expr) = tag.strip_prefix(":else if") {
        merge_expression_usage(
            usage,
            expr.trim(),
            imported_bindings,
            &current_locals(scopes),
        );
        return;
    }

    if tag.starts_with(":else") {
        return;
    }

    merge_expression_usage(usage, tag, imported_bindings, &current_locals(scopes));
}

fn pop_scope(scopes: &mut Vec<SvelteScopeFrame>, closing: &str) {
    let kind = match closing {
        "if" => Some(SvelteBlockKind::If),
        "each" => Some(SvelteBlockKind::Each),
        "await" => Some(SvelteBlockKind::Await),
        "key" => Some(SvelteBlockKind::Key),
        "snippet" => Some(SvelteBlockKind::Snippet),
        _ => None,
    };

    let Some(kind) = kind else {
        return;
    };

    if let Some(index) = scopes.iter().rposition(|frame| frame.kind == kind)
        && index > 0
    {
        scopes.truncate(index);
    }
}

fn current_locals(scopes: &[SvelteScopeFrame]) -> Vec<String> {
    scopes
        .iter()
        .flat_map(|frame| frame.locals.iter().cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::collect_template_usage;
    use rustc_hash::FxHashSet;

    fn imported(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn plain_expression_marks_binding_used() {
        let usage = collect_template_usage(
            "<script>import { formatDate } from './utils';</script><p>{formatDate(value)}</p>",
            &imported(&["formatDate"]),
        );

        assert!(usage.used_bindings.contains("formatDate"));
    }

    #[test]
    fn each_alias_shadows_import_name() {
        let usage = collect_template_usage(
            "<script>import { item } from './utils';</script>{#each items as item}<p>{item}</p>{/each}",
            &imported(&["item"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn await_then_alias_shadows_import_name() {
        let usage = collect_template_usage(
            "<script>import { value } from './utils';</script>{#await promise}{:then value}<p>{value}</p>{/await}",
            &imported(&["value"]),
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn namespace_member_accesses_are_retained() {
        let usage = collect_template_usage(
            "<script>import * as utils from './utils';</script><p>{utils.formatDate(value)}</p>",
            &imported(&["utils"]),
        );

        assert!(usage.used_bindings.contains("utils"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "utils");
        assert_eq!(usage.member_accesses[0].member, "formatDate");
    }

    #[test]
    fn styles_are_ignored() {
        let usage = collect_template_usage(
            "<style>.button { color: red; }</style><script>import { button } from './utils';</script>",
            &imported(&["button"]),
        );

        assert!(usage.is_empty());
    }
}
