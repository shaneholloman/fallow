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
    /// Istanbul match stats: functions matched / total (only meaningful with Istanbul model).
    pub istanbul_matched: usize,
    pub istanbul_total: usize,
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

/// CRAP score threshold (inclusive). CC=5 untested gives exactly 30 (5^2 + 5),
/// matching the canonical CRAP threshold from Savoia & Evans (2007).
pub(super) const CRAP_THRESHOLD: f64 = 30.0;

/// Compute per-function CRAP scores using the static binary model.
///
/// Binary model: test-reachable file -> CRAP = CC, untested -> CRAP = CC^2 + CC.
/// Superseded by `compute_crap_scores_estimated` but retained for test coverage
/// of the binary formula behavior.
///
/// Returns `(max_crap, count_above_threshold)`.
#[cfg(test)]
#[expect(
    clippy::suboptimal_flops,
    reason = "cc * cc + cc matches the CRAP formula specification"
)]
fn compute_crap_scores_binary(
    complexity: &[fallow_types::extract::FunctionComplexity],
    is_test_reachable: bool,
) -> (f64, usize) {
    if complexity.is_empty() {
        return (0.0, 0);
    }
    let mut max = 0.0_f64;
    let mut above = 0usize;
    for f in complexity {
        let cc = f64::from(f.cyclomatic);
        let crap = if is_test_reachable { cc } else { cc * cc + cc };
        max = max.max(crap);
        if crap >= CRAP_THRESHOLD {
            above += 1;
        }
    }
    ((max * 10.0).round() / 10.0, above)
}

/// Istanbul CRAP result: CRAP scores plus match statistics.
pub(super) struct IstanbulCrapResult {
    pub max_crap: f64,
    pub above_threshold: usize,
    /// Functions that found a match in Istanbul data.
    pub matched: usize,
    /// Total functions evaluated.
    pub total: usize,
}

/// Compute per-function CRAP scores using Istanbul coverage data.
///
/// For each function, looks up its per-function statement coverage percentage
/// from the Istanbul data and applies the canonical CRAP formula:
/// `CRAP = CC^2 * (1 - cov/100)^3 + CC`
///
/// Functions not found in the coverage data fall back to the estimated model
/// using the file's test-reachability status.
///
/// Returns CRAP scores and match statistics for reporting.
#[expect(
    clippy::suboptimal_flops,
    reason = "cc * cc + cc matches the CRAP formula specification"
)]
fn compute_crap_scores_istanbul(
    complexity: &[fallow_types::extract::FunctionComplexity],
    file_coverage: Option<&IstanbulFileCoverage>,
    is_test_reachable: bool,
) -> IstanbulCrapResult {
    if complexity.is_empty() {
        return IstanbulCrapResult {
            max_crap: 0.0,
            above_threshold: 0,
            matched: 0,
            total: 0,
        };
    }
    let mut max = 0.0_f64;
    let mut above = 0usize;
    let mut matched = 0usize;
    for f in complexity {
        let cc = f64::from(f.cyclomatic);
        let crap = if let Some(cov_pct) =
            file_coverage.and_then(|fc| fc.lookup(f.name.as_str(), f.line))
        {
            matched += 1;
            crap_formula(cc, cov_pct)
        } else if is_test_reachable {
            cc
        } else {
            cc * cc + cc
        };
        max = max.max(crap);
        if crap >= CRAP_THRESHOLD {
            above += 1;
        }
    }
    IstanbulCrapResult {
        max_crap: (max * 10.0).round() / 10.0,
        above_threshold: above,
        matched,
        total: complexity.len(),
    }
}

/// Estimated coverage for functions directly referenced by test-reachable modules.
/// An export imported in a test file likely exercises most of the function body.
const DIRECT_TEST_COVERAGE_ESTIMATE: f64 = 85.0;

/// Estimated coverage for functions in test-reachable files but not directly
/// referenced by tests. The file is imported by tests, so the function may
/// be exercised indirectly, but with lower confidence.
const INDIRECT_TEST_COVERAGE_ESTIMATE: f64 = 40.0;

/// Compute per-function CRAP scores using graph-based coverage estimation.
///
/// For each function, estimates coverage from the module graph:
/// - Function name matches an export with test-reachable references: 85%
/// - File is test-reachable but function not directly referenced: 40%
/// - File is not test-reachable at all: 0%
///
/// Applies the canonical CRAP formula with these estimates.
/// Returns `(max_crap, count_above_threshold)`.
fn compute_crap_scores_estimated(
    complexity: &[fallow_types::extract::FunctionComplexity],
    test_referenced_exports: &rustc_hash::FxHashSet<String>,
    is_test_reachable: bool,
) -> (f64, usize) {
    if complexity.is_empty() {
        return (0.0, 0);
    }
    let mut max = 0.0_f64;
    let mut above = 0usize;
    for f in complexity {
        let cc = f64::from(f.cyclomatic);
        let estimated_coverage = if test_referenced_exports.contains(f.name.as_str()) {
            DIRECT_TEST_COVERAGE_ESTIMATE
        } else if is_test_reachable {
            INDIRECT_TEST_COVERAGE_ESTIMATE
        } else {
            0.0
        };
        let crap = crap_formula(cc, estimated_coverage);
        max = max.max(crap);
        if crap >= CRAP_THRESHOLD {
            above += 1;
        }
    }
    ((max * 10.0).round() / 10.0, above)
}

/// Build the set of export names that have at least one test-reachable reference.
///
/// This is the per-function signal: if an export named "foo" has a reference from
/// a test-reachable module, the function "foo" is considered directly tested.
fn build_test_referenced_exports(
    exports: &[fallow_core::graph::ExportSymbol],
    graph_modules: &[fallow_core::graph::ModuleNode],
) -> rustc_hash::FxHashSet<String> {
    let mut set = rustc_hash::FxHashSet::default();
    for export in exports {
        if export.is_type_only {
            continue;
        }
        let has_test_ref = export.references.iter().any(|reference| {
            graph_modules
                .get(reference.from_file.0 as usize)
                .is_some_and(fallow_core::graph::ModuleNode::is_test_reachable)
        });
        if has_test_ref {
            set.insert(export.name.to_string());
        }
    }
    set
}

