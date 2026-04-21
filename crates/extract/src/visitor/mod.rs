//! Oxc AST visitor for extracting imports, exports, re-exports, and member accesses.

mod declarations;
mod helpers;
mod visit_impl;

use oxc_ast::ast::{
    Argument, BindingPattern, CallExpression, Expression, ImportExpression, ObjectPattern,
    ObjectPropertyKind, Statement,
};
use oxc_span::Span;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::suppress::Suppression;
use crate::{
    DynamicImportInfo, DynamicImportPattern, ExportInfo, ExportName, ImportInfo, MemberAccess,
    MemberInfo, ModuleInfo, ReExportInfo, RequireCallInfo, VisibilityTag,
};
use fallow_types::extract::ClassHeritageInfo;

#[derive(Debug, Clone)]
struct LocalClassExportInfo {
    members: Vec<MemberInfo>,
    super_class: Option<String>,
    implemented_interfaces: Vec<String>,
}

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
    /// Local bindings and `this.<field>` aliases resolved to a target symbol name.
    /// Used so `x.method()` or `this.service.method()` can be mapped back to the
    /// imported/exported class or interface that owns the member.
    binding_target_names: FxHashMap<String, String>,
    /// Nesting depth inside `TSModuleDeclaration` (namespace) bodies.
    /// When > 0, inner `export` declarations are collected as namespace members
    /// instead of being extracted as top-level module exports.
    namespace_depth: u32,
    /// Members collected while walking a namespace body.
    /// Moved to the namespace's `ExportInfo.members` after the walk completes.
    pending_namespace_members: Vec<MemberInfo>,
    /// Heritage metadata for exported classes.
    pub(crate) class_heritage: Vec<ClassHeritageInfo>,
    /// Module-scope local class declarations keyed by local binding name.
    local_class_exports: FxHashMap<String, LocalClassExportInfo>,
    /// Block nesting depth used to distinguish module-scope declarations.
    block_depth: u32,
    /// Function / arrow-function nesting depth used to distinguish module scope.
    function_depth: u32,
    /// Stack of super-class names for classes currently being walked.
    /// Each frame holds the local identifier from the `extends` clause, or `None`
    /// when the class has no super class (or an unanalyzable one like `extends mixin()`).
    /// Read when a `super.member` access is encountered, so it can be recorded as
    /// `MemberAccess { object: <super_local>, member }`. Dropped when the entry is `None`.
    pub(crate) class_super_stack: Vec<Option<String>>,
}

impl ModuleInfoExtractor {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_local_class_export(
        &mut self,
        name: String,
        members: Vec<MemberInfo>,
        super_class: Option<String>,
        implemented_interfaces: Vec<String>,
    ) {
        self.local_class_exports.insert(
            name,
            LocalClassExportInfo {
                members,
                super_class,
                implemented_interfaces,
            },
        );
    }

    fn enrich_local_class_exports(&mut self) {
        if self.local_class_exports.is_empty() {
            return;
        }

        for export in &mut self.exports {
            let Some(local_name) = export.local_name.as_deref() else {
                continue;
            };
            let Some(local_class) = self.local_class_exports.get(local_name) else {
                continue;
            };

            if export.members.is_empty() {
                export.members = local_class.members.clone();
            }
            if export.super_class.is_none() {
                export.super_class = local_class.super_class.clone();
            }

            let export_name = export.name.to_string();
            let already_has_heritage = self
                .class_heritage
                .iter()
                .any(|heritage| heritage.export_name == export_name);
            if !already_has_heritage
                && (local_class.super_class.is_some()
                    || !local_class.implemented_interfaces.is_empty())
            {
                self.class_heritage.push(ClassHeritageInfo {
                    export_name,
                    super_class: local_class.super_class.clone(),
                    implements: local_class.implemented_interfaces.clone(),
                });
            }
        }
    }

