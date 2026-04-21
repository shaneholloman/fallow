use fallow_config::{ScopedUsedClassMemberRule, UsedClassMemberRule};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::discover::FileId;
use crate::extract::{ANGULAR_TPL_SENTINEL, MemberKind, ModuleInfo};
use crate::graph::ModuleGraph;
use crate::resolve::{ResolveResult, ResolvedModule};
use crate::results::UnusedMember;
use crate::suppress::{IssueKind, SuppressionContext};

use super::predicates::{is_angular_lifecycle_method, is_react_lifecycle_method};
use super::{LineOffsetsMap, byte_offset_to_line_col};

/// Find unused enum and class members in exported symbols.
///
/// Collects all `Identifier.member` static member accesses from all modules,
/// maps them to their imported names, and filters out members that are accessed.
///
/// `user_class_member_allowlist` extends the built-in Angular/React lifecycle
/// allowlist with framework-invoked method names contributed by plugins and
/// top-level config (see `FallowConfig::used_class_members` and
/// `Plugin::used_class_members`). Plain string entries suppress matching member
/// names globally; scoped object entries only suppress classes whose heritage
/// clause matches the configured `extends` / `implements` constraints.
#[derive(Default)]
struct ClassMemberAllowlist<'a> {
    global: FxHashSet<&'a str>,
    scoped: FxHashMap<&'a str, Vec<&'a ScopedUsedClassMemberRule>>,
}

impl<'a> ClassMemberAllowlist<'a> {
    fn from_rules(rules: &'a [UsedClassMemberRule]) -> Self {
        let mut allowlist = Self::default();
        for rule in rules {
            match rule {
                UsedClassMemberRule::Name(name) => {
                    allowlist.global.insert(name.as_str());
                }
                UsedClassMemberRule::Scoped(rule) => {
                    for member in &rule.members {
                        allowlist
                            .scoped
                            .entry(member.as_str())
                            .or_default()
                            .push(rule);
                    }
                }
            }
        }
        allowlist
    }

