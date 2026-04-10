//! Oxc AST visitor for extracting imports, exports, re-exports, and member accesses.

mod declarations;
mod helpers;
mod visit_impl;

use oxc_ast::ast::{Argument, CallExpression, Expression, ImportExpression, ObjectPattern};
use oxc_span::Span;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::suppress::Suppression;
use crate::{
    DynamicImportInfo, DynamicImportPattern, ExportInfo, ExportName, ImportInfo, MemberAccess,
    MemberInfo, ModuleInfo, ReExportInfo, RequireCallInfo,
};

/// AST visitor that extracts all import/export information in a single pass.
#[derive(Default)]
pub(crate) struct ModuleInfoExtractor {
    pub(crate) exports: Vec<ExportInfo>,
    pub(crate) imports: Vec<ImportInfo>,
    pub(crate) re_exports: Vec<ReExportInfo>,
    pub(crate) dynamic_imports: Vec<DynamicImportInfo>,
    pub(crate) dynamic_import_patterns: Vec<DynamicImportPattern>,
    pub(crate) require_calls: Vec<RequireCallInfo>,
    pub(crate) member_accesses: Vec<MemberAccess>,
    pub(crate) whole_object_uses: Vec<String>,
    pub(crate) has_cjs_exports: bool,
    /// Spans of `require()` calls already handled via destructured require detection.
    handled_require_spans: FxHashSet<Span>,
    /// Spans of `import()` expressions already handled via variable declarator detection.
    handled_import_spans: FxHashSet<Span>,
    /// Local names of namespace imports and namespace-like bindings
    /// (e.g., `import * as ns`, `const mod = require(...)`, `const mod = await import(...)`).
    /// Used to detect destructuring patterns like `const { a, b } = ns`.
    namespace_binding_names: Vec<String>,
    /// Local names bound to `new ClassName()` expressions.
    /// Maps local_name -> class_name so that `x.method()` member accesses
    /// on an instance `const x = new Foo()` count against `Foo`'s members.
    instance_binding_names: FxHashMap<String, String>,
    /// Nesting depth inside `TSModuleDeclaration` (namespace) bodies.
    /// When > 0, inner `export` declarations are collected as namespace members
    /// instead of being extracted as top-level module exports.
    namespace_depth: u32,
    /// Members collected while walking a namespace body.
    /// Moved to the namespace's `ExportInfo.members` after the walk completes.
    pending_namespace_members: Vec<MemberInfo>,
}

impl ModuleInfoExtractor {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Map instance member accesses to class member accesses.
    ///
    /// When `const x = new Foo()` and later `x.bar()`, emit an additional
    /// `MemberAccess { object: "Foo", member: "bar" }` so the analysis layer
    /// can track it as usage of Foo's class member. Same for whole-object uses.
    fn resolve_instance_member_accesses(&mut self) {
        if self.instance_binding_names.is_empty() {
            return;
        }
        let additional_accesses: Vec<MemberAccess> = self
            .member_accesses
            .iter()
            .filter_map(|access| {
                self.instance_binding_names
                    .get(&access.object)
                    .map(|class_name| MemberAccess {
                        object: class_name.clone(),
                        member: access.member.clone(),
                    })
            })
            .collect();
        let additional_whole: Vec<String> = self
            .whole_object_uses
            .iter()
            .filter_map(|name| self.instance_binding_names.get(name).cloned())
            .collect();
        self.member_accesses.extend(additional_accesses);
        self.whole_object_uses.extend(additional_whole);
    }

    /// Push a type-only export (type alias or interface).
    fn push_type_export(&mut self, name: &str, span: Span) {
        self.exports.push(ExportInfo {
            name: ExportName::Named(name.to_string()),
            local_name: Some(name.to_string()),
            is_type_only: true,
            is_public: false,
            span,
            members: vec![],
        });
    }

    /// Convert this extractor into a `ModuleInfo`, consuming its fields.
    pub(crate) fn into_module_info(
        mut self,
        file_id: fallow_types::discover::FileId,
        content_hash: u64,
        suppressions: Vec<Suppression>,
    ) -> ModuleInfo {
        self.resolve_instance_member_accesses();
        ModuleInfo {
            file_id,
            exports: self.exports,
            imports: self.imports,
            re_exports: self.re_exports,
            dynamic_imports: self.dynamic_imports,
            dynamic_import_patterns: self.dynamic_import_patterns,
            require_calls: self.require_calls,
            member_accesses: self.member_accesses,
            whole_object_uses: self.whole_object_uses,
            has_cjs_exports: self.has_cjs_exports,
            content_hash,
            suppressions,
            unused_import_bindings: Vec::new(),
            line_offsets: Vec::new(),
            complexity: Vec::new(),
            flag_uses: Vec::new(),
        }
    }

    /// Merge this extractor's fields into an existing `ModuleInfo`.
    pub(crate) fn merge_into(mut self, info: &mut ModuleInfo) {
        self.resolve_instance_member_accesses();
        info.imports.extend(self.imports);
        info.exports.extend(self.exports);
        info.re_exports.extend(self.re_exports);
        info.dynamic_imports.extend(self.dynamic_imports);
        info.dynamic_import_patterns
            .extend(self.dynamic_import_patterns);
        info.require_calls.extend(self.require_calls);
        info.member_accesses.extend(self.member_accesses);
        info.whole_object_uses.extend(self.whole_object_uses);
        info.has_cjs_exports |= self.has_cjs_exports;
    }
}

/// Extract destructured property names from an object pattern.
///
/// Returns an empty `Vec` when a rest element is present (conservative:
/// the caller cannot know which names are captured).
fn extract_destructured_names(obj_pat: &ObjectPattern<'_>) -> Vec<String> {
    if obj_pat.rest.is_some() {
        return Vec::new();
    }
    obj_pat
        .properties
        .iter()
        .filter_map(|prop| prop.key.static_name().map(|n| n.to_string()))
        .collect()
}

/// Try to match `require('...')` from a call expression initializer.
///
/// Returns `(call_expr, source_string)` on success.
fn try_extract_require<'a, 'b>(
    init: &'b Expression<'a>,
) -> Option<(&'b CallExpression<'a>, &'b str)> {
    let Expression::CallExpression(call) = init else {
        return None;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    if callee.name != "require" {
        return None;
    }
    let Some(Argument::StringLiteral(lit)) = call.arguments.first() else {
        return None;
    };
    Some((call, &lit.value))
}

/// Try to extract a dynamic `import()` expression (possibly wrapped in `await`)
/// with a static string source.
///
/// Returns `(import_expr, source_string)` on success.
fn try_extract_dynamic_import<'a, 'b>(
    init: &'b Expression<'a>,
) -> Option<(&'b ImportExpression<'a>, &'b str)> {
    let import_expr = match init {
        Expression::AwaitExpression(await_expr) => match &await_expr.argument {
            Expression::ImportExpression(imp) => imp,
            _ => return None,
        },
        Expression::ImportExpression(imp) => imp,
        _ => return None,
    };
    let Expression::StringLiteral(lit) = &import_expr.source else {
        return None;
    };
    Some((import_expr, &lit.value))
}

#[cfg(all(test, not(miri)))]
mod tests;
