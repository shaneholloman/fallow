use crate::health_types::{
    CoverageGapSummary, CoverageGaps, FileHealthScore, UntestedExport, UntestedFile,
};

pub(super) struct CoverageGapData {
    pub report: CoverageGaps,
    pub runtime_paths: Vec<std::path::PathBuf>,
}

/// Output from `compute_file_scores`, including auxiliary data for refactoring targets.
pub(super) struct FileScoreOutput {
    pub scores: Vec<FileHealthScore>,
    /// Static coverage gaps derived from runtime-vs-test reachability.
    pub coverage: CoverageGapData,
    /// Files participating in circular dependencies (absolute paths).
    pub circular_files: rustc_hash::FxHashSet<std::path::PathBuf>,
    /// Top 3 functions by cognitive complexity per file (name, line, cognitive score).
    pub top_complex_fns: rustc_hash::FxHashMap<std::path::PathBuf, Vec<(String, u32, u16)>>,
    /// Files that are configured entry points.
    pub entry_points: rustc_hash::FxHashSet<std::path::PathBuf>,
    /// Total number of value exports per file (for dead code gate: total_value_exports >= 3).
    pub value_export_counts: rustc_hash::FxHashMap<std::path::PathBuf, usize>,
    /// Unused export names per file (for evidence linking).
    pub unused_export_names: rustc_hash::FxHashMap<std::path::PathBuf, Vec<String>>,
    /// Cycle members per file: maps each file to the other files in its cycle.
    pub cycle_members: rustc_hash::FxHashMap<std::path::PathBuf, Vec<std::path::PathBuf>>,
    /// Aggregate counts from AnalysisResults for vital signs.
    pub analysis_counts: crate::vital_signs::AnalysisCounts,
}

/// Aggregate complexity totals from a parsed module.
///
/// Returns `(total_cyclomatic, total_cognitive, function_count, lines)`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "line count is bounded by source file size"
)]
pub(super) fn aggregate_complexity(
    module: &fallow_core::extract::ModuleInfo,
) -> (u32, u32, usize, u32) {
    let cyc: u32 = module
        .complexity
        .iter()
        .map(|f| u32::from(f.cyclomatic))
        .sum();
    let cog: u32 = module
        .complexity
        .iter()
        .map(|f| u32::from(f.cognitive))
        .sum();
    let funcs = module.complexity.len();
    // line_offsets length = number of lines in the file
    let lines = module.line_offsets.len() as u32;
    (cyc, cog, funcs, lines)
}

/// Compute the dead code ratio for a single file.
///
/// Returns the fraction of VALUE exports with zero references (0.0-1.0).
/// Type-only exports (interfaces, type aliases) are excluded from both
/// numerator and denominator to avoid inflating the ratio for well-typed
/// codebases. Returns 1.0 if the entire file is unused, 0.0 if it has no
/// value exports.
pub(super) fn compute_dead_code_ratio(
    path: &std::path::Path,
    exports: &[fallow_core::graph::ExportSymbol],
    unused_files: &rustc_hash::FxHashSet<&std::path::Path>,
    unused_exports_by_path: &rustc_hash::FxHashMap<&std::path::Path, usize>,
) -> f64 {
    if unused_files.contains(path) {
        return 1.0;
    }
    let value_exports = exports.iter().filter(|e| !e.is_type_only).count();
    if value_exports == 0 {
        return 0.0;
    }
    let unused = unused_exports_by_path.get(path).copied().unwrap_or(0);
    (unused as f64 / value_exports as f64).min(1.0)
}

/// Compute complexity density: total cyclomatic / lines of code.
///
/// Returns 0.0 when the file has no lines.
pub(super) fn compute_complexity_density(total_cyclomatic: u32, lines: u32) -> f64 {
    if lines > 0 {
        f64::from(total_cyclomatic) / f64::from(lines)
    } else {
        0.0
    }
}

/// Count unused VALUE exports per file path for O(1) lookup.
///
/// Type-only exports (interfaces, type aliases) are intentionally excluded ---
/// they are a different concern than unused functions/components.
pub(super) fn count_unused_exports_by_path(
    unused_exports: &[fallow_core::results::UnusedExport],
) -> rustc_hash::FxHashMap<&std::path::Path, usize> {
    let mut map: rustc_hash::FxHashMap<&std::path::Path, usize> = rustc_hash::FxHashMap::default();
    for exp in unused_exports {
        *map.entry(exp.path.as_path()).or_default() += 1;
    }
    map
}

pub(super) fn build_coverage_summary(
    runtime_files: usize,
    covered_files: usize,
    untested_files: usize,
    untested_exports: usize,
) -> CoverageGapSummary {
    let file_coverage_pct = if runtime_files == 0 {
        100.0
    } else {
        ((covered_files as f64 / runtime_files as f64) * 1000.0).round() / 10.0
    };

    CoverageGapSummary {
        runtime_files,
        covered_files,
        file_coverage_pct,
        untested_files,
        untested_exports,
    }
}

