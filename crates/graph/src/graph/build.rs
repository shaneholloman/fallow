//! Phase 1 (populate_edges) and Phase 2 (populate_references) of graph construction.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::resolve::{ResolveResult, ResolvedModule};
use fallow_types::discover::{DiscoveredFile, FileId};
use fallow_types::extract::{ExportName, ImportedName};

use super::types::ModuleNode;
use super::types::{ExportSymbol, ReExportEdge, ReferenceKind, SymbolReference};
use super::{Edge, ImportedSymbol, ModuleGraph};

/// Insert into the namespace-imported bitset with bounds checking.
fn record_namespace_import(
    target_id: FileId,
    namespace_imported: &mut fixedbitset::FixedBitSet,
    total_capacity: usize,
) {
    let idx = target_id.0 as usize;
    if idx < total_capacity {
        namespace_imported.insert(idx);
    }
}

/// Track that a file uses an npm package, and optionally record type-only usage.
fn record_package_usage(
    name: &str,
    file_id: FileId,
    package_usage: &mut FxHashMap<String, Vec<FileId>>,
    type_only_package_usage: &mut FxHashMap<String, Vec<FileId>>,
    is_type_only: bool,
) {
    package_usage
        .entry(name.to_owned())
        .or_default()
        .push(file_id);
    if is_type_only {
        type_only_package_usage
            .entry(name.to_owned())
            .or_default()
            .push(file_id);
    }
}

