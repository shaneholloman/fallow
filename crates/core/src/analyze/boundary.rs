use rustc_hash::FxHashMap;

use fallow_config::ResolvedConfig;

use crate::discover::FileId;
use crate::graph::ModuleGraph;
use crate::suppress::{self, IssueKind, Suppression};
use fallow_types::results::BoundaryViolation;

use super::{LineOffsetsMap, byte_offset_to_line_col};

/// Detect imports that cross architecture boundary zones without permission.
///
/// For each reachable module, classifies it into a zone and checks all its
/// import targets. If the target is in a different zone that the source zone
/// is not allowed to import from, a `BoundaryViolation` is emitted.
pub fn find_boundary_violations(
    graph: &ModuleGraph,
    config: &ResolvedConfig,
    suppressions_by_file: &FxHashMap<FileId, &[Suppression]>,
    line_offsets_by_file: &LineOffsetsMap<'_>,
) -> Vec<BoundaryViolation> {
    let boundaries = &config.boundaries;
    let mut violations = Vec::new();

    // Cache zone classification per FileId to avoid repeated glob matching.
    let mut zone_cache: FxHashMap<FileId, Option<String>> = FxHashMap::default();

    let classify =
        |file_id: FileId, cache: &mut FxHashMap<FileId, Option<String>>| -> Option<String> {
            if let Some(cached) = cache.get(&file_id) {
                return cached.clone();
            }
            let node = &graph.modules[file_id.0 as usize];
            let rel_path = node
                .path
                .strip_prefix(&config.root)
                .ok()
                .map(|p| p.to_string_lossy().replace('\\', "/"));
            let zone = rel_path.and_then(|p| boundaries.classify_zone(&p).map(str::to_owned));
            cache.insert(file_id, zone.clone());
            zone
        };

    for node in &graph.modules {
        // Only check reachable files — unreachable files are already reported as unused.
        if !node.is_reachable() && !node.is_entry_point() {
            continue;
        }

        let Some(from_zone) = classify(node.file_id, &mut zone_cache) else {
            continue; // Unzoned files are unrestricted.
        };

        // Check if this zone has any restrictions at all.
        let has_rule = boundaries.rules.iter().any(|r| r.from_zone == from_zone);
        if !has_rule {
            continue; // Unrestricted zone — skip all edge checks.
        }

        // Check file-level suppression.
        if suppressions_by_file
            .get(&node.file_id)
            .is_some_and(|supps| suppress::is_file_suppressed(supps, IssueKind::BoundaryViolation))
        {
            continue;
        }

        let targets = graph.edges_for(node.file_id);
        for target_id in targets {
            let Some(to_zone) = classify(target_id, &mut zone_cache) else {
                continue; // Unzoned targets always allowed.
            };

            if boundaries.is_import_allowed(&from_zone, &to_zone) {
                continue;
            }

            // Check line-level suppression at the import site.
            let span_start = graph.find_import_span_start(node.file_id, target_id);
            let (line, col) = span_start.map_or((1, 0), |s| {
                byte_offset_to_line_col(line_offsets_by_file, node.file_id, s)
            });

            if suppressions_by_file
                .get(&node.file_id)
                .is_some_and(|supps| {
                    suppress::is_suppressed(supps, line, IssueKind::BoundaryViolation)
                })
            {
                continue;
            }

            // Use target's relative path as the import specifier since the raw
            // specifier string is not carried in graph edges.
            let target_node = &graph.modules[target_id.0 as usize];
            let import_specifier = target_node.path.strip_prefix(&config.root).map_or_else(
                |_| target_node.path.to_string_lossy().replace('\\', "/"),
                |p| p.to_string_lossy().replace('\\', "/"),
            );

            violations.push(BoundaryViolation {
                from_path: node.path.clone(),
                to_path: target_node.path.clone(),
                from_zone: from_zone.clone(),
                to_zone: to_zone.clone(),
                import_specifier,
                line,
                col,
            });
        }
    }

    // Warn about zones that matched zero files — likely a misconfiguration.
    if !boundaries.is_empty() {
        let classified_zones: rustc_hash::FxHashSet<&str> =
            zone_cache.values().filter_map(|z| z.as_deref()).collect();
        for zone in &boundaries.zones {
            if !classified_zones.contains(zone.name.as_str()) {
                tracing::warn!(
                    "boundary zone '{}' matched 0 reachable files — check your directory \
                     structure, pattern, or whether these files are all currently unreachable",
                    zone.name
                );
            }
        }
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discover::{DiscoveredFile, EntryPoint, EntryPointSource};
    use crate::graph::ModuleGraph;
    use crate::resolve::ResolvedModule;
    use crate::suppress::Suppression;
    use fallow_config::{
        BoundaryConfig, BoundaryRule, BoundaryZone, DuplicatesConfig, FallowConfig, HealthConfig,
        OutputFormat, ResolvedConfig, RulesConfig, Severity,
    };
    use rustc_hash::FxHashSet;
    use std::path::PathBuf;

    fn make_config(root: PathBuf, boundaries: BoundaryConfig) -> ResolvedConfig {
        FallowConfig {
            schema: None,
            extends: vec![],
            entry: vec![],
            ignore_patterns: vec![],
            framework: vec![],
            workspaces: None,
            ignore_dependencies: vec![],
            ignore_exports: vec![],
            duplicates: DuplicatesConfig::default(),
            health: HealthConfig::default(),
            rules: RulesConfig {
                boundary_violation: Severity::Error,
                ..RulesConfig::default()
            },
            boundaries,
            production: false,
            plugins: vec![],
            dynamically_loaded: vec![],
            overrides: vec![],
            regression: None,
            codeowners: None,
            public_packages: vec![],
        }
        .resolve(root, OutputFormat::Human, 1, true, true)
    }

    fn resolved_module(file_id: FileId, path: PathBuf) -> ResolvedModule {
        ResolvedModule {
            file_id,
            path,
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: FxHashSet::default(),
        }
    }

    fn build_graph(
        root: &std::path::Path,
        file_names: &[&str],
        edges: &[(usize, usize)],
    ) -> (Vec<DiscoveredFile>, ModuleGraph) {
        let files: Vec<DiscoveredFile> = file_names
            .iter()
            .enumerate()
            .map(|(i, name)| DiscoveredFile {
                id: FileId(i as u32),
                path: root.join(name),
                size_bytes: 100,
            })
            .collect();

        let entry_points = vec![EntryPoint {
            path: files[0].path.clone(),
            source: EntryPointSource::ManualEntry,
        }];

        let resolved: Vec<ResolvedModule> = files
            .iter()
            .map(|f| {
                let mut rm = resolved_module(f.id, f.path.clone());
                // Add import edges
                for &(from, to) in edges {
                    if from == f.id.0 as usize {
                        rm.resolved_imports.push(crate::resolve::ResolvedImport {
                            target: crate::resolve::ResolveResult::InternalModule(FileId(
                                to as u32,
                            )),
                            info: fallow_types::extract::ImportInfo {
                                source: format!("./{}", file_names[to]),
                                imported_name: fallow_types::extract::ImportedName::Default,
                                local_name: "x".to_string(),
                                is_type_only: false,
                                span: oxc_span::Span::new(0, 10),
                                source_span: oxc_span::Span::new(0, 10),
                            },
                        });
                    }
                }
                rm
            })
            .collect();

        let graph = ModuleGraph::build(&resolved, &entry_points, &files);
        (files, graph)
    }

    #[test]
    fn no_boundaries_returns_empty() {
        let root = PathBuf::from("/tmp/boundary-test");
        let config = make_config(root.clone(), BoundaryConfig::default());
        let (_, graph) = build_graph(&root, &["src/ui/Button.tsx", "src/db/query.ts"], &[(0, 1)]);
        let suppressions = FxHashMap::default();
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert!(violations.is_empty());
    }

    #[test]
    fn allowed_import_no_violation() {
        let root = PathBuf::from("/tmp/boundary-test");
        let boundaries = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec!["src/ui/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec!["src/shared/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["shared".to_string()],
            }],
        };
        let config = make_config(root.clone(), boundaries);
        let (_, graph) = build_graph(
            &root,
            &["src/ui/Button.tsx", "src/shared/utils.ts"],
            &[(0, 1)],
        );
        let suppressions = FxHashMap::default();
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert!(violations.is_empty());
    }

    #[test]
    fn disallowed_import_produces_violation() {
        let root = PathBuf::from("/tmp/boundary-test");
        let boundaries = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec!["src/ui/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec!["src/db/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec!["src/shared/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["shared".to_string()],
            }],
        };
        let config = make_config(root.clone(), boundaries);
        let (_, graph) = build_graph(&root, &["src/ui/Button.tsx", "src/db/query.ts"], &[(0, 1)]);
        let suppressions = FxHashMap::default();
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].from_zone, "ui");
        assert_eq!(violations[0].to_zone, "db");
    }

    #[test]
    fn self_import_always_allowed() {
        let root = PathBuf::from("/tmp/boundary-test");
        let boundaries = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec!["src/ui/**".to_string()],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec![],
            }],
        };
        let config = make_config(root.clone(), boundaries);
        let (_, graph) = build_graph(
            &root,
            &["src/ui/Button.tsx", "src/ui/helpers.ts"],
            &[(0, 1)],
        );
        let suppressions = FxHashMap::default();
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert!(violations.is_empty());
    }

    #[test]
    fn unzoned_files_unrestricted() {
        let root = PathBuf::from("/tmp/boundary-test");
        let boundaries = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec!["src/ui/**".to_string()],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec![],
            }],
        };
        let config = make_config(root.clone(), boundaries);
        // src/utils.ts is unzoned — importing it from ui should be allowed
        let (_, graph) = build_graph(&root, &["src/ui/Button.tsx", "src/utils.ts"], &[(0, 1)]);
        let suppressions = FxHashMap::default();
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert!(violations.is_empty());
    }

    #[test]
    fn file_level_suppression_skips_file() {
        let root = PathBuf::from("/tmp/boundary-test");
        let boundaries = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec!["src/ui/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec!["src/db/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec![],
            }],
        };
        let config = make_config(root.clone(), boundaries);
        let (_, graph) = build_graph(&root, &["src/ui/Button.tsx", "src/db/query.ts"], &[(0, 1)]);

        // File-level suppression (line 0)
        let supps = vec![Suppression {
            line: 0,
            kind: Some(IssueKind::BoundaryViolation),
        }];
        let mut suppressions = FxHashMap::default();
        suppressions.insert(FileId(0), supps.as_slice());
        let line_offsets = FxHashMap::default();

        let violations = find_boundary_violations(&graph, &config, &suppressions, &line_offsets);
        assert!(violations.is_empty());
    }
}