fn compute_coverage_gaps(
    graph: &fallow_core::graph::ModuleGraph,
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    module_by_id: &rustc_hash::FxHashMap<
        fallow_core::discover::FileId,
        &fallow_core::extract::ModuleInfo,
    >,
) -> CoverageGapData {
    let mut runtime_files = 0usize;
    let mut covered_files = 0usize;
    let mut runtime_paths = Vec::new();
    let mut files = Vec::new();
    let mut exports = Vec::new();

    for node in &graph.modules {
        if !node.is_runtime_reachable {
            continue;
        }

        let Some(path) = file_paths.get(&node.file_id) else {
            continue;
        };

        // Skip non-executable assets (CSS/SCSS/LESS/SASS) from coverage gap analysis.
        // These are runtime-reachable (imported by JS) but not testable in the same way.
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext, "css" | "scss" | "less" | "sass"))
        {
            continue;
        }

        runtime_paths.push((*path).clone());

        runtime_files += 1;
        if node.is_test_reachable {
            covered_files += 1;
        } else {
            files.push(UntestedFile {
                path: (*path).clone(),
                value_export_count: node.exports.iter().filter(|e| !e.is_type_only).count(),
            });
        }

        let Some(module) = module_by_id.get(&node.file_id) else {
            continue;
        };

        for export in &node.exports {
            if export.is_type_only {
                continue;
            }

            let has_test_dependency = export.references.iter().any(|reference| {
                graph
                    .modules
                    .get(reference.from_file.0 as usize)
                    .is_some_and(|module| module.is_test_reachable)
            });
            if has_test_dependency {
                continue;
            }

            let (line, col) = fallow_types::extract::byte_offset_to_line_col(
                &module.line_offsets,
                export.span.start,
            );
            exports.push(UntestedExport {
                path: (*path).clone(),
                export_name: export.name.to_string(),
                line,
                col,
            });
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    exports.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then_with(|| a.export_name.cmp(&b.export_name))
            .then_with(|| a.line.cmp(&b.line))
    });

    CoverageGapData {
        report: CoverageGaps {
            summary: build_coverage_summary(
                runtime_files,
                covered_files,
                files.len(),
                exports.len(),
            ),
            files,
            exports,
        },
        runtime_paths,
    }
}

/// Compute the maintainability index for a single file.
///
/// Formula:
/// ```text
/// fan_out_penalty = min(ln(fan_out + 1) * 4, 15)
/// MI = 100 - (complexity_density * 30) - (dead_code_ratio * 20) - fan_out_penalty
/// ```
///
/// Fan-out uses logarithmic scaling capped at 15 points to reflect diminishing
/// marginal risk (the 30th import is less concerning than the 5th) and prevent
/// composition-root files from being unfairly penalized.
///
/// Clamped to \[0, 100\]. Higher is better.
pub(super) fn compute_maintainability_index(
    complexity_density: f64,
    dead_code_ratio: f64,
    fan_out: usize,
) -> f64 {
    let fan_out_penalty = ((fan_out as f64).ln_1p() * 4.0).min(15.0);
    #[expect(
        clippy::suboptimal_flops,
        reason = "formula matches documented specification"
    )]
    let score = 100.0 - (complexity_density * 30.0) - (dead_code_ratio * 20.0) - fan_out_penalty;
    score.clamp(0.0, 100.0)
}

