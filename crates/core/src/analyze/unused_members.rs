use rustc_hash::{FxHashMap, FxHashSet};

use crate::discover::FileId;
use crate::extract::MemberKind;
use crate::graph::ModuleGraph;
use crate::resolve::ResolvedModule;
use crate::results::*;
use crate::suppress::{self, IssueKind, Suppression};

use super::predicates::{is_angular_lifecycle_method, is_react_lifecycle_method};
use super::{LineOffsetsMap, byte_offset_to_line_col};

/// Find unused enum and class members in exported symbols.
///
/// Collects all `Identifier.member` static member accesses from all modules,
/// maps them to their imported names, and filters out members that are accessed.
pub fn find_unused_members(
    graph: &ModuleGraph,
    resolved_modules: &[ResolvedModule],
    suppressions_by_file: &FxHashMap<FileId, &[Suppression]>,
    line_offsets_by_file: &LineOffsetsMap<'_>,
) -> (Vec<UnusedMember>, Vec<UnusedMember>) {
    let mut unused_enum_members = Vec::new();
    let mut unused_class_members = Vec::new();

    // Map export_name -> set of member_names that are accessed across all modules.
    // We map local import names back to the original imported names.
    let mut accessed_members: FxHashMap<String, FxHashSet<String>> = FxHashMap::default();

    // Also build a per-file set of `this.member` accesses. These indicate internal usage
    // within a class body — class members accessed via `this.foo` are used internally
    // even if no external code accesses them via `ClassName.foo`.
    let mut self_accessed_members: FxHashMap<crate::discover::FileId, FxHashSet<String>> =
        FxHashMap::default();

    // Build a set of export names that are used as whole objects (Object.values, for..in, etc.).
    // All members of these exports should be considered used.
    let mut whole_object_used_exports: FxHashSet<String> = FxHashSet::default();

    for resolved in resolved_modules {
        // Build a map from local name -> imported name for this module's imports
        let local_to_imported: FxHashMap<&str, &str> = resolved
            .resolved_imports
            .iter()
            .filter_map(|imp| match &imp.info.imported_name {
                crate::extract::ImportedName::Named(name) => {
                    Some((imp.info.local_name.as_str(), name.as_str()))
                }
                crate::extract::ImportedName::Default => {
                    Some((imp.info.local_name.as_str(), "default"))
                }
                _ => None,
            })
            .collect();

        for access in &resolved.member_accesses {
            // Track `this.member` accesses per-file for internal class usage
            if access.object == "this" {
                self_accessed_members
                    .entry(resolved.file_id)
                    .or_default()
                    .insert(access.member.clone());
                continue;
            }
            // If the object is a local name for an import, map it to the original export name
            let export_name = local_to_imported
                .get(access.object.as_str())
                .copied()
                .unwrap_or(access.object.as_str());
            accessed_members
                .entry(export_name.to_string())
                .or_default()
                .insert(access.member.clone());
        }

        // Map whole-object uses from local names to imported names
        for local_name in &resolved.whole_object_uses {
            let export_name = local_to_imported
                .get(local_name.as_str())
                .copied()
                .unwrap_or(local_name.as_str());
            whole_object_used_exports.insert(export_name.to_string());
        }
    }

    for module in &graph.modules {
        if !module.is_reachable || module.is_entry_point {
            continue;
        }

        for export in &module.exports {
            if export.members.is_empty() {
                continue;
            }

            // If the export itself is unused, skip member analysis (whole export is dead)
            if export.references.is_empty() && !graph.has_namespace_import(module.file_id) {
                continue;
            }

            let export_name = export.name.to_string();

            // If this export is used as a whole object (Object.values, for..in, etc.),
            // all members are considered used — skip individual member analysis.
            if whole_object_used_exports.contains(&export_name) {
                continue;
            }

            // Get `this.member` accesses from this file (internal class usage)
            let file_self_accesses = self_accessed_members.get(&module.file_id);

            for member in &export.members {
                // Check if this member is accessed anywhere via external import
                if accessed_members
                    .get(&export_name)
                    .is_some_and(|s| s.contains(&member.name))
                {
                    continue;
                }

                // Check if this member is accessed via `this.member` within the same file
                // (internal class usage — e.g., constructor sets this.label, methods use this.label)
                if matches!(
                    member.kind,
                    MemberKind::ClassMethod | MemberKind::ClassProperty
                ) && file_self_accesses.is_some_and(|accesses| accesses.contains(&member.name))
                {
                    continue;
                }

                // Skip decorated class members — decorators like @Column(), @ApiProperty(),
                // @Inject() etc. indicate runtime usage by frameworks (NestJS, TypeORM,
                // class-validator, class-transformer). These members are accessed
                // reflectively and should never be flagged as unused.
                if member.has_decorator {
                    continue;
                }

                // Skip React class component lifecycle methods — they are called by the
                // React runtime, not user code, so they should never be flagged as unused.
                // Also skip Angular lifecycle hooks (OnInit, OnDestroy, etc.).
                if matches!(
                    member.kind,
                    MemberKind::ClassMethod | MemberKind::ClassProperty
                ) && (is_react_lifecycle_method(&member.name)
                    || is_angular_lifecycle_method(&member.name))
                {
                    continue;
                }

                let (line, col) = byte_offset_to_line_col(
                    line_offsets_by_file,
                    module.file_id,
                    member.span.start,
                );

                // Check inline suppression
                let issue_kind = match member.kind {
                    MemberKind::EnumMember => IssueKind::UnusedEnumMember,
                    MemberKind::ClassMethod | MemberKind::ClassProperty => {
                        IssueKind::UnusedClassMember
                    }
                };
                if let Some(supps) = suppressions_by_file.get(&module.file_id)
                    && suppress::is_suppressed(supps, line, issue_kind)
                {
                    continue;
                }

                let unused = UnusedMember {
                    path: module.path.clone(),
                    parent_name: export_name.clone(),
                    member_name: member.name.clone(),
                    kind: member.kind.clone(),
                    line,
                    col,
                };

                match member.kind {
                    MemberKind::EnumMember => unused_enum_members.push(unused),
                    MemberKind::ClassMethod | MemberKind::ClassProperty => {
                        unused_class_members.push(unused);
                    }
                }
            }
        }
    }

    (unused_enum_members, unused_class_members)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{DiscoveredFile, EntryPoint, EntryPointSource, FileId};
    use crate::extract::{
        ExportName, ImportInfo, ImportedName, MemberAccess, MemberInfo, MemberKind,
    };
    use crate::graph::{ExportSymbol, ModuleGraph, SymbolReference};
    use crate::resolve::{ResolveResult, ResolvedImport, ResolvedModule};
    use oxc_span::Span;
    use std::path::PathBuf;

    fn build_graph(file_specs: &[(&str, bool)]) -> ModuleGraph {
        let files: Vec<DiscoveredFile> = file_specs
            .iter()
            .enumerate()
            .map(|(i, (path, _))| DiscoveredFile {
                id: FileId(i as u32),
                path: PathBuf::from(path),
                size_bytes: 0,
            })
            .collect();

        let entry_points: Vec<EntryPoint> = file_specs
            .iter()
            .filter(|(_, is_entry)| *is_entry)
            .map(|(path, _)| EntryPoint {
                path: PathBuf::from(path),
                source: EntryPointSource::ManualEntry,
            })
            .collect();

        let resolved_modules: Vec<ResolvedModule> = files
            .iter()
            .map(|f| ResolvedModule {
                file_id: f.id,
                path: f.path.clone(),
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: FxHashSet::default(),
            })
            .collect();

        ModuleGraph::build(&resolved_modules, &entry_points, &files)
    }

    fn make_member(name: &str, kind: MemberKind) -> MemberInfo {
        MemberInfo {
            name: name.to_string(),
            kind,
            span: Span::new(10, 20),
            has_decorator: false,
        }
    }

    fn make_export_with_members(
        name: &str,
        members: Vec<MemberInfo>,
        ref_from: Option<u32>,
    ) -> ExportSymbol {
        let references = ref_from
            .map(|from| {
                vec![SymbolReference {
                    from_file: FileId(from),
                    kind: crate::graph::ReferenceKind::NamedImport,
                    import_span: Span::new(0, 10),
                }]
            })
            .unwrap_or_default();
        ExportSymbol {
            name: ExportName::Named(name.to_string()),
            is_type_only: false,
            is_public: false,
            span: Span::new(0, 10),
            references,
            members,
        }
    }

    #[test]
    fn unused_members_empty_graph() {
        let graph = build_graph(&[]);

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn unused_enum_member_detected() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0), // referenced from entry
        )];

        // No member accesses at all — both should be unused
        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(enum_members.len(), 2);
        assert!(class_members.is_empty());
        let names: FxHashSet<&str> = enum_members
            .iter()
            .map(|m| m.member_name.as_str())
            .collect();
        assert!(names.contains("Active"));
        assert!(names.contains("Inactive"));
    }

    #[test]
    fn accessed_enum_member_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // Consumer accesses Status.Active
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "Status".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                object: "Status".to_string(),
                member: "Active".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // Only Inactive should be unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn whole_object_use_skips_all_members() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // Consumer uses Object.values(Status) — whole object use
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "Status".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec!["Status".to_string()],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn decorated_class_member_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/entity.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "User",
            vec![MemberInfo {
                name: "name".to_string(),
                kind: MemberKind::ClassProperty,
                span: Span::new(10, 20),
                has_decorator: true, // @Column() etc.
            }],
            Some(0),
        )];

        let (_, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(class_members.is_empty());
    }

    #[test]
    fn react_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "MyComponent",
            vec![
                make_member("render", MemberKind::ClassMethod),
                make_member("componentDidMount", MemberKind::ClassMethod),
                make_member("customMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        // Only customMethod should be flagged
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "customMethod");
    }

    #[test]
    fn angular_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "AppComponent",
            vec![
                make_member("ngOnInit", MemberKind::ClassMethod),
                make_member("ngOnDestroy", MemberKind::ClassMethod),
                make_member("myHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "myHelper");
    }

    #[test]
    fn this_member_access_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Service",
            vec![
                make_member("label", MemberKind::ClassProperty),
                make_member("unused_prop", MemberKind::ClassProperty),
            ],
            Some(0),
        )];

        // The service file itself accesses this.label
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(1), // same file as the service
            path: PathBuf::from("/src/service.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "label".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // Only unused_prop should be flagged (label is accessed via this)
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unused_prop");
    }

    #[test]
    fn unreferenced_export_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        // Export has members but NO references — whole export is dead, members skipped
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            None, // no references
        )];

        let (enum_members, _) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        // Member analysis skipped because export itself is unreferenced
        assert!(enum_members.is_empty());
    }

    #[test]
    fn unreachable_module_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/dead.ts", false)]);
        // Module 1 stays unreachable
        graph.modules[1].exports = vec![make_export_with_members(
            "DeadEnum",
            vec![make_member("X", MemberKind::EnumMember)],
            Some(0),
        )];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn entry_point_module_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true)]);
        graph.modules[0].exports = vec![make_export_with_members(
            "EntryEnum",
            vec![make_member("X", MemberKind::EnumMember)],
            None,
        )];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn enum_member_kind_routed_to_enum_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].kind, MemberKind::EnumMember);
        assert!(class_members.is_empty());
    }

    #[test]
    fn class_member_kind_routed_to_class_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/class.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "MyClass",
            vec![
                make_member("myMethod", MemberKind::ClassMethod),
                make_member("myProp", MemberKind::ClassProperty),
            ],
            Some(0),
        )];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(enum_members.is_empty());
        assert_eq!(class_members.len(), 2);
        assert!(
            class_members
                .iter()
                .any(|m| m.kind == MemberKind::ClassMethod)
        );
        assert!(
            class_members
                .iter()
                .any(|m| m.kind == MemberKind::ClassProperty)
        );
    }

    #[test]
    fn instance_member_access_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "MyService",
            vec![
                make_member("greet", MemberKind::ClassMethod),
                make_member("unusedMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        // Consumer imports MyService and accesses greet via instance.
        // The visitor maps `svc.greet()` → `MyService.greet` at extraction time,
        // so the analysis layer sees it as a direct member access on the export name.
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./service".to_string(),
                    imported_name: ImportedName::Named("MyService".to_string()),
                    local_name: "MyService".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                // Already mapped by the visitor from `svc.greet()` → `MyService.greet`
                object: "MyService".to_string(),
                member: "greet".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // Only unusedMethod should be flagged; greet is used via instance access
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unusedMethod");
    }

    #[test]
    fn this_access_does_not_skip_enum_members() {
        // `this.member` accesses only suppress class members, not enum members.
        // Enums don't have `this` — this test ensures the check is scoped to class kinds.
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Direction",
            vec![
                make_member("Up", MemberKind::EnumMember),
                make_member("Down", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        // File accesses this.Up — but for enum members, this should NOT suppress
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(1),
            path: PathBuf::from("/src/enums.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "Up".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // Both enum members should be flagged — `this` access doesn't apply to enums
        assert_eq!(enum_members.len(), 2);
    }

    #[test]
    fn mixed_enum_and_class_in_same_module() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/mixed.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![
            make_export_with_members(
                "Status",
                vec![make_member("Active", MemberKind::EnumMember)],
                Some(0),
            ),
            make_export_with_members(
                "Service",
                vec![make_member("doWork", MemberKind::ClassMethod)],
                Some(0),
            ),
        ];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].parent_name, "Status");
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].parent_name, "Service");
    }

    #[test]
    fn local_name_mapped_to_imported_name() {
        // import { Status as S } from './enums'
        // S.Active → should map "S" back to "Status" for member access matching
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "S".to_string(), // aliased
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                object: "S".to_string(), // uses local alias
                member: "Active".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // S.Active maps back to Status.Active, so only Inactive is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn default_import_maps_to_default_export() {
        // import MyEnum from './enums' → local "MyEnum", imported "default"
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "default",
            vec![
                make_member("X", MemberKind::EnumMember),
                make_member("Y", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Default,
                    local_name: "MyEnum".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![MemberAccess {
                object: "MyEnum".to_string(),
                member: "X".to_string(),
            }],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // MyEnum.X maps to default.X, so only Y is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Y");
    }

    #[test]
    fn suppressed_enum_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        // Suppress on line 1 (byte offset 10 => line 1 with no offsets)
        let supps = vec![Suppression {
            line: 1,
            kind: Some(IssueKind::UnusedEnumMember),
        }];
        let mut suppressions: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        suppressions.insert(FileId(1), &supps);

        let (enum_members, _) =
            find_unused_members(&graph, &[], &suppressions, &FxHashMap::default());
        assert!(
            enum_members.is_empty(),
            "suppressed enum member should not be flagged"
        );
    }

    #[test]
    fn suppressed_class_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Service",
            vec![make_member("doWork", MemberKind::ClassMethod)],
            Some(0),
        )];

        let supps = vec![Suppression {
            line: 1,
            kind: Some(IssueKind::UnusedClassMember),
        }];
        let mut suppressions: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        suppressions.insert(FileId(1), &supps);

        let (_, class_members) =
            find_unused_members(&graph, &[], &suppressions, &FxHashMap::default());
        assert!(
            class_members.is_empty(),
            "suppressed class member should not be flagged"
        );
    }

    #[test]
    fn whole_object_use_via_aliased_import() {
        // import { Status as S } from './enums'
        // Object.values(S) → should map S back to Status and suppress all members
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("A", MemberKind::EnumMember),
                make_member("B", MemberKind::EnumMember),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![ResolvedImport {
                info: ImportInfo {
                    source: "./enums".to_string(),
                    imported_name: ImportedName::Named("Status".to_string()),
                    local_name: "S".to_string(),
                    is_type_only: false,
                    span: Span::new(0, 30),
                    source_span: Span::default(),
                },
                target: ResolveResult::InternalModule(FileId(1)),
            }],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec!["S".to_string()], // aliased local name
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &FxHashMap::default(),
            &FxHashMap::default(),
        );
        // Object.values(S) maps S→Status, so all members of Status should be considered used
        assert!(
            enum_members.is_empty(),
            "whole object use via alias should suppress all members"
        );
    }

    #[test]
    fn export_with_no_members_skipped() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/utils.ts", false)]);
        graph.modules[1].is_reachable = true;
        graph.modules[1].exports = vec![make_export_with_members(
            "helper",
            vec![], // no members
            Some(0),
        )];

        let (enum_members, class_members) =
            find_unused_members(&graph, &[], &FxHashMap::default(), &FxHashMap::default());
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }
}