/// Build a `ModuleNode` for a file, including exports, re-export edges, and metadata.
fn build_module_node(
    file: &DiscoveredFile,
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
    entry_point_ids: &FxHashSet<FileId>,
    edge_range: std::ops::Range<usize>,
) -> ModuleNode {
    let mut exports: Vec<ExportSymbol> = module_by_id
        .get(&file.id)
        .map(|m| {
            m.exports
                .iter()
                .map(|e| ExportSymbol {
                    name: e.name.clone(),
                    is_type_only: e.is_type_only,
                    is_public: e.is_public,
                    span: e.span,
                    references: Vec::new(),
                    members: e.members.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Create ExportSymbol entries for re-exports so that consumers
    // importing from this barrel can have their references attached.
    // Without this, `export { Foo } from './source'` on a barrel would
    // not be trackable as an export of the barrel module.
    if let Some(resolved) = module_by_id.get(&file.id) {
        for re in &resolved.re_exports {
            // Skip star re-exports without an alias (`export * from './x'`)
            // — they don't create a named export on the barrel.
            // But `export * as name from './x'` does create one.
            if re.info.exported_name == "*" {
                continue;
            }

            // Avoid duplicates: if an export with this name already exists
            // (e.g. the module both declares and re-exports the same name),
            // skip creating another one.
            let export_name = if re.info.exported_name == "default" {
                ExportName::Default
            } else {
                ExportName::Named(re.info.exported_name.clone())
            };
            let already_exists = exports.iter().any(|e| e.name == export_name);
            if already_exists {
                continue;
            }

            exports.push(ExportSymbol {
                name: export_name,
                is_type_only: re.info.is_type_only,
                is_public: false,
                span: oxc_span::Span::new(0, 0), // re-exports don't have a meaningful span on the barrel
                references: Vec::new(),
                members: Vec::new(),
            });
        }
    }

    let has_cjs_exports = module_by_id
        .get(&file.id)
        .is_some_and(|m| m.has_cjs_exports);

    // Build re-export edges
    let re_export_edges: Vec<ReExportEdge> = module_by_id
        .get(&file.id)
        .map(|m| {
            m.re_exports
                .iter()
                .filter_map(|re| {
                    if let ResolveResult::InternalModule(target_id) = &re.target {
                        Some(ReExportEdge {
                            source_file: *target_id,
                            imported_name: re.info.imported_name.clone(),
                            exported_name: re.info.exported_name.clone(),
                            is_type_only: re.info.is_type_only,
                        })
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    ModuleNode {
        file_id: file.id,
        path: file.path.clone(),
        edge_range,
        exports,
        re_exports: re_export_edges,
        is_entry_point: entry_point_ids.contains(&file.id),
        is_reachable: false,
        has_cjs_exports,
    }
}

/// Check whether an import binding is unused in the source file.
///
/// Returns `true` if the binding should be skipped (unused).
fn is_unused_import_binding(
    sym_local_name: &str,
    sym_imported_name: &ImportedName,
    source_mod: Option<&&ResolvedModule>,
) -> bool {
    !sym_local_name.is_empty()
        && !matches!(sym_imported_name, ImportedName::SideEffect)
        && source_mod.is_some_and(|m| m.unused_import_bindings.iter().any(|n| n == sym_local_name))
}

/// Extract member access names for a given local variable from a resolved module.
fn extract_accessed_members(source_mod: Option<&&ResolvedModule>, local_name: &str) -> Vec<String> {
    source_mod
        .map(|m| {
            m.member_accesses
                .iter()
                .filter(|ma| ma.object == local_name)
                .map(|ma| ma.member.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Mark all exports on a module as referenced by a given source file.
///
/// Deduplicates: skips exports already referenced by `source_id`.
fn mark_all_exports_referenced(
    exports: &mut Vec<ExportSymbol>,
    source_id: FileId,
    import_span: oxc_span::Span,
    kind: &ReferenceKind,
) {
    for export in exports {
        if export.references.iter().all(|r| r.from_file != source_id) {
            export.references.push(SymbolReference {
                from_file: source_id,
                kind: kind.clone(),
                import_span,
            });
        }
    }
}

/// Mark only exports whose names appear in `accessed_members` as referenced.
///
/// Returns the set of member names that were found among the exports.
fn mark_member_exports_referenced(
    exports: &mut [ExportSymbol],
    source_id: FileId,
    accessed_members: &[String],
    import_span: oxc_span::Span,
    kind: &ReferenceKind,
) -> FxHashSet<String> {
    let mut found_members: FxHashSet<String> = FxHashSet::default();
    for export in exports {
        let name_str = export.name.to_string();
        if accessed_members.contains(&name_str) {
            found_members.insert(name_str);
            if export.references.iter().all(|r| r.from_file != source_id) {
                export.references.push(SymbolReference {
                    from_file: source_id,
                    kind: kind.clone(),
                    import_span,
                });
            }
        }
    }
    found_members
}

/// Create synthetic `ExportSymbol` entries for members accessed via namespace import
/// that were not found among the target's own exports, but the target has `export *`
/// re-exports that may forward those names.
fn create_synthetic_exports_for_star_re_exports(
    exports: &mut Vec<ExportSymbol>,
    re_exports: &[ReExportEdge],
    source_id: FileId,
    accessed_members: &[String],
    found_members: &FxHashSet<String>,
    import_span: oxc_span::Span,
) {
    let has_star_re_exports = re_exports.iter().any(|re| re.exported_name == "*");
    if !has_star_re_exports {
        return;
    }
    for member in accessed_members {
        if found_members.contains(member) {
            continue;
        }
        let export_name = if member == "default" {
            ExportName::Default
        } else {
            ExportName::Named(member.clone())
        };
        exports.push(ExportSymbol {
            name: export_name,
            is_type_only: false,
            is_public: false,
            span: oxc_span::Span::new(0, 0),
            references: vec![SymbolReference {
                from_file: source_id,
                kind: ReferenceKind::NamespaceImport,
                import_span,
            }],
            members: Vec::new(),
        });
    }
}

/// Handle namespace import narrowing for `import * as ns from './x'`.
///
/// If member accesses can be determined, only those exports are marked as used.
/// Otherwise, all exports are conservatively marked as referenced.
fn narrow_namespace_references(
    module: &mut ModuleNode,
    source_id: FileId,
    sym_local_name: &str,
    sym_import_span: oxc_span::Span,
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
    entry_point_ids: &FxHashSet<FileId>,
) {
    let local_name = sym_local_name;
    let source_mod = module_by_id.get(&source_id);
    let accessed_members = extract_accessed_members(source_mod, local_name);

    // Check if the namespace is consumed as a whole object
    // (Object.values, for..in, spread, destructuring with rest, etc.)
    let is_whole_object =
        source_mod.is_some_and(|m| m.whole_object_uses.iter().any(|n| n == local_name));

    // Check if the namespace variable is re-exported (export { ns } or export default ns)
    // from a NON-entry-point file. If the importing file IS an entry point,
    // the re-export is for external consumption and doesn't prove internal usage.
    let is_re_exported_from_non_entry = source_mod.is_some_and(|m| {
        m.exports
            .iter()
            .any(|e| e.local_name.as_deref() == Some(local_name))
    }) && !entry_point_ids.contains(&source_id);

    // For entry point files with no member accesses, the namespace
    // is purely re-exported for external use — don't mark all exports
    // as used internally. The `export *` path handles individual tracking.
    let is_entry_with_no_access =
        accessed_members.is_empty() && !is_whole_object && entry_point_ids.contains(&source_id);

    if is_whole_object
        || (!is_entry_with_no_access
            && (accessed_members.is_empty() || is_re_exported_from_non_entry))
    {
        // Can't narrow — mark all exports as referenced (conservative)
        mark_all_exports_referenced(
            &mut module.exports,
            source_id,
            sym_import_span,
            &ReferenceKind::NamespaceImport,
        );
    } else {
        // Narrow: only mark accessed members as referenced
        let found_members = mark_member_exports_referenced(
            &mut module.exports,
            source_id,
            &accessed_members,
            sym_import_span,
            &ReferenceKind::NamespaceImport,
        );

        // For members not found on the target (e.g., barrel with
        // `export *` that has no own exports for these names),
        // create synthetic ExportSymbol entries so that
        // resolve_re_export_chains can propagate them to the
        // actual source modules.
        create_synthetic_exports_for_star_re_exports(
            &mut module.exports,
            &module.re_exports,
            source_id,
            &accessed_members,
            &found_members,
            sym_import_span,
        );
    }
}

/// Handle CSS Module default-import narrowing.
///
/// `import styles from './Button.module.css'` — member accesses like `styles.primary`
/// mark the `primary` named export as referenced, since CSS module default imports act
/// as namespace objects where each property corresponds to a class name (named export).
fn narrow_css_module_references(
    exports: &mut Vec<ExportSymbol>,
    source_id: FileId,
    sym_local_name: &str,
    sym_import_span: oxc_span::Span,
    module_by_id: &FxHashMap<FileId, &ResolvedModule>,
) {
    let local_name = sym_local_name;
    let source_mod = module_by_id.get(&source_id);
    let is_whole_object =
        source_mod.is_some_and(|m| m.whole_object_uses.iter().any(|n| n == local_name));
    let accessed_members = extract_accessed_members(source_mod, local_name);

    if is_whole_object || accessed_members.is_empty() {
        mark_all_exports_referenced(
            exports,
            source_id,
            sym_import_span,
            &ReferenceKind::DefaultImport,
        );
    } else {
        mark_member_exports_referenced(
            exports,
            source_id,
            &accessed_members,
            sym_import_span,
            &ReferenceKind::DefaultImport,
        );
    }
}

impl ModuleGraph {
    /// Build flat edge storage from resolved modules.
    ///
    /// Creates `ModuleNode` entries, flat `Edge` storage, reverse dependency
    /// indices, package usage maps, and the namespace-imported bitset.
    pub(super) fn populate_edges(
        files: &[DiscoveredFile],
        module_by_id: &FxHashMap<FileId, &ResolvedModule>,
        entry_point_ids: &FxHashSet<FileId>,
        module_count: usize,
        total_capacity: usize,
    ) -> Self {
        let mut all_edges = Vec::new();
        let mut modules = Vec::with_capacity(module_count);
        let mut package_usage: FxHashMap<String, Vec<FileId>> = FxHashMap::default();
        let mut type_only_package_usage: FxHashMap<String, Vec<FileId>> = FxHashMap::default();
        let mut reverse_deps = vec![Vec::new(); total_capacity];
        let mut namespace_imported = fixedbitset::FixedBitSet::with_capacity(total_capacity);

        for file in files {
            let edge_start = all_edges.len();

            if let Some(resolved) = module_by_id.get(&file.id) {
                // Group imports by target
                let mut edges_by_target: FxHashMap<FileId, Vec<ImportedSymbol>> =
                    FxHashMap::default();

                for import in &resolved.resolved_imports {
                    match &import.target {
                        ResolveResult::InternalModule(target_id) => {
                            // Track namespace imports during edge creation
                            if matches!(import.info.imported_name, ImportedName::Namespace) {
                                record_namespace_import(
                                    *target_id,
                                    &mut namespace_imported,
                                    total_capacity,
                                );
                            }
                            edges_by_target
                                .entry(*target_id)
                                .or_default()
                                .push(ImportedSymbol {
                                    imported_name: import.info.imported_name.clone(),
                                    local_name: import.info.local_name.clone(),
                                    import_span: import.info.span,
                                });
                        }
                        ResolveResult::NpmPackage(name) => {
                            record_package_usage(
                                name,
                                file.id,
                                &mut package_usage,
                                &mut type_only_package_usage,
                                import.info.is_type_only,
                            );
                        }
                        _ => {}
                    }
                }

                // Re-exports also create edges
                for re_export in &resolved.re_exports {
                    if let ResolveResult::InternalModule(target_id) = &re_export.target {
                        // ALL re-exports use SideEffect edges to avoid marking source
                        // exports as "used" just because they're re-exported. The
                        // re-export chain propagation handles tracking which specific
                        // names consumers actually import.
                        edges_by_target
                            .entry(*target_id)
                            .or_default()
                            .push(ImportedSymbol {
                                imported_name: ImportedName::SideEffect,
                                local_name: String::new(),
                                import_span: oxc_span::Span::new(0, 0),
                            });
                    } else if let ResolveResult::NpmPackage(name) = &re_export.target {
                        record_package_usage(
                            name,
                            file.id,
                            &mut package_usage,
                            &mut type_only_package_usage,
                            re_export.info.is_type_only,
                        );
                    }
                }

                // Dynamic imports — use the imported_name/local_name from resolution.
                // Named imports (`const { foo } = await import('./x')`) create Named edges.
                // Namespace imports (`const mod = await import('./x')`) create Namespace edges
                // with a local_name, enabling member access narrowing.
                // Side-effect imports (`await import('./x')`) create SideEffect edges.
                for import in &resolved.resolved_dynamic_imports {
                    if let ResolveResult::InternalModule(target_id) = &import.target {
                        if matches!(import.info.imported_name, ImportedName::Namespace) {
                            record_namespace_import(
                                *target_id,
                                &mut namespace_imported,
                                total_capacity,
                            );
                        }
                        edges_by_target
                            .entry(*target_id)
                            .or_default()
                            .push(ImportedSymbol {
                                imported_name: import.info.imported_name.clone(),
                                local_name: import.info.local_name.clone(),
                                import_span: import.info.span,
                            });
                    }
                }

                // Dynamic import patterns (template literals, string concat, import.meta.glob)
                for (_pattern, matched_ids) in &resolved.resolved_dynamic_patterns {
                    for target_id in matched_ids {
                        record_namespace_import(
                            *target_id,
                            &mut namespace_imported,
                            total_capacity,
                        );
                        edges_by_target
                            .entry(*target_id)
                            .or_default()
                            .push(ImportedSymbol {
                                imported_name: ImportedName::Namespace,
                                local_name: String::new(),
                                import_span: oxc_span::Span::new(0, 0),
                            });
                    }
                }

                // Sort by target FileId for deterministic edge order across runs
                let mut sorted_edges: Vec<_> = edges_by_target.into_iter().collect();
                sorted_edges.sort_by_key(|(target_id, _)| target_id.0);

                for (target_id, symbols) in sorted_edges {
                    all_edges.push(Edge {
                        source: file.id,
                        target: target_id,
                        symbols,
                    });

                    if (target_id.0 as usize) < reverse_deps.len() {
                        reverse_deps[target_id.0 as usize].push(file.id);
                    }
                }
            }

            let edge_end = all_edges.len();

            modules.push(build_module_node(
                file,
                module_by_id,
                entry_point_ids,
                edge_start..edge_end,
            ));
        }

        Self {
            modules,
            edges: all_edges,
            package_usage,
            type_only_package_usage,
            entry_points: entry_point_ids.clone(),
            reverse_deps,
            namespace_imported,
        }
    }

    /// Record which files reference which exports from edges.
    ///
    /// Walks every edge and attaches `SymbolReference` entries to the target
    /// module's exports. Includes namespace import narrowing (member access
    /// tracking) and CSS Module default-import narrowing.
    pub(super) fn populate_references(
        &mut self,
        module_by_id: &FxHashMap<FileId, &ResolvedModule>,
        entry_point_ids: &FxHashSet<FileId>,
    ) {
        for edge_idx in 0..self.edges.len() {
            let source_id = self.edges[edge_idx].source;
            let target_idx = self.edges[edge_idx].target.0 as usize;
            if target_idx >= self.modules.len() {
                continue;
            }
            for sym_idx in 0..self.edges[edge_idx].symbols.len() {
                let sym_imported_name = self.edges[edge_idx].symbols[sym_idx].imported_name.clone();
                let sym_local_name = self.edges[edge_idx].symbols[sym_idx].local_name.clone();
                let sym_import_span = self.edges[edge_idx].symbols[sym_idx].import_span;

                let ref_kind = match &sym_imported_name {
                    ImportedName::Named(_) => ReferenceKind::NamedImport,
                    ImportedName::Default => ReferenceKind::DefaultImport,
                    ImportedName::Namespace => ReferenceKind::NamespaceImport,
                    ImportedName::SideEffect => ReferenceKind::SideEffectImport,
                };

                // Skip references for import bindings that are never used in the
                // importing file.
                if is_unused_import_binding(
                    &sym_local_name,
                    &sym_imported_name,
                    module_by_id.get(&source_id),
                ) {
                    continue;
                }

                let target_module = &mut self.modules[target_idx];

                // Match to specific export
                if let Some(export) = target_module
                    .exports
                    .iter_mut()
                    .find(|e| export_matches(&e.name, &sym_imported_name))
                {
                    export.references.push(SymbolReference {
                        from_file: source_id,
                        kind: ref_kind,
                        import_span: sym_import_span,
                    });
                }

                // Namespace imports: check if we can narrow to specific member accesses.
                // `import * as ns from './x'; ns.foo; ns.bar` → only mark foo, bar as used.
                // If the namespace variable is re-exported (`export { ns }`) or no member
                // accesses are found, conservatively mark ALL exports as used.
                if matches!(sym_imported_name, ImportedName::Namespace)
                    && !sym_local_name.is_empty()
                {
                    narrow_namespace_references(
                        &mut self.modules[target_idx],
                        source_id,
                        &sym_local_name,
                        sym_import_span,
                        module_by_id,
                        entry_point_ids,
                    );
                } else if matches!(sym_imported_name, ImportedName::Namespace) {
                    // No local name available — mark all (conservative)
                    mark_all_exports_referenced(
                        &mut self.modules[target_idx].exports,
                        source_id,
                        sym_import_span,
                        &ReferenceKind::NamespaceImport,
                    );
                }

                // CSS Module default imports: `import styles from './Button.module.css'`
                // Member accesses like `styles.primary` should mark the `primary` named
                // export as referenced, since CSS module default imports act as namespace
                // objects where each property corresponds to a class name (named export).
                if matches!(sym_imported_name, ImportedName::Default)
                    && !sym_local_name.is_empty()
                    && is_css_module_path(&self.modules[target_idx].path)
                {
                    narrow_css_module_references(
                        &mut self.modules[target_idx].exports,
                        source_id,
                        &sym_local_name,
                        sym_import_span,
                        module_by_id,
                    );
                }
            }
        }
    }
}

/// Check if a path is a CSS Module file (`.module.css` or `.module.scss`).
pub(super) fn is_css_module_path(path: &std::path::Path) -> bool {
    path.file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|stem| stem.ends_with(".module"))
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext == "css" || ext == "scss")
}

/// Check if an export name matches an imported name.
pub(super) fn export_matches(export: &ExportName, import: &ImportedName) -> bool {
    match (export, import) {
        (ExportName::Named(e), ImportedName::Named(i)) => e == i,
        (ExportName::Default, ImportedName::Default) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_matches_named_same() {
        assert!(export_matches(
            &ExportName::Named("foo".to_string()),
            &ImportedName::Named("foo".to_string())
        ));
    }

    #[test]
    fn export_matches_named_different() {
        assert!(!export_matches(
            &ExportName::Named("foo".to_string()),
            &ImportedName::Named("bar".to_string())
        ));
    }

    #[test]
    fn export_matches_default() {
        assert!(export_matches(&ExportName::Default, &ImportedName::Default));
    }

    #[test]
    fn export_matches_named_vs_default() {
        assert!(!export_matches(
            &ExportName::Named("foo".to_string()),
            &ImportedName::Default
        ));
    }

    #[test]
    fn export_matches_default_vs_named() {
        assert!(!export_matches(
            &ExportName::Default,
            &ImportedName::Named("foo".to_string())
        ));
    }

    #[test]
    fn export_matches_namespace_no_match() {
        assert!(!export_matches(
            &ExportName::Named("foo".to_string()),
            &ImportedName::Namespace
        ));
        assert!(!export_matches(
            &ExportName::Default,
            &ImportedName::Namespace
        ));
    }

    #[test]
    fn export_matches_side_effect_no_match() {
        assert!(!export_matches(
            &ExportName::Named("foo".to_string()),
            &ImportedName::SideEffect
        ));
    }
}