/// Compute per-file health scores using a pre-computed analysis output.
///
/// The caller provides an `AnalysisOutput` (with graph and dead code results)
/// so this function does not need to re-run the analysis pipeline. Complexity
/// density is derived from the already-parsed modules.
pub(super) fn compute_file_scores(
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    analysis_output: fallow_core::AnalysisOutput,
) -> Result<FileScoreOutput, String> {
    let graph = analysis_output.graph.ok_or("graph not available")?;
    let results = &analysis_output.results;

    // Build auxiliary data for refactoring targets
    let circular_files: rustc_hash::FxHashSet<std::path::PathBuf> = results
        .circular_dependencies
        .iter()
        .flat_map(|c| c.files.iter().cloned())
        .collect();

    let mut top_complex_fns: rustc_hash::FxHashMap<std::path::PathBuf, Vec<(String, u32, u16)>> =
        rustc_hash::FxHashMap::default();
    for module in modules {
        if module.complexity.is_empty() {
            continue;
        }
        let Some(path) = file_paths.get(&module.file_id) else {
            continue;
        };
        let mut funcs: Vec<(String, u32, u16)> = module
            .complexity
            .iter()
            .map(|f| (f.name.clone(), f.line, f.cognitive))
            .collect();
        funcs.sort_by(|a, b| b.2.cmp(&a.2));
        funcs.truncate(3);
        if funcs[0].2 > 0 {
            top_complex_fns.insert((*path).clone(), funcs);
        }
    }

    // Build cycle membership map: each file -> list of other files in its cycle
    let mut cycle_members: rustc_hash::FxHashMap<std::path::PathBuf, Vec<std::path::PathBuf>> =
        rustc_hash::FxHashMap::default();
    for cycle in &results.circular_dependencies {
        for file in &cycle.files {
            let others: Vec<std::path::PathBuf> =
                cycle.files.iter().filter(|f| *f != file).cloned().collect();
            cycle_members
                .entry(file.clone())
                .or_default()
                .extend(others);
        }
    }
    // Deduplicate: a file may appear in multiple cycles
    for members in cycle_members.values_mut() {
        members.sort();
        members.dedup();
    }

    // Build unused export names per file for evidence linking
    let mut unused_export_names: rustc_hash::FxHashMap<std::path::PathBuf, Vec<String>> =
        rustc_hash::FxHashMap::default();
    for exp in &results.unused_exports {
        unused_export_names
            .entry(exp.path.clone())
            .or_default()
            .push(exp.export_name.clone());
    }

    let mut entry_points: rustc_hash::FxHashSet<std::path::PathBuf> =
        rustc_hash::FxHashSet::default();
    let mut value_export_counts: rustc_hash::FxHashMap<std::path::PathBuf, usize> =
        rustc_hash::FxHashMap::default();

    // Build a set of unused file paths for O(1) lookup
    let unused_files: rustc_hash::FxHashSet<&std::path::Path> = results
        .unused_files
        .iter()
        .map(|f| f.path.as_path())
        .collect();

    let unused_exports_by_path = count_unused_exports_by_path(&results.unused_exports);

    // Build FileId -> ModuleInfo lookup
    let module_by_id: rustc_hash::FxHashMap<
        fallow_core::discover::FileId,
        &fallow_core::extract::ModuleInfo,
    > = modules.iter().map(|m| (m.file_id, m)).collect();
    let coverage = compute_coverage_gaps(&graph, file_paths, &module_by_id);

    let mut scores = Vec::with_capacity(graph.modules.len());

    for node in &graph.modules {
        let Some(path) = file_paths.get(&node.file_id) else {
            continue;
        };

        // Track entry points for refactoring target exclusion
        if node.is_entry_point {
            entry_points.insert((*path).clone());
        }

        // Fan-in: number of files that import this file
        let fan_in = graph
            .reverse_deps
            .get(node.file_id.0 as usize)
            .map_or(0, Vec::len);

        // Fan-out: number of files this file imports (from edge_range)
        let fan_out = node.edge_range.len();

        let (total_cyclomatic, total_cognitive, function_count, lines) = module_by_id
            .get(&node.file_id)
            .map_or((0, 0, 0, 0), |module| aggregate_complexity(module));

        // Track value export count for dead code gate
        let value_exports = node.exports.iter().filter(|e| !e.is_type_only).count();
        // Clone the path once; reuse via .clone() for map keys that need ownership,
        // and move the final copy into FileHealthScore to avoid one extra allocation.
        let path_owned = (*path).clone();
        value_export_counts.insert(path_owned.clone(), value_exports);

        // For fully-unused files, populate all export names as evidence
        // (unused_exports only tracks individually-unused exports, not exports from unreachable files)
        if unused_files.contains(path_owned.as_path())
            && !unused_export_names.contains_key(&path_owned)
        {
            let names: Vec<String> = node
                .exports
                .iter()
                .filter(|e| !e.is_type_only)
                .map(|e| e.name.to_string())
                .collect();
            if !names.is_empty() {
                unused_export_names.insert(path_owned.clone(), names);
            }
        }

        let dead_code_ratio = compute_dead_code_ratio(
            path_owned.as_path(),
            &node.exports,
            &unused_files,
            &unused_exports_by_path,
        );
        let complexity_density = compute_complexity_density(total_cyclomatic, lines);

        // Round intermediate values first so the MI in JSON is reproducible
        // from the other rounded fields in the same JSON object.
        let dead_code_ratio_rounded = (dead_code_ratio * 100.0).round() / 100.0;
        let complexity_density_rounded = (complexity_density * 100.0).round() / 100.0;

        let maintainability_index = compute_maintainability_index(
            complexity_density_rounded,
            dead_code_ratio_rounded,
            fan_out,
        );

        scores.push(FileHealthScore {
            path: path_owned,
            fan_in,
            fan_out,
            dead_code_ratio: dead_code_ratio_rounded,
            complexity_density: complexity_density_rounded,
            maintainability_index: (maintainability_index * 10.0).round() / 10.0,
            total_cyclomatic,
            total_cognitive,
            function_count,
            lines,
        });
    }

    // Apply --changed-since filter to keep scores consistent with findings
    if let Some(changed) = changed_files {
        scores.retain(|s| changed.contains(&s.path));
    }

    // Exclude zero-function files (barrel/re-export files) by default.
    // These have zero complexity density and can only be penalized by dead_code_ratio
    // and fan-out, making their MI a dead-code detector rather than a maintainability
    // metric. They pollute the rankings and obscure actually complex files.
    scores.retain(|s| s.function_count > 0);

    // Sort by maintainability index ascending (worst files first)
    scores.sort_by(|a, b| {
        a.maintainability_index
            .partial_cmp(&b.maintainability_index)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute aggregate counts for vital signs
    let total_exports: usize = modules.iter().map(|m| m.exports.len()).sum();
    let dead_exports = results.unused_exports.len() + results.unused_types.len();
    let unused_deps = results.unused_dependencies.len()
        + results.unused_dev_dependencies.len()
        + results.unused_optional_dependencies.len();
    // Total deps not available from ResolvedConfig — approximate as 0.
    // The snapshot counts.total_deps will be 0 until package.json data is plumbed.
    let total_deps = 0usize;

    Ok(FileScoreOutput {
        scores,
        coverage,
        circular_files,
        top_complex_fns,
        entry_points,
        value_export_counts,
        unused_export_names,
        cycle_members,
        analysis_counts: crate::vital_signs::AnalysisCounts {
            total_exports,
            dead_files: results.unused_files.len(),
            dead_exports,
            unused_deps,
            circular_deps: results.circular_dependencies.len(),
            total_deps,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintainability_perfect_score() {
        // No complexity, no dead code, no fan-out -> 100
        assert!((compute_maintainability_index(0.0, 0.0, 0) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_clamped_at_zero() {
        // Very high complexity density -> clamped to 0
        assert!((compute_maintainability_index(10.0, 1.0, 100) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_formula_correct() {
        // complexity_density=0.5, dead_code_ratio=0.3, fan_out=10
        // fan_out_penalty = min(ln(11) * 4, 15) = min(9.59, 15) = 9.59
        // 100 - 15 - 6 - 9.59 = 69.41
        let result = compute_maintainability_index(0.5, 0.3, 10);
        let expected = 11.0_f64.ln().mul_add(-4.0, 100.0 - 15.0 - 6.0);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn maintainability_dead_file_penalty() {
        // Fully dead file: dead_code_ratio=1.0, fan_out=0
        // fan_out_penalty = min(ln(1) * 4, 15) = 0
        // 100 - 0 - 20 - 0 = 80
        let result = compute_maintainability_index(0.0, 1.0, 0);
        assert!((result - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_fan_out_is_logarithmic() {
        // fan_out=10: penalty = min(ln(11) * 4, 15) ~ 9.59
        let result_10 = compute_maintainability_index(0.0, 0.0, 10);
        // fan_out=100: penalty = min(ln(101) * 4, 15) = 15 (capped)
        let result_100 = compute_maintainability_index(0.0, 0.0, 100);
        // fan_out=200: also capped at 15
        let result_200 = compute_maintainability_index(0.0, 0.0, 200);

        // Logarithmic: 10->100 jump is much less than 10x the penalty
        assert!(result_10 > 90.0); // ~90.4
        assert!(result_100 > 84.0); // 85.0 (capped)
        // Capped: 100 and 200 should score the same
        assert!((result_100 - result_200).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_fan_out_capped_at_15() {
        // Very high fan-out should not push score below 65 (100 - 0 - 20 - 15)
        // even with full dead code
        let result = compute_maintainability_index(0.0, 1.0, 1000);
        assert!((result - 65.0).abs() < f64::EPSILON);
    }

    // --- compute_complexity_density ---

    #[test]
    fn complexity_density_zero_lines() {
        assert!((compute_complexity_density(10, 0)).abs() < f64::EPSILON);
    }

    #[test]
    fn complexity_density_normal() {
        // 10 cyclomatic / 100 lines = 0.1
        let result = compute_complexity_density(10, 100);
        assert!((result - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn complexity_density_high() {
        // 50 cyclomatic / 10 lines = 5.0
        let result = compute_complexity_density(50, 10);
        assert!((result - 5.0).abs() < f64::EPSILON);
    }

    // --- compute_dead_code_ratio ---

    #[test]
    fn dead_code_ratio_no_exports() {
        let unused_files = rustc_hash::FxHashSet::default();
        let unused_map = rustc_hash::FxHashMap::default();
        let path = std::path::Path::new("/src/foo.ts");
        let exports: Vec<fallow_core::graph::ExportSymbol> = vec![];

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!((ratio).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_all_unused_file() {
        let mut unused_files: rustc_hash::FxHashSet<&std::path::Path> =
            rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/foo.ts");
        unused_files.insert(path);
        let unused_map = rustc_hash::FxHashMap::default();
        let exports: Vec<fallow_core::graph::ExportSymbol> = vec![];

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_mix() {
        let unused_files = rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/foo.ts");

        // 3 value exports, 1 type-only export
        let exports = vec![
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("a".into()),
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("b".into()),
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("c".into()),
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("MyType".into()),
                is_type_only: true,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
        ];

        // 2 of 3 value exports are unused
        let mut unused_map: rustc_hash::FxHashMap<&std::path::Path, usize> =
            rustc_hash::FxHashMap::default();
        unused_map.insert(path, 2);

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        // 2/3 ~ 0.6667
        assert!((ratio - 2.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn dead_code_ratio_all_type_only_exports() {
        let unused_files = rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/types.ts");

        // Only type exports -> value_exports = 0 -> ratio 0.0
        let exports = vec![fallow_core::graph::ExportSymbol {
            name: fallow_core::extract::ExportName::Named("Foo".into()),
            is_type_only: true,
            is_public: false,
            span: oxc_span::Span::empty(0),
            references: vec![],
            members: vec![],
        }];
        let unused_map = rustc_hash::FxHashMap::default();

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!((ratio).abs() < f64::EPSILON);
    }

    // --- aggregate_complexity ---

    #[test]
    fn aggregate_complexity_empty_module() {
        let module = fallow_core::extract::ModuleInfo {
            file_id: fallow_core::discover::FileId(0),
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
        };

        let (cyc, cog, funcs, lines) = aggregate_complexity(&module);
        assert_eq!(cyc, 0);
        assert_eq!(cog, 0);
        assert_eq!(funcs, 0);
        assert_eq!(lines, 0);
    }

    #[test]
    fn aggregate_complexity_single_function() {
        let module = fallow_core::extract::ModuleInfo {
            file_id: fallow_core::discover::FileId(0),
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
            line_offsets: vec![0, 10, 20, 30, 40], // 5 lines
            complexity: vec![fallow_types::extract::FunctionComplexity {
                name: "doStuff".into(),
                line: 1,
                col: 0,
                cyclomatic: 7,
                cognitive: 4,
                line_count: 5,
            }],
        };

        let (cyc, cog, funcs, lines) = aggregate_complexity(&module);
        assert_eq!(cyc, 7);
        assert_eq!(cog, 4);
        assert_eq!(funcs, 1);
        assert_eq!(lines, 5);
    }

    #[test]
    fn aggregate_complexity_multiple_functions() {
        let module = fallow_core::extract::ModuleInfo {
            file_id: fallow_core::discover::FileId(0),
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
            line_offsets: vec![0, 10, 20], // 3 lines
            complexity: vec![
                fallow_types::extract::FunctionComplexity {
                    name: "a".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 3,
                    cognitive: 2,
                    line_count: 1,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "b".into(),
                    line: 2,
                    col: 0,
                    cyclomatic: 5,
                    cognitive: 8,
                    line_count: 2,
                },
            ],
        };

        let (cyc, cog, funcs, lines) = aggregate_complexity(&module);
        assert_eq!(cyc, 8);
        assert_eq!(cog, 10);
        assert_eq!(funcs, 2);
        assert_eq!(lines, 3);
    }

    // --- count_unused_exports_by_path ---

    #[test]
    fn count_unused_exports_empty() {
        let exports: Vec<fallow_core::results::UnusedExport> = vec![];
        let map = count_unused_exports_by_path(&exports);
        assert!(map.is_empty());
    }

    #[test]
    fn count_unused_exports_groups_by_path() {
        let exports = vec![
            fallow_core::results::UnusedExport {
                path: std::path::PathBuf::from("/src/a.ts"),
                export_name: "foo".into(),
                is_type_only: false,
                line: 1,
                col: 0,
                span_start: 0,
                is_re_export: false,
            },
            fallow_core::results::UnusedExport {
                path: std::path::PathBuf::from("/src/a.ts"),
                export_name: "bar".into(),
                is_type_only: false,
                line: 5,
                col: 0,
                span_start: 40,
                is_re_export: false,
            },
            fallow_core::results::UnusedExport {
                path: std::path::PathBuf::from("/src/b.ts"),
                export_name: "baz".into(),
                is_type_only: false,
                line: 1,
                col: 0,
                span_start: 0,
                is_re_export: false,
            },
        ];
        let map = count_unused_exports_by_path(&exports);
        assert_eq!(map.get(std::path::Path::new("/src/a.ts")).copied(), Some(2));
        assert_eq!(map.get(std::path::Path::new("/src/b.ts")).copied(), Some(1));
    }

    // --- additional compute_dead_code_ratio edge cases ---

    #[test]
    fn dead_code_ratio_all_value_exports_unused() {
        let unused_files = rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/foo.ts");

        let exports = vec![
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("a".into()),
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("b".into()),
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
        ];

        // All 2 value exports unused
        let mut unused_map: rustc_hash::FxHashMap<&std::path::Path, usize> =
            rustc_hash::FxHashMap::default();
        unused_map.insert(path, 2);

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_clamped_when_unused_exceeds_value_exports() {
        // Edge case: unused count > value exports (shouldn't happen, but clamped to 1.0)
        let unused_files = rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/foo.ts");

        let exports = vec![fallow_core::graph::ExportSymbol {
            name: fallow_core::extract::ExportName::Named("a".into()),
            is_type_only: false,
            is_public: false,
            span: oxc_span::Span::empty(0),
            references: vec![],
            members: vec![],
        }];

        let mut unused_map: rustc_hash::FxHashMap<&std::path::Path, usize> =
            rustc_hash::FxHashMap::default();
        unused_map.insert(path, 5); // 5 unused but only 1 value export

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_no_unused_exports_for_path() {
        let unused_files = rustc_hash::FxHashSet::default();
        let path = std::path::Path::new("/src/clean.ts");

        let exports = vec![fallow_core::graph::ExportSymbol {
            name: fallow_core::extract::ExportName::Named("used".into()),
            is_type_only: false,
            is_public: false,
            span: oxc_span::Span::empty(0),
            references: vec![],
            members: vec![],
        }];

        // Path not in unused map -> 0 unused
        let unused_map = rustc_hash::FxHashMap::default();
        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_map);
        assert!(ratio.abs() < f64::EPSILON);
    }

    // --- additional compute_complexity_density edge cases ---

    #[test]
    fn complexity_density_zero_cyclomatic_with_lines() {
        let result = compute_complexity_density(0, 100);
        assert!(result.abs() < f64::EPSILON);
    }

    #[test]
    fn complexity_density_single_line() {
        // 1 cyclomatic / 1 line = 1.0
        let result = compute_complexity_density(1, 1);
        assert!((result - 1.0).abs() < f64::EPSILON);
    }

    // --- additional compute_maintainability_index edge cases ---

    #[test]
    fn maintainability_only_complexity_penalty() {
        // complexity_density = 3.0 -> penalty = 3.0 * 30 = 90
        // 100 - 90 - 0 - 0 = 10
        let result = compute_maintainability_index(3.0, 0.0, 0);
        assert!((result - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_only_dead_code_penalty() {
        // dead_code_ratio = 0.5 -> penalty = 0.5 * 20 = 10
        // 100 - 0 - 10 - 0 = 90
        let result = compute_maintainability_index(0.0, 0.5, 0);
        assert!((result - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_fan_out_one() {
        // fan_out = 1: penalty = min(ln(2) * 4, 15) = ~2.77
        let result = compute_maintainability_index(0.0, 0.0, 1);
        let expected = 2.0_f64.ln().mul_add(-4.0, 100.0);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn maintainability_all_penalties_maxed() {
        // complexity_density = 10.0 -> 300 penalty (clamped total to 0)
        // dead_code_ratio = 1.0 -> 20 penalty
        // fan_out = 1000 -> 15 penalty (capped)
        // Total raw = 100 - 300 - 20 - 15 = -235 -> clamped to 0
        let result = compute_maintainability_index(10.0, 1.0, 1000);
        assert!(result.abs() < f64::EPSILON);
    }

    // --- count_unused_exports_by_path additional ---

    #[test]
    fn count_unused_exports_single_file_single_export() {
        let exports = vec![fallow_core::results::UnusedExport {
            path: std::path::PathBuf::from("/src/only.ts"),
            export_name: "lonely".into(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        }];
        let map = count_unused_exports_by_path(&exports);
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(std::path::Path::new("/src/only.ts")).copied(),
            Some(1)
        );
    }

    // --- compute_file_scores ---

    /// Helper to build a minimal `ModuleGraph` from scratch.
    fn build_test_graph(
        files: &[fallow_core::discover::DiscoveredFile],
        entry_point_paths: &[std::path::PathBuf],
        resolved_modules: &[fallow_core::resolve::ResolvedModule],
    ) -> fallow_core::graph::ModuleGraph {
        let entry_points: Vec<fallow_core::discover::EntryPoint> = entry_point_paths
            .iter()
            .map(|p| fallow_core::discover::EntryPoint {
                path: p.clone(),
                source: fallow_core::discover::EntryPointSource::PackageJsonMain,
            })
            .collect();
        fallow_core::graph::ModuleGraph::build(resolved_modules, &entry_points, files)
    }

    /// Helper to create a `ModuleInfo` with given complexity and line count.
    fn make_module_info(
        file_id: u32,
        line_count: usize,
        functions: Vec<fallow_types::extract::FunctionComplexity>,
    ) -> fallow_core::extract::ModuleInfo {
        fallow_core::extract::ModuleInfo {
            file_id: fallow_core::discover::FileId(file_id),
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
            line_offsets: (0..line_count).map(|i| (i * 10) as u32).collect(),
            complexity: functions,
        }
    }

    #[test]
    fn compute_file_scores_empty_graph() {
        let files: Vec<fallow_core::discover::DiscoveredFile> = vec![];
        let graph = build_test_graph(&files, &[], &[]);
        let modules: Vec<fallow_core::extract::ModuleInfo> = vec![];
        let file_paths = rustc_hash::FxHashMap::default();

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert!(result.scores.is_empty());
        assert!(result.circular_files.is_empty());
        assert!(result.top_complex_fns.is_empty());
        assert!(result.entry_points.is_empty());
        assert_eq!(result.analysis_counts.total_exports, 0);
        assert_eq!(result.analysis_counts.dead_files, 0);
    }

    #[test]
    fn compute_file_scores_no_graph_returns_error() {
        let modules: Vec<fallow_core::extract::ModuleInfo> = vec![];
        let file_paths = rustc_hash::FxHashMap::default();

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output);
        assert!(result.is_err());
        match result {
            Err(msg) => assert_eq!(msg, "graph not available"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn compute_file_scores_single_file_with_function() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("foo".into()),
                local_name: None,
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                members: vec![],
            }],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, std::slice::from_ref(&path_a), &resolved_modules);

        let modules = vec![make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "foo".into(),
                line: 1,
                col: 0,
                cyclomatic: 5,
                cognitive: 3,
                line_count: 10,
            }],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert_eq!(result.scores.len(), 1);

        let score = &result.scores[0];
        assert_eq!(score.path, path_a);
        assert_eq!(score.total_cyclomatic, 5);
        assert_eq!(score.total_cognitive, 3);
        assert_eq!(score.function_count, 1);
        assert_eq!(score.lines, 10);
        // complexity_density = 5/10 = 0.5, dead_code_ratio = 0.0
        assert!((score.complexity_density - 0.5).abs() < f64::EPSILON);
        assert!(score.dead_code_ratio.abs() < f64::EPSILON);
        // Entry point should be tracked
        assert!(result.entry_points.contains(&path_a));
    }

    #[test]
    fn compute_file_scores_excludes_barrel_files() {
        // A file with zero functions should be excluded (barrel file)
        let path_a = std::path::PathBuf::from("/src/index.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 50,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, std::slice::from_ref(&path_a), &resolved_modules);

        // Module with lines but no functions (barrel file)
        let modules = vec![make_module_info(0, 5, vec![])];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        // Barrel files (function_count == 0) are excluded
        assert!(result.scores.is_empty());
    }

    #[test]
    fn compute_file_scores_changed_since_filter() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let path_b = std::path::PathBuf::from("/src/b.ts");
        let files = vec![
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(0),
                path: path_a.clone(),
                size_bytes: 100,
            },
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                size_bytes: 100,
            },
        ];

        let resolved_modules = vec![
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(0),
                path: path_a,
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
        ];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![
            make_module_info(
                0,
                10,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "fn_a".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 2,
                    cognitive: 1,
                    line_count: 10,
                }],
            ),
            make_module_info(
                1,
                10,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "fn_b".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 3,
                    cognitive: 2,
                    line_count: 10,
                }],
            ),
        ];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);
        file_paths.insert(fallow_core::discover::FileId(1), &files[1].path);

        // Only path_b is in the changed set
        let path_b_check = std::path::PathBuf::from("/src/b.ts");
        let mut changed = rustc_hash::FxHashSet::default();
        changed.insert(path_b);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, Some(&changed), output).unwrap();
        // Only path_b should remain
        assert_eq!(result.scores.len(), 1);
        assert_eq!(result.scores[0].path, path_b_check);
    }

    #[test]
    fn compute_file_scores_sorted_by_maintainability_ascending() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let path_b = std::path::PathBuf::from("/src/b.ts");
        let files = vec![
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(0),
                path: path_a.clone(),
                size_bytes: 100,
            },
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                size_bytes: 100,
            },
        ];

        let resolved_modules = vec![
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(0),
                path: path_a.clone(),
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b,
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
        ];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        // File a: high complexity (low MI), file b: low complexity (high MI)
        let modules = vec![
            make_module_info(
                0,
                10,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "complex_fn".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 30,
                    cognitive: 20,
                    line_count: 10,
                }],
            ),
            make_module_info(
                1,
                100,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "simple_fn".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 1,
                    cognitive: 0,
                    line_count: 100,
                }],
            ),
        ];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);
        file_paths.insert(fallow_core::discover::FileId(1), &files[1].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert_eq!(result.scores.len(), 2);
        // Sorted ascending: worst (lowest MI) first
        assert!(result.scores[0].maintainability_index <= result.scores[1].maintainability_index);
        // File a (high complexity) should come first
        assert_eq!(result.scores[0].path, path_a);
    }

    #[test]
    fn compute_file_scores_with_unused_file_populates_evidence() {
        let path_a = std::path::PathBuf::from("/src/unused.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("orphan".into()),
                local_name: None,
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                members: vec![],
            }],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "orphan".into(),
                line: 1,
                col: 0,
                cyclomatic: 1,
                cognitive: 0,
                line_count: 10,
            }],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let mut results = fallow_types::results::AnalysisResults::default();
        results
            .unused_files
            .push(fallow_types::results::UnusedFile {
                path: path_a.clone(),
            });

        let output = fallow_core::AnalysisOutput {
            results,
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        // Unused file should have dead_code_ratio = 1.0
        assert_eq!(result.scores.len(), 1);
        assert!((result.scores[0].dead_code_ratio - 1.0).abs() < f64::EPSILON);
        // Evidence: unused export names should be populated
        assert!(result.unused_export_names.contains_key(&path_a));
        let names = &result.unused_export_names[&path_a];
        assert_eq!(names, &["orphan"]);
        // Analysis counts
        assert_eq!(result.analysis_counts.dead_files, 1);
    }

    #[test]
    fn compute_file_scores_tracks_top_complex_functions() {
        let path_a = std::path::PathBuf::from("/src/complex.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 500,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        // 4 functions, top 3 by cognitive should be kept
        let modules = vec![make_module_info(
            0,
            50,
            vec![
                fallow_types::extract::FunctionComplexity {
                    name: "high".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 10,
                    cognitive: 20,
                    line_count: 10,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "medium".into(),
                    line: 11,
                    col: 0,
                    cyclomatic: 5,
                    cognitive: 10,
                    line_count: 10,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "low".into(),
                    line: 21,
                    col: 0,
                    cyclomatic: 2,
                    cognitive: 5,
                    line_count: 10,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "trivial".into(),
                    line: 31,
                    col: 0,
                    cyclomatic: 1,
                    cognitive: 1,
                    line_count: 10,
                },
            ],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert!(result.top_complex_fns.contains_key(&path_a));
        let top = &result.top_complex_fns[&path_a];
        // Truncated to 3, sorted by cognitive descending
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, "high");
        assert_eq!(top[0].2, 20);
        assert_eq!(top[1].0, "medium");
        assert_eq!(top[1].2, 10);
        assert_eq!(top[2].0, "low");
        assert_eq!(top[2].2, 5);
    }

    #[test]
    fn compute_file_scores_with_circular_deps() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let path_b = std::path::PathBuf::from("/src/b.ts");
        let files = vec![
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(0),
                path: path_a.clone(),
                size_bytes: 100,
            },
            fallow_core::discover::DiscoveredFile {
                id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                size_bytes: 100,
            },
        ];

        let resolved_modules = vec![
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(0),
                path: path_a.clone(),
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                exports: vec![],
                re_exports: vec![],
                resolved_imports: vec![],
                resolved_dynamic_imports: vec![],
                resolved_dynamic_patterns: vec![],
                member_accesses: vec![],
                whole_object_uses: vec![],
                has_cjs_exports: false,
                unused_import_bindings: rustc_hash::FxHashSet::default(),
            },
        ];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![
            make_module_info(
                0,
                10,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "fn_a".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 2,
                    cognitive: 1,
                    line_count: 10,
                }],
            ),
            make_module_info(
                1,
                10,
                vec![fallow_types::extract::FunctionComplexity {
                    name: "fn_b".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 3,
                    cognitive: 2,
                    line_count: 10,
                }],
            ),
        ];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);
        file_paths.insert(fallow_core::discover::FileId(1), &files[1].path);

        let mut results = fallow_types::results::AnalysisResults::default();
        results
            .circular_dependencies
            .push(fallow_types::results::CircularDependency {
                files: vec![path_a.clone(), path_b.clone()],
                length: 2,
                line: 1,
                col: 0,
                is_cross_package: false,
            });

        let output = fallow_core::AnalysisOutput {
            results,
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        // Both files should appear in circular_files
        assert!(result.circular_files.contains(&path_a));
        assert!(result.circular_files.contains(&path_b));
        // Cycle members should map each to the other
        assert!(result.cycle_members.contains_key(&path_a));
        assert_eq!(result.cycle_members[&path_a], vec![path_b.clone()]);
        assert!(result.cycle_members.contains_key(&path_b));
        assert_eq!(result.cycle_members[&path_b], vec![path_a]);
        // Analysis counts
        assert_eq!(result.analysis_counts.circular_deps, 1);
    }

    #[test]
    fn compute_file_scores_analysis_counts_unused_exports_and_types() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        // Module has 2 exports so total_exports = 2
        let mut module = make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "fn_a".into(),
                line: 1,
                col: 0,
                cyclomatic: 1,
                cognitive: 0,
                line_count: 10,
            }],
        );
        module.exports = vec![
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("foo".into()),
                local_name: None,
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                members: vec![],
            },
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("bar".into()),
                local_name: None,
                is_type_only: false,
                is_public: false,
                span: oxc_span::Span::empty(0),
                members: vec![],
            },
        ];
        let modules = vec![module];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let mut results = fallow_types::results::AnalysisResults::default();
        results
            .unused_exports
            .push(fallow_types::results::UnusedExport {
                path: path_a.clone(),
                export_name: "foo".into(),
                is_type_only: false,
                line: 1,
                col: 0,
                span_start: 0,
                is_re_export: false,
            });
        results
            .unused_types
            .push(fallow_types::results::UnusedExport {
                path: path_a,
                export_name: "MyType".into(),
                is_type_only: true,
                line: 5,
                col: 0,
                span_start: 40,
                is_re_export: false,
            });
        results
            .unused_dependencies
            .push(fallow_types::results::UnusedDependency {
                package_name: "lodash".into(),
                location: fallow_types::results::DependencyLocation::Dependencies,
                path: std::path::PathBuf::from("/package.json"),
                line: 1,
            });

        let output = fallow_core::AnalysisOutput {
            results,
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert_eq!(result.analysis_counts.total_exports, 2);
        // dead_exports = unused_exports + unused_types = 1 + 1 = 2
        assert_eq!(result.analysis_counts.dead_exports, 2);
        assert_eq!(result.analysis_counts.unused_deps, 1);
    }

    #[test]
    fn compute_file_scores_module_not_in_file_paths_skipped() {
        // When file_paths doesn't contain a FileId, the module should be skipped
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a,
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "fn_a".into(),
                line: 1,
                col: 0,
                cyclomatic: 2,
                cognitive: 1,
                line_count: 10,
            }],
        )];

        // Empty file_paths: no FileId mappings
        let file_paths: rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf> =
            rustc_hash::FxHashMap::default();

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        assert!(result.scores.is_empty());
    }

    #[test]
    fn compute_file_scores_mi_rounded_to_one_decimal() {
        // Verify that maintainability_index is rounded to one decimal place
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, std::slice::from_ref(&path_a), &resolved_modules);

        let modules = vec![make_module_info(
            0,
            100,
            vec![fallow_types::extract::FunctionComplexity {
                name: "fn".into(),
                line: 1,
                col: 0,
                cyclomatic: 7,
                cognitive: 3,
                line_count: 100,
            }],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        let mi = result.scores[0].maintainability_index;
        // MI should be rounded to 1 decimal place
        let rounded = (mi * 10.0).round() / 10.0;
        assert!((mi - rounded).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_file_scores_value_export_counts_tracked() {
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        // 2 value exports + 1 type-only export
        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("a".into()),
                    local_name: None,
                    is_type_only: false,
                    is_public: false,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("b".into()),
                    local_name: None,
                    is_type_only: false,
                    is_public: false,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("T".into()),
                    local_name: None,
                    is_type_only: true,
                    is_public: false,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                },
            ],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "fn_a".into(),
                line: 1,
                col: 0,
                cyclomatic: 2,
                cognitive: 1,
                line_count: 10,
            }],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        // value_export_counts should track only non-type-only exports
        assert_eq!(result.value_export_counts[&path_a], 2);
    }

    #[test]
    fn compute_file_scores_top_complex_fns_zero_cognitive_excluded() {
        // If all functions have cognitive=0, top_complex_fns should not be populated
        let path_a = std::path::PathBuf::from("/src/simple.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![],
            re_exports: vec![],
            resolved_imports: vec![],
            resolved_dynamic_imports: vec![],
            resolved_dynamic_patterns: vec![],
            member_accesses: vec![],
            whole_object_uses: vec![],
            has_cjs_exports: false,
            unused_import_bindings: rustc_hash::FxHashSet::default(),
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        let modules = vec![make_module_info(
            0,
            10,
            vec![fallow_types::extract::FunctionComplexity {
                name: "trivial".into(),
                line: 1,
                col: 0,
                cyclomatic: 1,
                cognitive: 0,
                line_count: 10,
            }],
        )];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
        };

        let result = compute_file_scores(&modules, &file_paths, None, output).unwrap();
        // Top function has cognitive=0, so it should not be included
        assert!(!result.top_complex_fns.contains_key(&path_a));
    }
}