    /// Map bound member accesses to their target symbol member accesses.
    ///
    /// When `const x = new Foo()` and later `x.bar()`, or `const x: Service`
    /// and later `x.bar()`, emit an additional `MemberAccess` against the
    /// resolved symbol name so the analysis layer can track the member usage.
    fn resolve_bound_member_accesses(&mut self) {
        if self.binding_target_names.is_empty() {
            return;
        }
        let additional_accesses: Vec<MemberAccess> = self
            .member_accesses
            .iter()
            .filter_map(|access| {
                self.binding_target_names
                    .get(&access.object)
                    .map(|target_name| MemberAccess {
                        object: target_name.clone(),
                        member: access.member.clone(),
                    })
            })
            .collect();
        let additional_whole: Vec<String> = self
            .whole_object_uses
            .iter()
            .filter_map(|name| self.binding_target_names.get(name).cloned())
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
            visibility: VisibilityTag::None,
            span,
            members: vec![],
            super_class: None,
        });
    }

    /// Convert this extractor into a `ModuleInfo`, consuming its fields.
    pub(crate) fn into_module_info(
        mut self,
        file_id: fallow_types::discover::FileId,
        content_hash: u64,
        suppressions: Vec<Suppression>,
    ) -> ModuleInfo {
        self.enrich_local_class_exports();
        self.resolve_bound_member_accesses();
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
            class_heritage: self.class_heritage,
        }
    }

    /// Merge this extractor's fields into an existing `ModuleInfo`.
    pub(crate) fn merge_into(mut self, info: &mut ModuleInfo) {
        self.enrich_local_class_exports();
        self.resolve_bound_member_accesses();
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
        info.class_heritage.extend(self.class_heritage);
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

/// Try to extract a dynamic `import()` expression wrapped in an arrow function
/// that appears as an argument to a call expression. This covers patterns like:
///
/// - `React.lazy(() => import('./Foo'))`
/// - `loadable(() => import('./Component'))`
/// - `defineAsyncComponent(() => import('./View'))`
///
/// Returns `(import_expr, source_string)` on success.
fn try_extract_arrow_wrapped_import<'a, 'b>(
    arguments: &'b [Argument<'a>],
) -> Option<(&'b ImportExpression<'a>, &'b str)> {
    for arg in arguments {
        let import_expr = match arg {
            Argument::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    // Expression body: `() => import('./x')`
                    let Some(Statement::ExpressionStatement(expr_stmt)) =
                        arrow.body.statements.first()
                    else {
                        continue;
                    };
                    let Expression::ImportExpression(imp) = &expr_stmt.expression else {
                        continue;
                    };
                    imp
                } else {
                    // Block body: `() => { return import('./x'); }`
                    let Some(imp) = extract_import_from_return_body(&arrow.body.statements) else {
                        continue;
                    };
                    imp
                }
            }
            Argument::FunctionExpression(func) => {
                // `function() { return import('./x'); }`
                let Some(body) = &func.body else {
                    continue;
                };
                let Some(imp) = extract_import_from_return_body(&body.statements) else {
                    continue;
                };
                imp
            }
            _ => continue,
        };
        let Expression::StringLiteral(lit) = &import_expr.source else {
            continue;
        };
        return Some((import_expr, &lit.value));
    }
    None
}

/// Extract an `import()` expression from a block body's return statement.
fn extract_import_from_return_body<'a, 'b>(
    stmts: &'b [Statement<'a>],
) -> Option<&'b ImportExpression<'a>> {
    for stmt in stmts.iter().rev() {
        if let Statement::ReturnStatement(ret) = stmt
            && let Some(Expression::ImportExpression(imp)) = &ret.argument
        {
            return Some(imp);
        }
    }
    None
}

/// Result from extracting a `.then()` callback on a dynamic import.
struct ImportThenCallback {
    /// The import specifier string (e.g., `"./lib"`).
    source: String,
    /// The span of the `import()` expression (for dedup).
    import_span: oxc_span::Span,
    /// Named exports accessed in the callback, if extractable.
    destructured_names: Vec<String>,
    /// The callback parameter name if it's a simple identifier binding,
    /// for namespace-style narrowing when specific member names cannot
    /// be statically extracted from the body.
    local_name: Option<String>,
}