/// Canonical CRAP formula: `CC^2 * (1 - cov/100)^3 + CC`.
/// At 100% coverage: CRAP = CC. At 0% coverage: CRAP = CC^2 + CC.
#[expect(
    clippy::suboptimal_flops,
    reason = "explicit multiplication matches the CRAP formula specification"
)]
fn crap_formula(cc: f64, coverage_pct: f64) -> f64 {
    let uncovered = 1.0 - coverage_pct / 100.0;
    cc * cc * uncovered * uncovered * uncovered + cc
}

/// Pre-processed per-function coverage data for a single file,
/// derived from Istanbul `coverage-final.json`.
pub(super) struct IstanbulFileCoverage {
    /// Per-function coverage percentages, keyed by (name, line).
    /// Line is 1-based, matching both fallow's `FunctionComplexity.line`
    /// and Istanbul's `FnEntry.line`.
    functions: rustc_hash::FxHashMap<(String, u32), f64>,
}

impl IstanbulFileCoverage {
    /// Look up coverage for a function by name and start line.
    ///
    /// Tries exact (name, line) match first. If that fails, tries matching
    /// by name alone (for small line-number differences due to formatting).
    fn lookup(&self, name: &str, line: u32) -> Option<f64> {
        // Exact match
        if let Some(&pct) = self.functions.get(&(name.to_string(), line)) {
            return Some(pct);
        }
        // Fuzzy: match by name, pick the closest line within offset of 2.
        // Uses min_by_key for determinism (FxHashMap iteration order is arbitrary).
        self.functions
            .iter()
            .filter(|((n, l), _)| n == name && l.abs_diff(line) <= 2)
            .min_by_key(|((_, l), _)| l.abs_diff(line))
            .map(|(_, &pct)| pct)
    }
}

/// Loaded Istanbul coverage data, keyed by canonical file path.
pub(super) struct IstanbulCoverage {
    files: rustc_hash::FxHashMap<std::path::PathBuf, IstanbulFileCoverage>,
}

impl IstanbulCoverage {
    /// Get coverage data for a file path.
    pub fn get(&self, path: &std::path::Path) -> Option<&IstanbulFileCoverage> {
        self.files.get(path)
    }
}

