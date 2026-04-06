use oxc_allocator::Allocator;
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use rustc_hash::FxHashSet;

use crate::MemberAccess;
use crate::visitor::ModuleInfoExtractor;

#[derive(Debug, Default, Clone)]
pub struct TemplateUsage {
    pub(crate) used_bindings: FxHashSet<String>,
    pub(crate) member_accesses: Vec<MemberAccess>,
    pub(crate) whole_object_uses: Vec<String>,
}

impl TemplateUsage {
    pub(crate) fn merge(&mut self, other: Self) {
        self.used_bindings.extend(other.used_bindings);
        for access in other.member_accesses {
            let key = (&access.object, &access.member);
            let already_present = self
                .member_accesses
                .iter()
                .any(|existing| (&existing.object, &existing.member) == key);
            if !already_present {
                self.member_accesses.push(access);
            }
        }
        for whole in other.whole_object_uses {
            if !self
                .whole_object_uses
                .iter()
                .any(|existing| existing == &whole)
            {
                self.whole_object_uses.push(whole);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.used_bindings.is_empty()
            && self.member_accesses.is_empty()
            && self.whole_object_uses.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateSnippetKind {
    Expression,
    Statement,
}

pub fn analyze_template_snippet(
    snippet: &str,
    kind: TemplateSnippetKind,
    imported_bindings: &FxHashSet<String>,
    locals: &[String],
) -> TemplateUsage {
    let snippet = snippet.trim();
    if snippet.is_empty() || imported_bindings.is_empty() {
        return TemplateUsage::default();
    }

    let wrapped = wrap_snippet(snippet, kind, locals);
    let allocator = Allocator::default();
    let parser_return = Parser::new(&allocator, &wrapped, SourceType::ts()).parse();

    let semantic_ret = SemanticBuilder::new().build(&parser_return.program);
    let unresolved_names: FxHashSet<String> = semantic_ret
        .semantic
        .scoping()
        .root_unresolved_references()
        .keys()
        .filter_map(|name| {
            let name = name.as_str();
            imported_bindings.contains(name).then(|| name.to_string())
        })
        .collect();

    if unresolved_names.is_empty() {
        return TemplateUsage::default();
    }

    let mut extractor = ModuleInfoExtractor::new();
    extractor.visit_program(&parser_return.program);

    TemplateUsage {
        used_bindings: unresolved_names.clone(),
        member_accesses: dedup_member_accesses(
            extractor
                .member_accesses
                .into_iter()
                .filter(|access| unresolved_names.contains(&access.object))
                .collect(),
        ),
        whole_object_uses: dedup_names(
            extractor
                .whole_object_uses
                .into_iter()
                .filter(|name| unresolved_names.contains(name))
                .collect(),
        ),
    }
}

fn wrap_snippet(snippet: &str, kind: TemplateSnippetKind, locals: &[String]) -> String {
    let mut wrapped = String::new();
    if !locals.is_empty() {
        wrapped.push_str("const __fallow_local = undefined;\n");
        for local in locals {
            wrapped.push_str("const ");
            wrapped.push_str(local);
            wrapped.push_str(" = __fallow_local;\n");
        }
    }

    match kind {
        TemplateSnippetKind::Expression => {
            wrapped.push_str("void (");
            wrapped.push_str(snippet);
            wrapped.push_str(");\n");
        }
        TemplateSnippetKind::Statement => {
            wrapped.push_str("(() => {\n");
            wrapped.push_str(snippet);
            wrapped.push_str("\n})();\n");
        }
    }

    wrapped
}

fn dedup_member_accesses(accesses: Vec<MemberAccess>) -> Vec<MemberAccess> {
    let mut seen: FxHashSet<(String, String)> = FxHashSet::default();
    let mut deduped = Vec::with_capacity(accesses.len());
    for access in accesses {
        let key = (access.object.clone(), access.member.clone());
        if seen.insert(key) {
            deduped.push(access);
        }
    }
    deduped
}

fn dedup_names(names: Vec<String>) -> Vec<String> {
    let mut seen: FxHashSet<String> = FxHashSet::default();
    let mut deduped = Vec::with_capacity(names.len());
    for name in names {
        if seen.insert(name.clone()) {
            deduped.push(name);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn expression_usage_tracks_named_bindings() {
        let usage = analyze_template_snippet(
            "formatDate(user.createdAt)",
            TemplateSnippetKind::Expression,
            &bindings(&["formatDate"]),
            &[],
        );

        assert!(usage.used_bindings.contains("formatDate"));
        assert!(usage.member_accesses.is_empty());
        assert!(usage.whole_object_uses.is_empty());
    }

    #[test]
    fn expression_usage_tracks_namespace_members() {
        let usage = analyze_template_snippet(
            "utils.formatDate(user.createdAt)",
            TemplateSnippetKind::Expression,
            &bindings(&["utils"]),
            &[],
        );

        assert!(usage.used_bindings.contains("utils"));
        assert_eq!(usage.member_accesses.len(), 1);
        assert_eq!(usage.member_accesses[0].object, "utils");
        assert_eq!(usage.member_accesses[0].member, "formatDate");
    }

    #[test]
    fn locals_shadow_imported_names() {
        let usage = analyze_template_snippet(
            "item.name",
            TemplateSnippetKind::Expression,
            &bindings(&["item"]),
            &["item".to_string()],
        );

        assert!(usage.is_empty());
    }

    #[test]
    fn statement_usage_tracks_handler_references() {
        let usage = analyze_template_snippet(
            "count += increment(step);",
            TemplateSnippetKind::Statement,
            &bindings(&["increment"]),
            &[],
        );

        assert!(usage.used_bindings.contains("increment"));
    }
}