    fn matches(
        &self,
        member_name: &str,
        super_class: Option<&str>,
        implemented_interfaces: &[String],
    ) -> bool {
        self.global.contains(member_name)
            || self.scoped.get(member_name).is_some_and(|rules| {
                rules
                    .iter()
                    .any(|rule| rule.matches_heritage(super_class, implemented_interfaces))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ExportKey {
    file_id: FileId,
    export_name: String,
}

impl ExportKey {
    fn new(file_id: FileId, export_name: impl Into<String>) -> Self {
        Self {
            file_id,
            export_name: export_name.into(),
        }
    }
}

fn imported_export_name(imported_name: &crate::extract::ImportedName) -> Option<&str> {
    match imported_name {
        crate::extract::ImportedName::Named(name) => Some(name.as_str()),
        crate::extract::ImportedName::Default => Some("default"),
        crate::extract::ImportedName::Namespace | crate::extract::ImportedName::SideEffect => None,
    }
}

fn push_local_export_key<'a>(
    local_to_export_keys: &mut FxHashMap<&'a str, Vec<ExportKey>>,
    local_name: &'a str,
    export_key: ExportKey,
) {
    let entry = local_to_export_keys.entry(local_name).or_default();
    if !entry.contains(&export_key) {
        entry.push(export_key);
    }
}

fn build_local_to_export_keys(resolved: &ResolvedModule) -> FxHashMap<&str, Vec<ExportKey>> {
    let mut local_to_export_keys = FxHashMap::default();

    for import in &resolved.resolved_imports {
        let Some(imported_name) = imported_export_name(&import.info.imported_name) else {
            continue;
        };
        let ResolveResult::InternalModule(target_file_id) = &import.target else {
            continue;
        };
        push_local_export_key(
            &mut local_to_export_keys,
            import.info.local_name.as_str(),
            ExportKey::new(*target_file_id, imported_name),
        );
    }

    for export in &resolved.exports {
        if let Some(local_name) = export.local_name.as_deref() {
            push_local_export_key(
                &mut local_to_export_keys,
                local_name,
                ExportKey::new(resolved.file_id, export.name.to_string()),
            );
        }
    }

    local_to_export_keys
}

#[expect(
    clippy::too_many_lines,
    reason = "member tracking requires many graph traversal steps; split candidate for sig-audit-loop"
)]
pub fn find_unused_members(
    graph: &ModuleGraph,
    resolved_modules: &[ResolvedModule],
    modules: &[ModuleInfo],
    suppressions: &SuppressionContext<'_>,
    line_offsets_by_file: &LineOffsetsMap<'_>,
    user_class_member_allowlist: &[UsedClassMemberRule],
) -> (Vec<UnusedMember>, Vec<UnusedMember>) {
    let mut unused_enum_members = Vec::new();
    let mut unused_class_members = Vec::new();
    let allowlist = ClassMemberAllowlist::from_rules(user_class_member_allowlist);

    let mut class_heritage_by_export: FxHashMap<ExportKey, (Option<String>, Vec<String>)> =
        FxHashMap::default();
    let mut class_heritage_by_file = FxHashMap::default();
    for module in modules {
        class_heritage_by_file.insert(module.file_id, module.class_heritage.as_slice());
        class_heritage_by_export.extend(module.class_heritage.iter().map(|heritage| {
            (
                ExportKey::new(module.file_id, heritage.export_name.clone()),
                (heritage.super_class.clone(), heritage.implements.clone()),
            )
        }));
    }

    let mut interface_to_implementers: FxHashMap<ExportKey, Vec<ExportKey>> = FxHashMap::default();
    for resolved in resolved_modules {
        let Some(class_heritage) = class_heritage_by_file.get(&resolved.file_id) else {
            continue;
        };
        if class_heritage.is_empty() {
            continue;
        }

        let local_to_export_keys = build_local_to_export_keys(resolved);
        for heritage in *class_heritage {
            if heritage.implements.is_empty() {
                continue;
            }

            let implementer_key = ExportKey::new(resolved.file_id, heritage.export_name.clone());
            for interface_name in &heritage.implements {
                let Some(interface_keys) = local_to_export_keys.get(interface_name.as_str()) else {
                    continue;
                };
                for interface_key in interface_keys {
                    let implementers = interface_to_implementers
                        .entry(interface_key.clone())
                        .or_default();
                    if !implementers.contains(&implementer_key) {
                        implementers.push(implementer_key.clone());
                    }
                }
            }
        }
    }

    // Map exported symbol identity -> set of member names that are accessed across all modules.
    let mut accessed_members: FxHashMap<ExportKey, FxHashSet<String>> = FxHashMap::default();

    // Also build a per-file set of `this.member` accesses. These indicate internal usage
    // within a class body — class members accessed via `this.foo` are used internally
    // even if no external code accesses them via `ClassName.foo`.
    let mut self_accessed_members: FxHashMap<crate::discover::FileId, FxHashSet<String>> =
        FxHashMap::default();

    // Build a set of exported symbols that are used as whole objects
    // (Object.values, for..in, etc.). All members of these exports should be
    // considered used.
    let mut whole_object_used_exports: FxHashSet<ExportKey> = FxHashSet::default();

    for resolved in resolved_modules {
        let local_to_export_keys = build_local_to_export_keys(resolved);

        for access in &resolved.member_accesses {
            // Track `this.member` accesses per-file for internal class usage
            if access.object == "this" {
                self_accessed_members
                    .entry(resolved.file_id)
                    .or_default()
                    .insert(access.member.clone());
                continue;
            }

            if let Some(export_keys) = local_to_export_keys.get(access.object.as_str()) {
                for export_key in export_keys {
                    accessed_members
                        .entry(export_key.clone())
                        .or_default()
                        .insert(access.member.clone());
                }
            }
        }

        for local_name in &resolved.whole_object_uses {
            if let Some(export_keys) = local_to_export_keys.get(local_name.as_str()) {
                whole_object_used_exports.extend(export_keys.iter().cloned());
            }
        }
    }

    if !interface_to_implementers.is_empty() {
        let mut propagations: Vec<(ExportKey, Vec<String>)> = Vec::new();

        for (interface_key, implementer_keys) in &interface_to_implementers {
            let Some(interface_accesses) = accessed_members.get(interface_key) else {
                continue;
            };
            let accesses: Vec<String> = interface_accesses.iter().cloned().collect();
            for implementer_key in implementer_keys {
                propagations.push((implementer_key.clone(), accesses.clone()));
            }
        }

        for (implementer_key, accesses) in propagations {
            accessed_members
                .entry(implementer_key)
                .or_default()
                .extend(accesses);
        }
    }

    // ── Inheritance propagation ────────────────────────────────────────────
    //
    // Build an inheritance map from `extends` clauses, then propagate member
    // accesses through the hierarchy. This prevents false positives where:
    // - A parent class method accesses `this.member` (credits child overrides)
    // - A child class override is flagged unused when the parent method is used
    //
    // parent_to_children: BaseShape@file_a → [Circle@file_b, Rectangle@file_c]
    let mut parent_to_children: FxHashMap<ExportKey, Vec<ExportKey>> = FxHashMap::default();

    for resolved in resolved_modules {
        let local_to_export_keys = build_local_to_export_keys(resolved);

        for export in &resolved.exports {
            if let Some(super_local) = &export.super_class {
                let Some(parent_keys) = local_to_export_keys.get(super_local.as_str()) else {
                    continue;
                };
                let child_key = ExportKey::new(resolved.file_id, export.name.to_string());

                for parent_key in parent_keys {
                    let children = parent_to_children.entry(parent_key.clone()).or_default();
                    if !children.contains(&child_key) {
                        children.push(child_key.clone());
                    }
                }
            }
        }
    }

    // Propagate `this.member` accesses from parent files to child files.
    // When BaseShape.describe() calls `this.getArea()`, that should credit
    // Circle.getArea() and Rectangle.getArea() as used.
    if !parent_to_children.is_empty() {
        // Collect propagations first to avoid borrow conflicts
        let mut propagations: Vec<(FileId, Vec<String>)> = Vec::new();

        for (parent_key, children) in &parent_to_children {
            // Propagate parent's this.* accesses to child files
            if let Some(parent_self_accesses) = self_accessed_members.get(&parent_key.file_id) {
                let accesses: Vec<String> = parent_self_accesses.iter().cloned().collect();
                for child_key in children {
                    propagations.push((child_key.file_id, accesses.clone()));
                }
            }

            // Also propagate accessed_members bidirectionally:
            // If parent's member is externally accessed, credit all children
            // If child's member is externally accessed, credit the parent
            let parent_accesses = accessed_members.get(parent_key).cloned();
            let mut child_accesses_to_propagate: FxHashSet<String> = FxHashSet::default();

            for child_key in children {
                if let Some(child_accesses) = accessed_members.get(child_key) {
                    child_accesses_to_propagate.extend(child_accesses.iter().cloned());
                }
            }

            // Parent → children
            if let Some(ref parent_acc) = parent_accesses {
                for child_key in children {
                    accessed_members
                        .entry(child_key.clone())
                        .or_default()
                        .extend(parent_acc.iter().cloned());
                }
            }

            // Children → parent
            if !child_accesses_to_propagate.is_empty() {
                accessed_members
                    .entry(parent_key.clone())
                    .or_default()
                    .extend(child_accesses_to_propagate);
            }
        }

        // Apply self_accessed_members propagations
        for (file_id, members) in propagations {
            let entry = self_accessed_members.entry(file_id).or_default();
            for member in members {
                entry.insert(member);
            }
        }
    }

    // Bridge Angular template member refs to their owning components.
    //
    // Sentinel member accesses come from two sources:
    // 1. External templates: HTML files scanned for Angular syntax, with sentinel
    //    accesses stored on the HTML file's ModuleInfo. Bridged to the component
    //    via the SideEffect import edge from @Component({ templateUrl }).
    // 2. Inline templates/host/inputs/outputs: sentinel accesses stored directly
    //    on the component's own ModuleInfo (same file as the class).
    let angular_tpl_refs: FxHashMap<FileId, Vec<&str>> = resolved_modules
        .iter()
        .filter_map(|m| {
            let refs: Vec<&str> = m
                .member_accesses
                .iter()
                .filter(|a| a.object == ANGULAR_TPL_SENTINEL)
                .map(|a| a.member.as_str())
                .collect();
            if refs.is_empty() {
                None
            } else {
                Some((m.file_id, refs))
            }
        })
        .collect();

    if !angular_tpl_refs.is_empty() {
        for resolved in resolved_modules {
            // Case 1: sentinel accesses on the same file (inline template, host, inputs/outputs)
            if let Some(refs) = angular_tpl_refs.get(&resolved.file_id) {
                let entry = self_accessed_members.entry(resolved.file_id).or_default();
                for &ref_name in refs {
                    entry.insert(ref_name.to_string());
                }
            }
            // Case 2: sentinel accesses on an imported file (external templateUrl)
            for import in &resolved.resolved_imports {
                if let ResolveResult::InternalModule(target_id) = &import.target
                    && let Some(refs) = angular_tpl_refs.get(target_id)
                {
                    let entry = self_accessed_members.entry(resolved.file_id).or_default();
                    for &ref_name in refs {
                        entry.insert(ref_name.to_string());
                    }
                }
            }
        }
    }

    for module in &graph.modules {
        if !module.is_reachable() || module.is_entry_point() {
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
            let export_key = ExportKey::new(module.file_id, export_name.clone());
            let (super_class, implemented_interfaces) = class_heritage_by_export
                .get(&export_key)
                .map_or((None, &[][..]), |(super_class, interfaces)| {
                    (super_class.as_deref(), interfaces.as_slice())
                });

            // If this export is used as a whole object (Object.values, for..in, etc.),
            // all members are considered used — skip individual member analysis.
            if whole_object_used_exports.contains(&export_key) {
                continue;
            }

            // Get `this.member` accesses from this file (internal class usage)
            let file_self_accesses = self_accessed_members.get(&module.file_id);

            for member in &export.members {
                // Skip namespace members for now — individual namespace member
                // unused detection is a future enhancement. The namespace as a
                // whole is already tracked via unused export detection.
                if matches!(member.kind, MemberKind::NamespaceMember) {
                    continue;
                }

                // Check if this member is accessed anywhere via external import
                if accessed_members
                    .get(&export_key)
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
                // The user allowlist extends these built-ins with framework-invoked names
                // contributed by plugins and top-level config (ag-Grid's `agInit`, etc.).
                if matches!(
                    member.kind,
                    MemberKind::ClassMethod | MemberKind::ClassProperty
                ) && (is_react_lifecycle_method(&member.name)
                    || is_angular_lifecycle_method(&member.name)
                    || allowlist.matches(member.name.as_str(), super_class, implemented_interfaces))
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
                    MemberKind::NamespaceMember => unreachable!(),
                };
                if suppressions.is_suppressed(module.file_id, line, issue_kind) {
                    continue;
                }

                let unused = UnusedMember {
                    path: module.path.clone(),
                    parent_name: export_name.clone(),
                    member_name: member.name.clone(),
                    kind: member.kind,
                    line,
                    col,
                };

                match member.kind {
                    MemberKind::EnumMember => unused_enum_members.push(unused),
                    MemberKind::ClassMethod | MemberKind::ClassProperty => {
                        unused_class_members.push(unused);
                    }
                    MemberKind::NamespaceMember => unreachable!(),
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
        ExportName, ImportInfo, ImportedName, MemberAccess, MemberInfo, MemberKind, ModuleInfo,
        VisibilityTag,
    };
    use crate::graph::{ExportSymbol, ModuleGraph, SymbolReference};
    use crate::resolve::{ResolveResult, ResolvedImport, ResolvedModule};
    use fallow_config::{ScopedUsedClassMemberRule, UsedClassMemberRule};
    use fallow_types::extract::ClassHeritageInfo;
    use oxc_span::Span;
    use std::path::PathBuf;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "test file counts are trivially small"
    )]
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
                ..Default::default()
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
            visibility: VisibilityTag::None,
            span: Span::new(0, 10),
            references,
            members,
        }
    }

    fn make_module_with_class_heritage(
        file_id: u32,
        export_name: &str,
        super_class: Option<&str>,
        implements: &[&str],
    ) -> ModuleInfo {
        ModuleInfo {
            file_id: FileId(file_id),
            exports: vec![],
            imports: vec![],
            re_exports: vec![],
            dynamic_imports: vec![],
            dynamic_import_patterns: vec![],
            require_calls: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            content_hash: 0,
            suppressions: vec![],
            unused_import_bindings: vec![],
            line_offsets: vec![],
            complexity: vec![],
            flag_uses: vec![],
            class_heritage: vec![ClassHeritageInfo {
                export_name: export_name.to_string(),
                super_class: super_class.map(str::to_string),
                implements: implements.iter().map(ToString::to_string).collect(),
            }],
        }
    }

    #[test]
    fn unused_members_empty_graph() {
        let graph = build_graph(&[]);

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn unused_enum_member_detected() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![
                make_member("Active", MemberKind::EnumMember),
                make_member("Inactive", MemberKind::EnumMember),
            ],
            Some(0), // referenced from entry
        )];

        // No member accesses at all — both should be unused
        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
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
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                object: "Status".to_string(),
                member: "Active".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Only Inactive should be unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn whole_object_use_skips_all_members() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
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
            whole_object_uses: vec!["Status".to_string()],
            ..Default::default()
        }];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn decorated_class_member_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/entity.ts", false)]);
        graph.modules[1].set_reachable(true);
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

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert!(class_members.is_empty());
    }

    #[test]
    fn react_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyComponent",
            vec![
                make_member("render", MemberKind::ClassMethod),
                make_member("componentDidMount", MemberKind::ClassMethod),
                make_member("customMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Only customMethod should be flagged
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "customMethod");
    }

    #[test]
    fn angular_lifecycle_method_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/component.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "AppComponent",
            vec![
                make_member("ngOnInit", MemberKind::ClassMethod),
                make_member("ngOnDestroy", MemberKind::ClassMethod),
                make_member("myHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "myHelper");
    }

    #[test]
    fn user_class_member_allowlist_not_flagged() {
        // Third-party framework contract: library calls `agInit` and `refresh`
        // on the consumer class. The user allowlist (from config or a plugin)
        // extends the built-in Angular/React lifecycle check so these names are
        // treated as always-used. See issue #98 (ag-Grid `AgFrameworkComponent`).
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/renderer.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyRendererComponent",
            vec![
                make_member("agInit", MemberKind::ClassMethod),
                make_member("refresh", MemberKind::ClassMethod),
                make_member("customHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let allowlist = vec![
            UsedClassMemberRule::from("agInit"),
            UsedClassMemberRule::from("refresh"),
        ];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &allowlist,
        );
        assert_eq!(
            class_members.len(),
            1,
            "only customHelper should remain unused"
        );
        assert_eq!(class_members[0].member_name, "customHelper");
    }

    #[test]
    fn user_class_member_allowlist_does_not_affect_enums() {
        // The allowlist is scoped to class members; matching enum member names
        // must still be flagged as unused.
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/status.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("refresh", MemberKind::EnumMember)],
            Some(0),
        )];

        let allowlist = vec![UsedClassMemberRule::from("refresh")];

        let (enum_members, _) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &allowlist,
        );
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "refresh");
    }

    #[test]
    fn scoped_allowlist_matches_implements_only() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/renderer.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyRendererComponent",
            vec![
                make_member("refresh", MemberKind::ClassMethod),
                make_member("customHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let modules = vec![make_module_with_class_heritage(
            1,
            "MyRendererComponent",
            None,
            &["ICellRendererAngularComp"],
        )];
        let allowlist = vec![UsedClassMemberRule::Scoped(ScopedUsedClassMemberRule {
            extends: None,
            implements: Some("ICellRendererAngularComp".to_string()),
            members: vec!["refresh".to_string()],
        })];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &modules,
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &allowlist,
        );

        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "customHelper");
    }

    #[test]
    fn scoped_allowlist_matches_extends_only() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/command.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "GenerateReport",
            vec![
                make_member("execute", MemberKind::ClassMethod),
                make_member("customHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let modules = vec![make_module_with_class_heritage(
            1,
            "GenerateReport",
            Some("BaseCommand"),
            &[],
        )];
        let allowlist = vec![UsedClassMemberRule::Scoped(ScopedUsedClassMemberRule {
            extends: Some("BaseCommand".to_string()),
            implements: None,
            members: vec!["execute".to_string()],
        })];

        let (_, class_members) = find_unused_members(
            &graph,
            &[],
            &modules,
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &allowlist,
        );

        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "customHelper");
    }

    #[test]
    fn this_member_access_not_flagged() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "label".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Only unused_prop should be flagged (label is accessed via this)
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unused_prop");
    }

    #[test]
    fn unreferenced_export_skips_member_analysis() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        // Export has members but NO references — whole export is dead, members skipped
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            None, // no references
        )];

        let (enum_members, _) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
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

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
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

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }

    #[test]
    fn enum_member_kind_routed_to_enum_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].kind, MemberKind::EnumMember);
        assert!(class_members.is_empty());
    }

    #[test]
    fn class_member_kind_routed_to_class_results() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/class.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyClass",
            vec![
                make_member("myMethod", MemberKind::ClassMethod),
                make_member("myProp", MemberKind::ClassProperty),
            ],
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
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
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                // Already mapped by the visitor from `svc.greet()` → `MyService.greet`
                object: "MyService".to_string(),
                member: "greet".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
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
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                object: "this".to_string(),
                member: "Up".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Both enum members should be flagged — `this` access doesn't apply to enums
        assert_eq!(enum_members.len(), 2);
    }

    #[test]
    fn mixed_enum_and_class_in_same_module() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/mixed.ts", false)]);
        graph.modules[1].set_reachable(true);
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

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
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
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                object: "S".to_string(), // uses local alias
                member: "Active".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // S.Active maps back to Status.Active, so only Inactive is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Inactive");
    }

    #[test]
    fn default_import_maps_to_default_export() {
        // import MyEnum from './enums' → local "MyEnum", imported "default"
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
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
            member_accesses: vec![MemberAccess {
                object: "MyEnum".to_string(),
                member: "X".to_string(),
            }],
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // MyEnum.X maps to default.X, so only Y is unused
        assert_eq!(enum_members.len(), 1);
        assert_eq!(enum_members[0].member_name, "Y");
    }

    #[test]
    fn suppressed_enum_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/enums.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Status",
            vec![make_member("Active", MemberKind::EnumMember)],
            Some(0),
        )];

        // Suppress on line 1 (byte offset 10 => line 1 with no offsets)
        let supps = vec![Suppression {
            line: 1,
            comment_line: 0,
            kind: Some(IssueKind::UnusedEnumMember),
        }];
        let mut supp_map: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        supp_map.insert(FileId(1), &supps);
        let suppressions = SuppressionContext::from_map(supp_map);

        let (enum_members, _) =
            find_unused_members(&graph, &[], &[], &suppressions, &FxHashMap::default(), &[]);
        assert!(
            enum_members.is_empty(),
            "suppressed enum member should not be flagged"
        );
    }

    #[test]
    fn suppressed_class_member_not_flagged() {
        use crate::suppress::{IssueKind, Suppression};

        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Service",
            vec![make_member("doWork", MemberKind::ClassMethod)],
            Some(0),
        )];

        let supps = vec![Suppression {
            line: 1,
            comment_line: 0,
            kind: Some(IssueKind::UnusedClassMember),
        }];
        let mut supp_map: FxHashMap<FileId, &[Suppression]> = FxHashMap::default();
        supp_map.insert(FileId(1), &supps);
        let suppressions = SuppressionContext::from_map(supp_map);

        let (_, class_members) =
            find_unused_members(&graph, &[], &[], &suppressions, &FxHashMap::default(), &[]);
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
        graph.modules[1].set_reachable(true);
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
            whole_object_uses: vec!["S".to_string()], // aliased local name
            ..Default::default()
        }];

        let (enum_members, _) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Object.values(S) maps S→Status, so all members of Status should be considered used
        assert!(
            enum_members.is_empty(),
            "whole object use via alias should suppress all members"
        );
    }

    #[test]
    fn this_field_chained_access_not_flagged() {
        // `this.service = new MyService()` then `this.service.doWork()`
        // should recognize doWork as a used member of MyService.
        // The visitor emits MemberAccess { object: "MyService", member: "doWork" }
        // after resolving the `this.service` binding via binding_target_names.
        let mut graph = build_graph(&[("/src/main.ts", true), ("/src/service.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "MyService",
            vec![
                make_member("doWork", MemberKind::ClassMethod),
                make_member("unusedMethod", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        // Consumer imports MyService, stores in a field, and calls through it.
        // The visitor resolves `this.service.doWork()` → `MyService.doWork`.
        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/main.ts"),
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
            member_accesses: vec![MemberAccess {
                // Already resolved by visitor from `this.service.doWork()` → `MyService.doWork`
                object: "MyService".to_string(),
                member: "doWork".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        // Only unusedMethod should be flagged; doWork is used via this.service.doWork()
        assert_eq!(class_members.len(), 1);
        assert_eq!(class_members[0].member_name, "unusedMethod");
    }

    #[test]
    fn interface_member_usage_propagates_to_implementers() {
        let mut graph = build_graph(&[
            ("/src/main.ts", true),
            ("/src/scroll-strategy.interface.ts", false),
            ("/src/fixed-size-strategy.ts", false),
            ("/src/scroll-viewport.ts", false),
        ]);
        graph.modules[1].set_reachable(true);
        graph.modules[2].set_reachable(true);
        graph.modules[3].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "VirtualScrollStrategy",
            vec![],
            Some(3),
        )];
        graph.modules[2].exports = vec![make_export_with_members(
            "FixedSizeScrollStrategy",
            vec![
                make_member("attached", MemberKind::ClassProperty),
                make_member("attach", MemberKind::ClassMethod),
                make_member("detach", MemberKind::ClassMethod),
                make_member("unusedHelper", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let modules = vec![make_module_with_class_heritage(
            2,
            "FixedSizeScrollStrategy",
            None,
            &["VirtualScrollStrategy"],
        )];

        let resolved_modules = vec![
            ResolvedModule {
                file_id: FileId(2),
                path: PathBuf::from("/src/fixed-size-strategy.ts"),
                resolved_imports: vec![ResolvedImport {
                    info: ImportInfo {
                        source: "./scroll-strategy.interface".to_string(),
                        imported_name: ImportedName::Named("VirtualScrollStrategy".to_string()),
                        local_name: "VirtualScrollStrategy".to_string(),
                        is_type_only: false,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(1)),
                }],
                ..Default::default()
            },
            ResolvedModule {
                file_id: FileId(3),
                path: PathBuf::from("/src/scroll-viewport.ts"),
                resolved_imports: vec![ResolvedImport {
                    info: ImportInfo {
                        source: "./scroll-strategy.interface".to_string(),
                        imported_name: ImportedName::Named("VirtualScrollStrategy".to_string()),
                        local_name: "VirtualScrollStrategy".to_string(),
                        is_type_only: false,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(1)),
                }],
                member_accesses: vec![
                    MemberAccess {
                        object: "VirtualScrollStrategy".to_string(),
                        member: "attach".to_string(),
                    },
                    MemberAccess {
                        object: "VirtualScrollStrategy".to_string(),
                        member: "attached".to_string(),
                    },
                    MemberAccess {
                        object: "VirtualScrollStrategy".to_string(),
                        member: "detach".to_string(),
                    },
                ],
                ..Default::default()
            },
        ];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &modules,
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );

        let unused_names: FxHashSet<String> = class_members
            .iter()
            .map(|member| format!("{}.{}", member.parent_name, member.member_name))
            .collect();

        assert!(
            !unused_names.contains("FixedSizeScrollStrategy.attach"),
            "attach should be credited through interface usage: {unused_names:?}"
        );
        assert!(
            !unused_names.contains("FixedSizeScrollStrategy.attached"),
            "attached should be credited through interface usage: {unused_names:?}"
        );
        assert!(
            !unused_names.contains("FixedSizeScrollStrategy.detach"),
            "detach should be credited through interface usage: {unused_names:?}"
        );
        assert!(
            unused_names.contains("FixedSizeScrollStrategy.unusedHelper"),
            "unrelated members should still be reported: {unused_names:?}"
        );
    }

    #[test]
    fn same_named_interfaces_do_not_share_member_usage() {
        let mut graph = build_graph(&[
            ("/src/main.ts", true),
            ("/src/one-interface.ts", false),
            ("/src/two-interface.ts", false),
            ("/src/one-impl.ts", false),
            ("/src/two-impl.ts", false),
            ("/src/consumer.ts", false),
        ]);
        graph.modules[1].set_reachable(true);
        graph.modules[2].set_reachable(true);
        graph.modules[3].set_reachable(true);
        graph.modules[4].set_reachable(true);
        graph.modules[5].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members("Strategy", vec![], Some(5))];
        graph.modules[2].exports = vec![make_export_with_members("Strategy", vec![], Some(0))];
        graph.modules[3].exports = vec![make_export_with_members(
            "OneStrategy",
            vec![make_member("attach", MemberKind::ClassMethod)],
            Some(0),
        )];
        graph.modules[4].exports = vec![make_export_with_members(
            "TwoStrategy",
            vec![make_member("attach", MemberKind::ClassMethod)],
            Some(0),
        )];

        let modules = vec![
            make_module_with_class_heritage(3, "OneStrategy", None, &["Strategy"]),
            make_module_with_class_heritage(4, "TwoStrategy", None, &["Strategy"]),
        ];

        let resolved_modules = vec![
            ResolvedModule {
                file_id: FileId(3),
                path: PathBuf::from("/src/one-impl.ts"),
                resolved_imports: vec![ResolvedImport {
                    info: ImportInfo {
                        source: "./one-interface".to_string(),
                        imported_name: ImportedName::Named("Strategy".to_string()),
                        local_name: "Strategy".to_string(),
                        is_type_only: true,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(1)),
                }],
                ..Default::default()
            },
            ResolvedModule {
                file_id: FileId(4),
                path: PathBuf::from("/src/two-impl.ts"),
                resolved_imports: vec![ResolvedImport {
                    info: ImportInfo {
                        source: "./two-interface".to_string(),
                        imported_name: ImportedName::Named("Strategy".to_string()),
                        local_name: "Strategy".to_string(),
                        is_type_only: true,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(2)),
                }],
                ..Default::default()
            },
            ResolvedModule {
                file_id: FileId(5),
                path: PathBuf::from("/src/consumer.ts"),
                resolved_imports: vec![ResolvedImport {
                    info: ImportInfo {
                        source: "./one-interface".to_string(),
                        imported_name: ImportedName::Named("Strategy".to_string()),
                        local_name: "Strategy".to_string(),
                        is_type_only: true,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(1)),
                }],
                member_accesses: vec![MemberAccess {
                    object: "Strategy".to_string(),
                    member: "attach".to_string(),
                }],
                ..Default::default()
            },
        ];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &modules,
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );

        let unused_names: FxHashSet<String> = class_members
            .iter()
            .map(|member| format!("{}.{}", member.parent_name, member.member_name))
            .collect();

        assert!(
            !unused_names.contains("OneStrategy.attach"),
            "OneStrategy.attach should be credited through its own interface export: {unused_names:?}"
        );
        assert!(
            unused_names.contains("TwoStrategy.attach"),
            "TwoStrategy.attach should remain unused when only the other interface export is used: {unused_names:?}"
        );
    }

    #[test]
    fn same_named_exports_do_not_share_member_usage() {
        let mut graph = build_graph(&[
            ("/src/entry.ts", true),
            ("/src/one.ts", false),
            ("/src/two.ts", false),
        ]);
        graph.modules[1].set_reachable(true);
        graph.modules[2].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "Widget",
            vec![
                make_member("refresh", MemberKind::ClassMethod),
                make_member("unusedOne", MemberKind::ClassMethod),
            ],
            Some(0),
        )];
        graph.modules[2].exports = vec![make_export_with_members(
            "Widget",
            vec![
                make_member("refresh", MemberKind::ClassMethod),
                make_member("unusedTwo", MemberKind::ClassMethod),
            ],
            Some(0),
        )];

        let resolved_modules = vec![ResolvedModule {
            file_id: FileId(0),
            path: PathBuf::from("/src/entry.ts"),
            resolved_imports: vec![
                ResolvedImport {
                    info: ImportInfo {
                        source: "./one".to_string(),
                        imported_name: ImportedName::Named("Widget".to_string()),
                        local_name: "FirstWidget".to_string(),
                        is_type_only: false,
                        span: Span::new(0, 30),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(1)),
                },
                ResolvedImport {
                    info: ImportInfo {
                        source: "./two".to_string(),
                        imported_name: ImportedName::Named("Widget".to_string()),
                        local_name: "SecondWidget".to_string(),
                        is_type_only: false,
                        span: Span::new(31, 62),
                        source_span: Span::default(),
                    },
                    target: ResolveResult::InternalModule(FileId(2)),
                },
            ],
            member_accesses: vec![MemberAccess {
                object: "FirstWidget".to_string(),
                member: "refresh".to_string(),
            }],
            ..Default::default()
        }];

        let (_, class_members) = find_unused_members(
            &graph,
            &resolved_modules,
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );

        let unused_members: FxHashSet<(String, String)> = class_members
            .iter()
            .map(|member| {
                (
                    member.path.display().to_string(),
                    format!("{}.{}", member.parent_name, member.member_name),
                )
            })
            .collect();

        assert_eq!(
            unused_members.len(),
            3,
            "unexpected members: {unused_members:?}"
        );
        assert!(
            unused_members.contains(&("/src/one.ts".to_string(), "Widget.unusedOne".to_string()))
        );
        assert!(
            unused_members.contains(&("/src/two.ts".to_string(), "Widget.refresh".to_string()))
        );
        assert!(
            unused_members.contains(&("/src/two.ts".to_string(), "Widget.unusedTwo".to_string()))
        );
        assert!(
            !unused_members.contains(&("/src/one.ts".to_string(), "Widget.refresh".to_string())),
            "member usage from /src/one.ts should not leak into /src/two.ts: {unused_members:?}"
        );
    }

    #[test]
    fn export_with_no_members_skipped() {
        let mut graph = build_graph(&[("/src/entry.ts", true), ("/src/utils.ts", false)]);
        graph.modules[1].set_reachable(true);
        graph.modules[1].exports = vec![make_export_with_members(
            "helper",
            vec![], // no members
            Some(0),
        )];

        let (enum_members, class_members) = find_unused_members(
            &graph,
            &[],
            &[],
            &SuppressionContext::empty(),
            &FxHashMap::default(),
            &[],
        );
        assert!(enum_members.is_empty());
        assert!(class_members.is_empty());
    }
}