/// Load Istanbul coverage data from a `coverage-final.json` file or directory.
///
/// Auto-detect a `coverage-final.json` file in common locations relative to the project root.
///
/// Checks (in order): `coverage/coverage-final.json`, `.nyc_output/coverage-final.json`.
/// Returns the first path found, or `None` if no coverage file exists.
pub(super) fn auto_detect_coverage(root: &std::path::Path) -> Option<std::path::PathBuf> {
    let candidates = [
        root.join("coverage/coverage-final.json"),
        root.join(".nyc_output/coverage-final.json"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

/// If `path` is a directory, looks for `coverage-final.json` inside it.
/// Parses the Istanbul JSON format and pre-computes per-function statement
/// coverage percentages for efficient lookup during CRAP scoring.
///
/// When `coverage_root` is provided, file paths in the Istanbul data are rebased:
/// the `coverage_root` prefix is stripped and `project_root` is prepended, enabling
/// cross-environment matching (e.g., coverage from CI used on a local checkout).
pub(super) fn load_istanbul_coverage(
    path: &std::path::Path,
    coverage_root: Option<&std::path::Path>,
    project_root: Option<&std::path::Path>,
) -> Result<IstanbulCoverage, String> {
    let file_path = if path.is_dir() {
        let candidate = path.join("coverage-final.json");
        if candidate.is_file() {
            candidate
        } else {
            return Err(format!(
                "no coverage-final.json found in {}",
                path.display()
            ));
        }
    } else {
        path.to_path_buf()
    };

    let json = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read coverage file {}: {e}", file_path.display()))?;

    let raw: std::collections::BTreeMap<String, oxc_coverage_instrument::FileCoverage> =
        oxc_coverage_instrument::parse_coverage_map(&json).map_err(|e| {
            format!(
                "failed to parse coverage data from {}: {e}",
                file_path.display()
            )
        })?;

    let mut files = rustc_hash::FxHashMap::default();
    for file_cov in raw.values() {
        let raw_path = std::path::PathBuf::from(&file_cov.path);
        // Rebase path if --coverage-root was provided
        let file_path = if let (Some(cov_root), Some(proj_root)) = (coverage_root, project_root) {
            raw_path
                .strip_prefix(cov_root)
                .map(|rel| proj_root.join(rel))
                .unwrap_or(raw_path)
        } else {
            raw_path
        };
        let canonical = dunce::canonicalize(&file_path).unwrap_or(file_path);

        let mut functions = rustc_hash::FxHashMap::default();
        for (fn_id, fn_entry) in &file_cov.fn_map {
            let coverage_pct = compute_function_statement_coverage(file_cov, fn_id, fn_entry);
            functions.insert((fn_entry.name.clone(), fn_entry.line), coverage_pct);
        }

        files.insert(canonical, IstanbulFileCoverage { functions });
    }

    Ok(IstanbulCoverage { files })
}

/// Compute statement-level coverage percentage for a single function.
///
/// Maps statements from `statementMap` to the function's body range (`loc`)
/// and computes the fraction with non-zero hit counts. When no statements
/// fall within the function body (e.g., one-liner arrow functions, getters),
/// falls back to the function hit count as a binary signal.
fn compute_function_statement_coverage(
    file_cov: &oxc_coverage_instrument::FileCoverage,
    fn_id: &str,
    fn_entry: &oxc_coverage_instrument::FnEntry,
) -> f64 {
    let fn_start_line = fn_entry.loc.start.line;
    let fn_start_col = fn_entry.loc.start.column;
    let fn_end_line = fn_entry.loc.end.line;
    let fn_end_col = fn_entry.loc.end.column;

    let mut total = 0u32;
    let mut covered = 0u32;

    for (stmt_id, stmt_loc) in &file_cov.statement_map {
        // Check if statement falls within the function body
        let after_start = stmt_loc.start.line > fn_start_line
            || (stmt_loc.start.line == fn_start_line && stmt_loc.start.column >= fn_start_col);
        let before_end = stmt_loc.end.line < fn_end_line
            || (stmt_loc.end.line == fn_end_line && stmt_loc.end.column <= fn_end_col);

        if after_start && before_end {
            total += 1;
            if file_cov.s.get(stmt_id).copied().unwrap_or(0) > 0 {
                covered += 1;
            }
        }
    }

    if total == 0 {
        // No statements in range: fall back to function hit count.
        // If the function was entered at least once, treat as 100% covered;
        // if never entered, treat as 0% covered.
        let hit = file_cov.f.get(fn_id).copied().unwrap_or(0);
        if hit > 0 { 100.0 } else { 0.0 }
    } else {
        f64::from(covered) / f64::from(total) * 100.0
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
        if !node.is_runtime_reachable() {
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

        // Check inline suppression: // fallow-ignore-file coverage-gaps
        let module = module_by_id.get(&node.file_id);
        if module.is_some_and(|m| {
            fallow_core::suppress::is_file_suppressed(
                &m.suppressions,
                fallow_types::suppress::IssueKind::CoverageGaps,
            )
        }) {
            continue;
        }

        runtime_paths.push((*path).clone());

        runtime_files += 1;
        if node.is_test_reachable() {
            covered_files += 1;
        } else {
            files.push(UntestedFile {
                path: (*path).clone(),
                value_export_count: node.exports.iter().filter(|e| !e.is_type_only).count(),
            });
        }

        let Some(module) = module else {
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
                    .is_some_and(|module| module.is_test_reachable())
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
/// dampening = min(lines / 50, 1.0)
/// fan_out_penalty = min(ln(fan_out + 1) * 4, 15)
/// MI = 100 - (complexity_density * 30 * dampening) - (dead_code_ratio * 20) - fan_out_penalty
/// ```
///
/// The dampening factor prevents complexity density from dominating the score
/// on small files. A 5-line utility with CC=2 has density 0.40, but is trivially
/// readable; without dampening it scores worse than a 192-line function with CC=57
/// (density 0.30). Files under 50 lines get proportionally reduced density weight.
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
    lines: u32,
) -> f64 {
    let dampening = (f64::from(lines) / crate::health_types::MI_DENSITY_MIN_LINES).min(1.0);
    let fan_out_penalty = ((fan_out as f64).ln_1p() * 4.0).min(15.0);
    #[expect(
        clippy::suboptimal_flops,
        reason = "formula matches documented specification"
    )]
    let score = 100.0
        - (complexity_density * 30.0 * dampening)
        - (dead_code_ratio * 20.0)
        - fan_out_penalty;
    score.clamp(0.0, 100.0)
}

/// Compute per-file health scores using a pre-computed analysis output.
///
/// The caller provides an `AnalysisOutput` (with graph and dead code results)
/// so this function does not need to re-run the analysis pipeline. Complexity
/// density is derived from the already-parsed modules.
#[expect(
    clippy::too_many_lines,
    reason = "file scoring aggregates many metrics per file"
)]
pub(super) fn compute_file_scores(
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    analysis_output: fallow_core::AnalysisOutput,
    istanbul_coverage: Option<&IstanbulCoverage>,
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
    let mut istanbul_matched = 0usize;
    let mut istanbul_total = 0usize;

    for node in &graph.modules {
        let Some(path) = file_paths.get(&node.file_id) else {
            continue;
        };

        // Track entry points for refactoring target exclusion
        if node.is_entry_point() {
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
            lines,
        );

        // CRAP scoring: combine per-function CC with coverage data.
        // Tier 3 (Istanbul): real per-function statement coverage from coverage-final.json.
        // Tier 2 (estimated): graph-based per-function estimation from export references.
        // Files suppressed via `// fallow-ignore-file coverage-gaps` are treated
        // as test-reachable to stay consistent with coverage gap output.
        let module = module_by_id.get(&node.file_id);
        let is_coverage_suppressed = module.is_some_and(|m| {
            fallow_core::suppress::is_file_suppressed(
                &m.suppressions,
                fallow_types::suppress::IssueKind::CoverageGaps,
            )
        });
        let is_test_reachable = node.is_test_reachable() || is_coverage_suppressed;
        let (crap_max, crap_above_threshold) = if let Some(istanbul) = istanbul_coverage {
            let canonical = dunce::canonicalize(&path_owned).unwrap_or_else(|_| path_owned.clone());
            let result = module.map_or(
                IstanbulCrapResult {
                    max_crap: 0.0,
                    above_threshold: 0,
                    matched: 0,
                    total: 0,
                },
                |m| {
                    compute_crap_scores_istanbul(
                        &m.complexity,
                        istanbul.get(&canonical),
                        is_test_reachable,
                    )
                },
            );
            istanbul_matched += result.matched;
            istanbul_total += result.total;
            (result.max_crap, result.above_threshold)
        } else {
            module.map_or((0.0, 0), |m| {
                let test_refs = build_test_referenced_exports(&node.exports, &graph.modules);
                compute_crap_scores_estimated(&m.complexity, &test_refs, is_test_reachable)
            })
        };

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
            crap_max,
            crap_above_threshold,
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
    let total_exports: usize = graph.modules.iter().map(|m| m.exports.len()).sum();
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
        istanbul_matched,
        istanbul_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maintainability_perfect_score() {
        // No complexity, no dead code, no fan-out -> 100
        assert!((compute_maintainability_index(0.0, 0.0, 0, 100) - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_clamped_at_zero() {
        // Very high complexity density on a large file -> clamped to 0
        assert!((compute_maintainability_index(10.0, 1.0, 100, 200) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_formula_correct() {
        // complexity_density=0.5, dead_code_ratio=0.3, fan_out=10, lines=100 (no dampening)
        // fan_out_penalty = min(ln(11) * 4, 15) = min(9.59, 15) = 9.59
        // 100 - 15 - 6 - 9.59 = 69.41
        let result = compute_maintainability_index(0.5, 0.3, 10, 100);
        let expected = 11.0_f64.ln().mul_add(-4.0, 100.0 - 15.0 - 6.0);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn maintainability_dead_file_penalty() {
        // Fully dead file: dead_code_ratio=1.0, fan_out=0
        // fan_out_penalty = min(ln(1) * 4, 15) = 0
        // 100 - 0 - 20 - 0 = 80
        let result = compute_maintainability_index(0.0, 1.0, 0, 100);
        assert!((result - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_fan_out_is_logarithmic() {
        // fan_out=10: penalty = min(ln(11) * 4, 15) ~ 9.59
        let result_10 = compute_maintainability_index(0.0, 0.0, 10, 100);
        // fan_out=100: penalty = min(ln(101) * 4, 15) = 15 (capped)
        let result_100 = compute_maintainability_index(0.0, 0.0, 100, 100);
        // fan_out=200: also capped at 15
        let result_200 = compute_maintainability_index(0.0, 0.0, 200, 100);

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
        let result = compute_maintainability_index(0.0, 1.0, 1000, 100);
        assert!((result - 65.0).abs() < f64::EPSILON);
    }

    // --- LOC dampening ---

    #[test]
    fn maintainability_small_file_dampened() {
        // 5-line file with density 0.40: dampening = 5/50 = 0.10
        // penalty = 0.40 * 30 * 0.10 = 1.2
        // MI = 100 - 1.2 = 98.8
        let small = compute_maintainability_index(0.40, 0.0, 0, 5);
        assert!((small - 98.8).abs() < 0.01);
    }

    #[test]
    fn maintainability_large_file_undampened() {
        // 192-line file with density 0.30: dampening = 1.0 (above threshold)
        // penalty = 0.30 * 30 = 9.0
        // MI = 100 - 9.0 = 91.0
        let large = compute_maintainability_index(0.30, 0.0, 0, 192);
        assert!((large - 91.0).abs() < 0.01);
    }

    #[test]
    fn maintainability_small_file_ranks_better_than_complex_large_file() {
        // Regression test for issue #118:
        // 5-line type guard (CC=2, density=0.40) must score higher than
        // 192-line nightmare function (CC=57, density=0.30)
        let trivial = compute_maintainability_index(0.40, 0.0, 0, 5);
        let nightmare = compute_maintainability_index(0.30, 0.0, 0, 192);
        assert!(
            trivial > nightmare,
            "trivial file ({trivial}) should rank better than nightmare ({nightmare})"
        );
    }

    #[test]
    fn maintainability_at_dampening_boundary() {
        // At exactly 50 lines: dampening = 1.0 (full weight)
        let at_boundary = compute_maintainability_index(0.5, 0.0, 0, 50);
        // At 51 lines: also 1.0
        let above_boundary = compute_maintainability_index(0.5, 0.0, 0, 51);
        // Both should get full density penalty
        assert!((at_boundary - above_boundary).abs() < 0.01);
    }

    #[test]
    fn maintainability_zero_lines_zero_density_penalty() {
        // 0 lines: dampening = 0.0, density penalty is zeroed out
        let result = compute_maintainability_index(5.0, 0.0, 0, 0);
        assert!((result - 100.0).abs() < f64::EPSILON);
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
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("b".into()),
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("c".into()),
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("MyType".into()),
                is_type_only: true,
                visibility: fallow_core::extract::VisibilityTag::None,
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
            visibility: fallow_core::extract::VisibilityTag::None,
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
            flag_uses: vec![],
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
            flag_uses: vec![],
            line_offsets: vec![0, 10, 20, 30, 40], // 5 lines
            complexity: vec![fallow_types::extract::FunctionComplexity {
                name: "doStuff".into(),
                line: 1,
                col: 0,
                cyclomatic: 7,
                cognitive: 4,
                line_count: 5,
                param_count: 0,
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
            flag_uses: vec![],
            line_offsets: vec![0, 10, 20], // 3 lines
            complexity: vec![
                fallow_types::extract::FunctionComplexity {
                    name: "a".into(),
                    line: 1,
                    col: 0,
                    cyclomatic: 3,
                    cognitive: 2,
                    line_count: 1,
                    param_count: 0,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "b".into(),
                    line: 2,
                    col: 0,
                    cyclomatic: 5,
                    cognitive: 8,
                    line_count: 2,
                    param_count: 0,
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
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                references: vec![],
                members: vec![],
            },
            fallow_core::graph::ExportSymbol {
                name: fallow_core::extract::ExportName::Named("b".into()),
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
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
            visibility: fallow_core::extract::VisibilityTag::None,
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
            visibility: fallow_core::extract::VisibilityTag::None,
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
        // complexity_density = 3.0, lines=100 (no dampening) -> penalty = 3.0 * 30 = 90
        // 100 - 90 - 0 - 0 = 10
        let result = compute_maintainability_index(3.0, 0.0, 0, 100);
        assert!((result - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_only_dead_code_penalty() {
        // dead_code_ratio = 0.5 -> penalty = 0.5 * 20 = 10
        // 100 - 0 - 10 - 0 = 90
        let result = compute_maintainability_index(0.0, 0.5, 0, 100);
        assert!((result - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn maintainability_fan_out_one() {
        // fan_out = 1: penalty = min(ln(2) * 4, 15) = ~2.77
        let result = compute_maintainability_index(0.0, 0.0, 1, 100);
        let expected = 2.0_f64.ln().mul_add(-4.0, 100.0);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn maintainability_all_penalties_maxed() {
        // complexity_density = 10.0, lines=200 (no dampening) -> 300 penalty (clamped total to 0)
        // dead_code_ratio = 1.0 -> 20 penalty
        // fan_out = 1000 -> 15 penalty (capped)
        // Total raw = 100 - 300 - 20 - 15 = -235 -> clamped to 0
        let result = compute_maintainability_index(10.0, 1.0, 1000, 200);
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
            flag_uses: vec![],
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None);
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
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
            }],
            ..Default::default()
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
                param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            ..Default::default()
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
                ..Default::default()
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                ..Default::default()
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
                    param_count: 0,
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
                    param_count: 0,
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
            modules: None,
            files: None,
        };

        let result =
            compute_file_scores(&modules, &file_paths, Some(&changed), output, None).unwrap();
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
                ..Default::default()
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b,
                ..Default::default()
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
                    param_count: 0,
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
                    param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
            }],
            ..Default::default()
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
                param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            ..Default::default()
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
                    param_count: 0,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "medium".into(),
                    line: 11,
                    col: 0,
                    cyclomatic: 5,
                    cognitive: 10,
                    line_count: 10,
                    param_count: 0,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "low".into(),
                    line: 21,
                    col: 0,
                    cyclomatic: 2,
                    cognitive: 5,
                    line_count: 10,
                    param_count: 0,
                },
                fallow_types::extract::FunctionComplexity {
                    name: "trivial".into(),
                    line: 31,
                    col: 0,
                    cyclomatic: 1,
                    cognitive: 1,
                    line_count: 10,
                    param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
                ..Default::default()
            },
            fallow_core::resolve::ResolvedModule {
                file_id: fallow_core::discover::FileId(1),
                path: path_b.clone(),
                ..Default::default()
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
                    param_count: 0,
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
                    param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            exports: vec![
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("foo".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("bar".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
            ],
            ..Default::default()
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        // Graph module has 2 exports so total_exports = 2
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
                param_count: 0,
            }],
        );
        module.exports = vec![
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("foo".into()),
                local_name: None,
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
            },
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("bar".into()),
                local_name: None,
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
        assert_eq!(result.analysis_counts.total_exports, 2);
        // dead_exports = unused_exports + unused_types = 1 + 1 = 2
        assert_eq!(result.analysis_counts.dead_exports, 2);
        assert_eq!(result.analysis_counts.unused_deps, 1);
    }

    /// Regression: total_exports must count from graph (post-resolution), not extraction
    /// modules. Re-export chain resolution synthesizes exports in graph.modules that don't
    /// exist in extraction ModuleInfo.exports. Using extraction counts as denominator
    /// caused dead_export_pct > 100%.
    #[test]
    fn total_exports_counts_graph_modules_not_extraction_modules() {
        // Source module (a.ts) has 2 direct exports + 1 synthesized from re-export chain
        let path_a = std::path::PathBuf::from("/src/a.ts");
        let files = vec![fallow_core::discover::DiscoveredFile {
            id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            size_bytes: 100,
        }];

        // Graph source: 3 exports (2 direct + 1 synthesized by re-export resolution)
        let resolved_modules = vec![fallow_core::resolve::ResolvedModule {
            file_id: fallow_core::discover::FileId(0),
            path: path_a.clone(),
            exports: vec![
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("foo".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("bar".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
                // Simulates a synthesized export from re-export chain resolution
                // (in real code these have Span(0,0) sentinel)
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("baz".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::new(0, 0),
                    members: vec![],
                    super_class: None,
                },
            ],
            ..Default::default()
        }];

        let graph = build_test_graph(&files, &[], &resolved_modules);

        // Extraction module only knows about 2 direct exports (no synthesized re-exports)
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
                param_count: 0,
            }],
        );
        module.exports = vec![
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("foo".into()),
                local_name: None,
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
            },
            fallow_types::extract::ExportInfo {
                name: fallow_core::extract::ExportName::Named("bar".into()),
                local_name: None,
                is_type_only: false,
                visibility: fallow_core::extract::VisibilityTag::None,
                span: oxc_span::Span::empty(0),
                members: vec![],
                super_class: None,
            },
        ];
        let modules = vec![module];

        let mut file_paths: rustc_hash::FxHashMap<
            fallow_core::discover::FileId,
            &std::path::PathBuf,
        > = rustc_hash::FxHashMap::default();
        file_paths.insert(fallow_core::discover::FileId(0), &files[0].path);

        // All 3 exports are unused
        let mut results = fallow_types::results::AnalysisResults::default();
        for name in ["foo", "bar", "baz"] {
            results
                .unused_exports
                .push(fallow_types::results::UnusedExport {
                    path: path_a.clone(),
                    export_name: name.into(),
                    is_type_only: false,
                    line: 1,
                    col: 0,
                    span_start: 0,
                    is_re_export: name == "baz",
                });
        }

        let output = fallow_core::AnalysisOutput {
            results,
            timings: None,
            graph: Some(graph),
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
        // total_exports = 3 (from graph, including synthesized re-export)
        // Before the fix this was 2 (from extraction modules), causing 150% dead exports
        assert_eq!(result.analysis_counts.total_exports, 3);
        assert_eq!(result.analysis_counts.dead_exports, 3);
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
            ..Default::default()
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
                param_count: 0,
            }],
        )];

        // Empty file_paths: no FileId mappings
        let file_paths: rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf> =
            rustc_hash::FxHashMap::default();

        let output = fallow_core::AnalysisOutput {
            results: fallow_types::results::AnalysisResults::default(),
            timings: None,
            graph: Some(graph),
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            ..Default::default()
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
                param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("b".into()),
                    local_name: None,
                    is_type_only: false,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
                fallow_types::extract::ExportInfo {
                    name: fallow_core::extract::ExportName::Named("T".into()),
                    local_name: None,
                    is_type_only: true,
                    visibility: fallow_core::extract::VisibilityTag::None,
                    span: oxc_span::Span::empty(0),
                    members: vec![],
                    super_class: None,
                },
            ],
            ..Default::default()
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
                param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
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
            ..Default::default()
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
                param_count: 0,
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
            modules: None,
            files: None,
        };

        let result = compute_file_scores(&modules, &file_paths, None, output, None).unwrap();
        // Top function has cognitive=0, so it should not be included
        assert!(!result.top_complex_fns.contains_key(&path_a));
    }

    // --- compute_crap_scores ---

    fn make_fn_complexity(cyclomatic: u16) -> fallow_types::extract::FunctionComplexity {
        fallow_types::extract::FunctionComplexity {
            name: "test_fn".into(),
            line: 1,
            col: 0,
            cyclomatic,
            cognitive: 0,
            line_count: 10,
            param_count: 0,
        }
    }

    #[test]
    fn crap_scores_empty_complexity() {
        let (max, above) = compute_crap_scores_binary(&[], true);
        assert!((max).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    #[test]
    fn crap_scores_test_reachable() {
        // Test-reachable: CRAP = CC, so CC=5 -> 5.0 (below threshold)
        let funcs = vec![make_fn_complexity(5)];
        let (max, above) = compute_crap_scores_binary(&funcs, true);
        assert!((max - 5.0).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    #[test]
    fn crap_scores_untested_at_threshold() {
        // Untested: CC=5 -> 5^2 + 5 = 30.0 (exactly at threshold, inclusive)
        let funcs = vec![make_fn_complexity(5)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        assert!((max - 30.0).abs() < f64::EPSILON);
        assert_eq!(above, 1);
    }

    #[test]
    fn crap_scores_untested_above_threshold() {
        // Untested: CC=6 -> 6^2 + 6 = 42.0
        let funcs = vec![make_fn_complexity(6)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        assert!((max - 42.0).abs() < f64::EPSILON);
        assert_eq!(above, 1);
    }

    #[test]
    fn crap_scores_untested_below_threshold() {
        // Untested: CC=4 -> 4^2 + 4 = 20.0 (below 30)
        let funcs = vec![make_fn_complexity(4)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        assert!((max - 20.0).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    #[test]
    fn crap_scores_mixed_functions_untested() {
        // Three untested functions: CC=2->6, CC=5->30, CC=8->72
        let funcs = vec![
            make_fn_complexity(2),
            make_fn_complexity(5),
            make_fn_complexity(8),
        ];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        assert!((max - 72.0).abs() < f64::EPSILON);
        // CC=5 (30.0) and CC=8 (72.0) are >= threshold
        assert_eq!(above, 2);
    }

    // --- crap_formula ---

    #[test]
    fn crap_formula_full_coverage() {
        // 100% coverage: CRAP = CC^2 * 0^3 + CC = CC
        let result = crap_formula(10.0, 100.0);
        assert!((result - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn crap_formula_zero_coverage() {
        // 0% coverage: CRAP = CC^2 * 1^3 + CC = CC^2 + CC
        let result = crap_formula(5.0, 0.0);
        assert!((result - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn crap_formula_partial_coverage() {
        // 50% coverage, CC=10: CRAP = 100 * 0.125 + 10 = 22.5
        let result = crap_formula(10.0, 50.0);
        assert!((result - 22.5).abs() < f64::EPSILON);
    }

    #[test]
    fn crap_formula_high_coverage_low_complexity() {
        // 90% coverage, CC=2: CRAP = 4 * 0.001 + 2 = 2.004
        let result = crap_formula(2.0, 90.0);
        assert!((result - 2.004).abs() < 0.001);
    }

    // --- compute_crap_scores_istanbul ---

    #[test]
    fn istanbul_crap_with_coverage_data() {
        let funcs = vec![make_fn_complexity(10)];
        let mut functions = rustc_hash::FxHashMap::default();
        // 80% coverage: CRAP = 100 * 0.008 + 10 = 10.8
        functions.insert(("test_fn".to_string(), 1), 80.0);
        let file_cov = IstanbulFileCoverage { functions };
        let result = compute_crap_scores_istanbul(&funcs, Some(&file_cov), false);
        assert!((result.max_crap - 10.8).abs() < 0.1);
        assert_eq!(result.above_threshold, 0);
    }

    #[test]
    fn istanbul_crap_falls_back_to_binary_when_no_match() {
        let funcs = vec![make_fn_complexity(6)];
        // Empty coverage: no function match, untested fallback: 6^2 + 6 = 42
        let file_cov = IstanbulFileCoverage {
            functions: rustc_hash::FxHashMap::default(),
        };
        let result = compute_crap_scores_istanbul(&funcs, Some(&file_cov), false);
        assert!((result.max_crap - 42.0).abs() < f64::EPSILON);
        assert_eq!(result.above_threshold, 1);
    }

    #[test]
    fn istanbul_crap_falls_back_to_binary_when_no_file_coverage() {
        let funcs = vec![make_fn_complexity(5)];
        // No file coverage at all, test-reachable: CRAP = CC = 5
        let result = compute_crap_scores_istanbul(&funcs, None, true);
        assert!((result.max_crap - 5.0).abs() < f64::EPSILON);
        assert_eq!(result.above_threshold, 0);
    }

    #[test]
    fn istanbul_crap_zero_coverage_matches_binary_untested() {
        let funcs = vec![make_fn_complexity(5)];
        let mut functions = rustc_hash::FxHashMap::default();
        functions.insert(("test_fn".to_string(), 1), 0.0);
        let file_cov = IstanbulFileCoverage { functions };
        // 0% coverage: CRAP = 25 * 1 + 5 = 30 (same as binary untested)
        let result = compute_crap_scores_istanbul(&funcs, Some(&file_cov), false);
        assert!((result.max_crap - 30.0).abs() < f64::EPSILON);
        assert_eq!(result.above_threshold, 1);
    }

    // --- compute_crap_scores_estimated ---

    #[test]
    fn estimated_crap_direct_test_reference() {
        // Function "test_fn" is directly test-referenced: 85% estimated coverage
        // CC=10: CRAP = 100 * (0.15)^3 + 10 = 100 * 0.003375 + 10 = 10.3375
        let funcs = vec![make_fn_complexity(10)];
        let mut refs = rustc_hash::FxHashSet::default();
        refs.insert("test_fn".to_string());
        let (max, above) = compute_crap_scores_estimated(&funcs, &refs, true);
        assert!((max - 10.3).abs() < 0.1);
        assert_eq!(above, 0);
    }

    #[test]
    fn estimated_crap_indirect_test_reachable() {
        // File is test-reachable but function not directly referenced: 40% estimated
        // CC=10: CRAP = 100 * (0.6)^3 + 10 = 100 * 0.216 + 10 = 31.6
        let funcs = vec![make_fn_complexity(10)];
        let refs = rustc_hash::FxHashSet::default();
        let (max, above) = compute_crap_scores_estimated(&funcs, &refs, true);
        assert!((max - 31.6).abs() < 0.1);
        assert_eq!(above, 1); // above threshold of 30
    }

    #[test]
    fn estimated_crap_untested_file() {
        // File not test-reachable, no refs: 0% coverage
        // CC=5: CRAP = 25 * 1 + 5 = 30
        let funcs = vec![make_fn_complexity(5)];
        let refs = rustc_hash::FxHashSet::default();
        let (max, above) = compute_crap_scores_estimated(&funcs, &refs, false);
        assert!((max - 30.0).abs() < f64::EPSILON);
        assert_eq!(above, 1);
    }

    #[test]
    fn estimated_crap_low_complexity_direct_ref() {
        // CC=2 with direct test ref (85%): CRAP = 4 * 0.003375 + 2 ≈ 2.0
        let funcs = vec![make_fn_complexity(2)];
        let mut refs = rustc_hash::FxHashSet::default();
        refs.insert("test_fn".to_string());
        let (max, above) = compute_crap_scores_estimated(&funcs, &refs, true);
        assert!(max < 3.0);
        assert_eq!(above, 0);
    }

    #[test]
    fn estimated_crap_empty() {
        let refs = rustc_hash::FxHashSet::default();
        let (max, above) = compute_crap_scores_estimated(&[], &refs, true);
        assert!((max).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    // --- dead_code_ratio: type-only export exclusion ---

    fn make_export(name: &str, is_type_only: bool) -> fallow_core::graph::ExportSymbol {
        fallow_core::graph::ExportSymbol {
            name: fallow_types::extract::ExportName::Named(name.into()),
            is_type_only,
            visibility: fallow_core::extract::VisibilityTag::None,
            span: oxc_span::Span::default(),
            references: vec![],
            members: vec![],
        }
    }

    #[test]
    fn dead_code_ratio_type_only_exports_excluded_from_denominator() {
        let path = std::path::Path::new("src/types.ts");
        let exports = vec![
            make_export("MyInterface", true),
            make_export("MyType", true),
            make_export("myFunction", false),
        ];
        let unused_files = rustc_hash::FxHashSet::default();
        let mut unused_by_path = rustc_hash::FxHashMap::default();
        unused_by_path.insert(path, 1_usize); // 1 unused value export

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_by_path);
        // 1 unused / 1 value export = 1.0 (type-only excluded from denominator)
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_only_type_exports_returns_zero() {
        let path = std::path::Path::new("src/types.ts");
        let exports = vec![
            make_export("MyInterface", true),
            make_export("MyType", true),
        ];
        let unused_files = rustc_hash::FxHashSet::default();
        let unused_by_path = rustc_hash::FxHashMap::default();

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_by_path);
        // No value exports -> 0.0
        assert!(ratio.abs() < f64::EPSILON);
    }

    #[test]
    fn dead_code_ratio_mixed_exports_counts_only_values() {
        let path = std::path::Path::new("src/component.ts");
        let exports = vec![
            make_export("Props", true),      // type-only, excluded
            make_export("State", true),      // type-only, excluded
            make_export("Component", false), // value
            make_export("helper", false),    // value
        ];
        let unused_files = rustc_hash::FxHashSet::default();
        let mut unused_by_path = rustc_hash::FxHashMap::default();
        unused_by_path.insert(path, 1_usize); // 1 unused value export

        let ratio = compute_dead_code_ratio(path, &exports, &unused_files, &unused_by_path);
        // 1 unused / 2 value exports = 0.5
        assert!((ratio - 0.5).abs() < f64::EPSILON);
    }

    // --- IstanbulFileCoverage::lookup ---

    #[test]
    fn istanbul_lookup_exact_match() {
        let mut functions = rustc_hash::FxHashMap::default();
        functions.insert(("handleClick".to_string(), 10), 85.0);
        let fc = IstanbulFileCoverage { functions };
        assert!((fc.lookup("handleClick", 10).unwrap() - 85.0).abs() < f64::EPSILON);
    }

    #[test]
    fn istanbul_lookup_fuzzy_match_within_offset() {
        let mut functions = rustc_hash::FxHashMap::default();
        functions.insert(("handleClick".to_string(), 10), 72.0);
        let fc = IstanbulFileCoverage { functions };
        // Line 11 is within offset of 2 from line 10
        assert!((fc.lookup("handleClick", 11).unwrap() - 72.0).abs() < f64::EPSILON);
        // Line 12 is within offset of 2
        assert!((fc.lookup("handleClick", 12).unwrap() - 72.0).abs() < f64::EPSILON);
    }

    #[test]
    fn istanbul_lookup_fuzzy_match_outside_offset() {
        let mut functions = rustc_hash::FxHashMap::default();
        functions.insert(("handleClick".to_string(), 10), 72.0);
        let fc = IstanbulFileCoverage { functions };
        // Line 13 is 3 away from line 10, exceeds offset of 2
        assert!(fc.lookup("handleClick", 13).is_none());
    }

    #[test]
    fn istanbul_lookup_name_mismatch() {
        let mut functions = rustc_hash::FxHashMap::default();
        functions.insert(("handleClick".to_string(), 10), 85.0);
        let fc = IstanbulFileCoverage { functions };
        assert!(fc.lookup("handleSubmit", 10).is_none());
    }

    #[test]
    fn istanbul_lookup_empty() {
        let fc = IstanbulFileCoverage {
            functions: rustc_hash::FxHashMap::default(),
        };
        assert!(fc.lookup("anything", 1).is_none());
    }

    #[test]
    fn istanbul_lookup_fuzzy_picks_closest() {
        let mut functions = rustc_hash::FxHashMap::default();
        // Two entries for same name at lines 8 and 12
        functions.insert(("render".to_string(), 8), 60.0);
        functions.insert(("render".to_string(), 12), 90.0);
        let fc = IstanbulFileCoverage { functions };
        // Looking up line 10: distance to 8 is 2, distance to 12 is 2
        // Both within offset, min_by_key picks closest (tie broken by iteration)
        let result = fc.lookup("render", 10);
        assert!(result.is_some());
        // Either match is acceptable since both are distance 2
        let pct = result.unwrap();
        assert!((pct - 60.0).abs() < f64::EPSILON || (pct - 90.0).abs() < f64::EPSILON);
    }

    // --- build_test_referenced_exports ---

    #[test]
    fn build_test_refs_empty() {
        let exports: Vec<fallow_core::graph::ExportSymbol> = vec![];
        let modules: Vec<fallow_core::graph::ModuleNode> = vec![];
        let refs = build_test_referenced_exports(&exports, &modules);
        assert!(refs.is_empty());
    }

    #[test]
    fn build_test_refs_empty_inputs() {
        let exports: Vec<fallow_core::graph::ExportSymbol> = vec![];
        let modules: Vec<fallow_core::graph::ModuleNode> = vec![];
        let refs = build_test_referenced_exports(&exports, &modules);
        assert!(refs.is_empty());
    }

    // --- compute_crap_scores_istanbul: additional edge cases ---

    #[test]
    fn istanbul_crap_empty_complexity() {
        let result = compute_crap_scores_istanbul(&[], None, false);
        assert!((result.max_crap).abs() < f64::EPSILON);
        assert_eq!(result.above_threshold, 0);
        assert_eq!(result.matched, 0);
        assert_eq!(result.total, 0);
    }

    #[test]
    fn istanbul_crap_match_statistics() {
        let funcs = vec![make_fn_complexity(5), {
            let mut f = make_fn_complexity(3);
            f.name = "other_fn".into();
            f.line = 10;
            f
        }];
        let mut functions = rustc_hash::FxHashMap::default();
        // Only match first function
        functions.insert(("test_fn".to_string(), 1), 80.0);
        let file_cov = IstanbulFileCoverage { functions };
        let result = compute_crap_scores_istanbul(&funcs, Some(&file_cov), true);
        assert_eq!(result.matched, 1);
        assert_eq!(result.total, 2);
    }

    // --- compute_crap_scores_estimated: multiple functions ---

    #[test]
    fn estimated_crap_multiple_functions_mixed_coverage() {
        let funcs = vec![
            make_fn_complexity(10), // name "test_fn" line 1
            {
                let mut f = make_fn_complexity(3);
                f.name = "helper".into();
                f.line = 20;
                f
            },
        ];
        let mut refs = rustc_hash::FxHashSet::default();
        refs.insert("test_fn".to_string()); // Only test_fn is directly referenced
        let (max, above) = compute_crap_scores_estimated(&funcs, &refs, true);
        // test_fn: CC=10, 85% coverage -> CRAP ~10.3
        // helper: CC=3, 40% coverage (indirect) -> CRAP = 9*0.216+3 = 4.944
        assert!(max > 10.0);
        assert_eq!(above, 0); // Neither exceeds 30
    }

    // --- compute_crap_scores_binary ---

    #[test]
    fn binary_crap_test_reachable() {
        let funcs = vec![make_fn_complexity(10)];
        let (max, above) = compute_crap_scores_binary(&funcs, true);
        // Test-reachable: CRAP = CC = 10
        assert!((max - 10.0).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    #[test]
    fn binary_crap_not_reachable() {
        let funcs = vec![make_fn_complexity(6)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        // Not test-reachable: CRAP = 36 + 6 = 42
        assert!((max - 42.0).abs() < f64::EPSILON);
        assert_eq!(above, 1);
    }

    #[test]
    fn binary_crap_threshold_boundary() {
        // CC=5 untested: 25 + 5 = 30 (exactly at threshold)
        let funcs = vec![make_fn_complexity(5)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        assert!((max - 30.0).abs() < f64::EPSILON);
        assert_eq!(above, 1); // >= threshold
    }

    #[test]
    fn binary_crap_empty() {
        let (max, above) = compute_crap_scores_binary(&[], true);
        assert!((max).abs() < f64::EPSILON);
        assert_eq!(above, 0);
    }

    #[test]
    fn binary_crap_multiple_functions() {
        let funcs = vec![make_fn_complexity(3), make_fn_complexity(8)];
        let (max, above) = compute_crap_scores_binary(&funcs, false);
        // CC=3: 9+3=12 (below threshold), CC=8: 64+8=72 (above threshold)
        assert!((max - 72.0).abs() < f64::EPSILON);
        assert_eq!(above, 1);
    }
}