/// Try to extract a `.then()` callback on a dynamic `import()` expression.
///
/// Handles patterns like:
/// - `import('./lib').then(m => m.foo)` — expression body member access
/// - `import('./lib').then(({ foo, bar }) => { ... })` — param destructuring
/// - `import('./lib').then(m => { ... m.foo ... })` — namespace binding
///
/// Returns extraction results on success.
fn try_extract_import_then_callback(expr: &CallExpression<'_>) -> Option<ImportThenCallback> {
    // Callee must be `<something>.then`
    let Expression::StaticMemberExpression(member) = &expr.callee else {
        return None;
    };
    if member.property.name != "then" {
        return None;
    }

    // The object must be an `import('...')` expression with a string literal source
    let Expression::ImportExpression(import_expr) = &member.object else {
        return None;
    };
    let Expression::StringLiteral(lit) = &import_expr.source else {
        return None;
    };
    let source = lit.value.to_string();
    let import_span = import_expr.span;

    // First argument must be a callback (arrow or function expression)
    let first_arg = expr.arguments.first()?;

    match first_arg {
        Argument::ArrowFunctionExpression(arrow) => {
            let param = arrow.params.items.first()?;
            match &param.pattern {
                // Destructured: `({ foo, bar }) => ...`
                BindingPattern::ObjectPattern(obj_pat) => Some(ImportThenCallback {
                    source,
                    import_span,
                    destructured_names: extract_destructured_names(obj_pat),
                    local_name: None,
                }),
                // Identifier: `m => m.foo` or `m => { ... }`
                BindingPattern::BindingIdentifier(id) => {
                    let param_name = id.name.to_string();

                    // For expression bodies, try to extract direct member access
                    if arrow.expression
                        && let Some(Statement::ExpressionStatement(expr_stmt)) =
                            arrow.body.statements.first()
                        && let Some(names) =
                            extract_member_names_from_expr(&expr_stmt.expression, &param_name)
                    {
                        return Some(ImportThenCallback {
                            source,
                            import_span,
                            destructured_names: names,
                            local_name: None,
                        });
                    }

                    // Fall back to namespace binding for narrowing
                    Some(ImportThenCallback {
                        source,
                        import_span,
                        destructured_names: Vec::new(),
                        local_name: Some(param_name),
                    })
                }
                _ => None,
            }
        }
        Argument::FunctionExpression(func) => {
            let param = func.params.items.first()?;
            match &param.pattern {
                BindingPattern::ObjectPattern(obj_pat) => Some(ImportThenCallback {
                    source,
                    import_span,
                    destructured_names: extract_destructured_names(obj_pat),
                    local_name: None,
                }),
                BindingPattern::BindingIdentifier(id) => Some(ImportThenCallback {
                    source,
                    import_span,
                    destructured_names: Vec::new(),
                    local_name: Some(id.name.to_string()),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Extract member names from an expression that accesses the given parameter.
///
/// Handles:
/// - `m.foo` → `["foo"]`
/// - `({ default: m.Foo })` → `["Foo"]` (React.lazy `.then` pattern)
fn extract_member_names_from_expr(expr: &Expression<'_>, param_name: &str) -> Option<Vec<String>> {
    match expr {
        // `m.foo`
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object
                && obj.name == param_name
            {
                Some(vec![member.property.name.to_string()])
            } else {
                None
            }
        }
        // `({ default: m.Foo })` — wrapped in parens as object literal
        Expression::ObjectExpression(obj) => extract_member_names_from_object(obj, param_name),
        // Parenthesized: `(expr)` — unwrap and recurse
        Expression::ParenthesizedExpression(paren) => {
            extract_member_names_from_expr(&paren.expression, param_name)
        }
        _ => None,
    }
}

/// Extract member names from object literal properties that access the given parameter.
fn extract_member_names_from_object(
    obj: &oxc_ast::ast::ObjectExpression<'_>,
    param_name: &str,
) -> Option<Vec<String>> {
    let mut names = Vec::new();
    for prop in &obj.properties {
        if let ObjectPropertyKind::ObjectProperty(p) = prop
            && let Expression::StaticMemberExpression(member) = &p.value
            && let Expression::Identifier(obj) = &member.object
            && obj.name == param_name
        {
            names.push(member.property.name.to_string());
        }
    }
    if names.is_empty() { None } else { Some(names) }
}

#[cfg(all(test, not(miri)))]
mod tests;
