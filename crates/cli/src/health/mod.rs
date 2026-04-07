mod hotspots;
mod scoring;
mod targets;

use std::process::ExitCode;
use std::time::{Duration, Instant};

use fallow_config::{OutputFormat, ResolvedConfig};

use crate::baseline::{HealthBaselineData, filter_new_health_findings, filter_new_health_targets};
use crate::check::{get_changed_files, resolve_workspace_filter};
use crate::error::emit_error;
pub use crate::health_types::*;
use crate::load_config;
use crate::report;
use crate::vital_signs;

use hotspots::compute_hotspots;
use scoring::compute_file_scores;
use targets::{TargetAuxData, compute_refactoring_targets};

/// Sort criteria for complexity output.
#[derive(Clone, clap::ValueEnum)]
pub enum SortBy {
    Cyclomatic,
    Cognitive,
    Lines,
}

pub struct HealthOptions<'a> {
    pub root: &'a std::path::Path,
    pub config_path: &'a Option<std::path::PathBuf>,
    pub output: OutputFormat,
    pub no_cache: bool,
    pub threads: usize,
    pub quiet: bool,
    pub max_cyclomatic: Option<u16>,
    pub max_cognitive: Option<u16>,
    pub top: Option<usize>,
    pub sort: SortBy,
    pub production: bool,
    pub changed_since: Option<&'a str>,
    pub workspace: Option<&'a str>,
    pub baseline: Option<&'a std::path::Path>,
    pub save_baseline: Option<&'a std::path::Path>,
    pub complexity: bool,
    pub file_scores: bool,
    pub coverage_gaps: bool,
    pub hotspots: bool,
    pub targets: bool,
    pub effort: Option<EffortEstimate>,
    pub score: bool,
    pub min_score: Option<f64>,
    pub since: Option<&'a str>,
    pub min_commits: Option<u32>,
    pub explain: bool,
    /// When true, emit a condensed summary instead of full item-level output.
    #[allow(
        dead_code,
        reason = "wired from CLI but consumed by combined mode, not standalone health"
    )]
    pub summary: bool,
    pub save_snapshot: Option<std::path::PathBuf>,
    pub trend: bool,
    pub group_by: Option<crate::GroupBy>,
}

/// Run health analysis and return results without printing.
pub fn execute_health(opts: &HealthOptions<'_>) -> Result<HealthResult, ExitCode> {
    let start = Instant::now();

    let config = load_config(
        opts.root,
        opts.config_path,
        opts.output,
        opts.no_cache,
        opts.threads,
        opts.production,
        opts.quiet,
    )?;

    // Resolve thresholds: CLI flags override config
    let max_cyclomatic = opts.max_cyclomatic.unwrap_or(config.health.max_cyclomatic);
    let max_cognitive = opts.max_cognitive.unwrap_or(config.health.max_cognitive);

    // Discover and parse files
    let files = fallow_core::discover::discover_files(&config);
    let cache = if config.no_cache {
        None
    } else {
        fallow_core::cache::CacheStore::load(&config.cache_dir)
    };
    let parse_result = fallow_core::extract::parse_all_files(&files, cache.as_ref(), true);

    let ignore_set = build_ignore_set(&config.health.ignore);
    let changed_files = opts
        .changed_since
        .and_then(|git_ref| get_changed_files(opts.root, git_ref));
    let ws_root = if let Some(ws_name) = opts.workspace {
        Some(resolve_workspace_filter(opts.root, ws_name, opts.output)?)
    } else {
        None
    };

    // Build FileId -> path lookup for O(1) access
    let file_paths: rustc_hash::FxHashMap<_, _> = files.iter().map(|f| (f.id, &f.path)).collect();

    // Collect and filter complexity findings
    let (mut findings, files_analyzed, total_functions) = collect_findings(
        &parse_result.modules,
        &file_paths,
        &config.root,
        &ignore_set,
        changed_files.as_ref(),
        max_cyclomatic,
        max_cognitive,
    );
    if let Some(ref ws) = ws_root {
        findings.retain(|f| f.path.starts_with(ws));
    }
    sort_findings(&mut findings, &opts.sort);
    let total_above_threshold = findings.len();

    // Load baseline for filtering (save happens after targets are computed)
    let loaded_baseline = if let Some(load_path) = opts.baseline {
        Some(load_health_baseline(
            load_path,
            &mut findings,
            &config.root,
            opts.output,
        )?)
    } else {
        None
    };
    if let Some(top) = opts.top {
        findings.truncate(top);
    }

    // --coverage-gaps flag overrides Off severity (explicit user intent).
    // Without the flag, coverage gaps only activate when config severity is not Off.
    let effective_coverage_gaps = opts.coverage_gaps;

    // Compute file-level health scores (needed by hotspots and targets too)
    let needs_file_scores =
        opts.file_scores || effective_coverage_gaps || opts.hotspots || opts.targets;
    let (score_output, files_scored, average_maintainability) = if needs_file_scores {
        compute_filtered_file_scores(
            &config,
            &parse_result.modules,
            &file_paths,
            changed_files.as_ref(),
            ws_root.as_deref(),
            &ignore_set,
            opts.output,
        )?
    } else {
        (None, None, None)
    };

    let file_scores_slice = score_output
        .as_ref()
        .map_or(&[] as &[_], |o| o.scores.as_slice());

    // Compute hotspot analysis when requested (or when targets need churn data)
    let (hotspots, hotspot_summary) = if opts.hotspots || opts.targets {
        compute_hotspots(
            opts,
            &config,
            file_scores_slice,
            &ignore_set,
            ws_root.as_deref(),
        )
    } else {
        (Vec::new(), None)
    };

    // Compute refactoring targets
    let (targets, target_thresholds) = compute_targets(
        opts,
        score_output.as_ref(),
        file_scores_slice,
        &hotspots,
        loaded_baseline.as_ref(),
        &config.root,
    );

    if let Some(save_path) = opts.save_baseline {
        save_health_baseline(
            save_path,
            &findings,
            &targets,
            &config.root,
            opts.quiet,
            opts.output,
        )?;
    }

    // Compute vital signs (always needed for report summary)
    let (vital_signs, counts) = compute_vital_signs_and_counts(
        score_output.as_ref(),
        &parse_result.modules,
        needs_file_scores,
        file_scores_slice,
        opts.hotspots || opts.targets,
        &hotspots,
        files.len(),
    );

    let health_score = if opts.score {
        Some(vital_signs::compute_health_score(&vital_signs, files.len()))
    } else {
        None
    };

    if let Some(ref snapshot_path) = opts.save_snapshot {
        save_snapshot(
            opts,
            snapshot_path,
            &vital_signs,
            &counts,
            hotspot_summary.as_ref(),
            health_score.as_ref(),
        )?;
    }

    let health_trend = compute_health_trend(opts, &vital_signs, &counts, health_score.as_ref());

    // Assemble final report
    let report = assemble_health_report(
        opts,
        effective_coverage_gaps,
        findings,
        files_analyzed,
        total_functions,
        total_above_threshold,
        max_cyclomatic,
        max_cognitive,
        files_scored,
        average_maintainability,
        vital_signs,
        health_score,
        score_output,
        hotspots,
        hotspot_summary,
        targets,
        target_thresholds,
        health_trend,
    );

    Ok(HealthResult {
        report,
        config,
        elapsed: start.elapsed(),
    })
}

/// Sort findings by the specified criteria.
fn sort_findings(findings: &mut [HealthFinding], sort: &SortBy) {
    match sort {
        SortBy::Cyclomatic => findings.sort_by(|a, b| b.cyclomatic.cmp(&a.cyclomatic)),
        SortBy::Cognitive => findings.sort_by(|a, b| b.cognitive.cmp(&a.cognitive)),
        SortBy::Lines => findings.sort_by(|a, b| b.line_count.cmp(&a.line_count)),
    }
}

/// `(score_output, files_scored, average_maintainability)`.
type FileScoreResult = (Option<scoring::FileScoreOutput>, Option<usize>, Option<f64>);

/// Compute file scores, applying workspace and ignore filters.
fn compute_filtered_file_scores(
    config: &ResolvedConfig,
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    ws_root: Option<&std::path::Path>,
    ignore_set: &globset::GlobSet,
    output: OutputFormat,
) -> Result<FileScoreResult, ExitCode> {
    let analysis_output = fallow_core::analyze_with_parse_result(config, modules)
        .map_err(|e| emit_error(&format!("analysis failed: {e}"), 2, output))?;
    match compute_file_scores(modules, file_paths, changed_files, analysis_output) {
        Ok(mut output) => {
            if let Some(ws) = ws_root {
                output.scores.retain(|s| s.path.starts_with(ws));
            }
            if !ignore_set.is_empty() {
                output.scores.retain(|s| {
                    let relative = s.path.strip_prefix(&config.root).unwrap_or(&s.path);
                    !ignore_set.is_match(relative)
                });
            }
            filter_coverage_gaps(
                &mut output.coverage.report,
                &mut output.coverage.runtime_paths,
                config,
                changed_files,
                ws_root,
                ignore_set,
            );
            // Compute average BEFORE --top truncation so it reflects the full project
            let total_scored = output.scores.len();
            let avg = if total_scored > 0 {
                let sum: f64 = output.scores.iter().map(|s| s.maintainability_index).sum();
                Some((sum / total_scored as f64 * 10.0).round() / 10.0)
            } else {
                None
            };
            Ok((Some(output), Some(total_scored), avg))
        }
        Err(e) => {
            eprintln!("Warning: failed to compute file scores: {e}");
            Ok((None, Some(0), None))
        }
    }
}

/// Compute refactoring targets when requested, applying baseline and top filters.
fn compute_targets(
    opts: &HealthOptions<'_>,
    score_output: Option<&scoring::FileScoreOutput>,
    file_scores_slice: &[FileHealthScore],
    hotspots: &[HotspotEntry],
    loaded_baseline: Option<&HealthBaselineData>,
    config_root: &std::path::Path,
) -> (Vec<RefactoringTarget>, Option<TargetThresholds>) {
    if !opts.targets {
        return (Vec::new(), None);
    }
    let Some(output) = score_output else {
        return (Vec::new(), None);
    };
    let target_aux = TargetAuxData::from(output);
    let (mut tgts, thresholds) =
        compute_refactoring_targets(file_scores_slice, &target_aux, hotspots);
    if let Some(baseline) = loaded_baseline {
        tgts = filter_new_health_targets(tgts, baseline, config_root);
    }
    if let Some(ref effort) = opts.effort {
        tgts.retain(|t| t.effort == *effort);
    }
    if let Some(top) = opts.top {
        tgts.truncate(top);
    }
    (tgts, Some(thresholds))
}

fn path_in_health_scope(
    path: &std::path::Path,
    config: &ResolvedConfig,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    ws_root: Option<&std::path::Path>,
    ignore_set: &globset::GlobSet,
) -> bool {
    if let Some(changed) = changed_files
        && !changed.contains(path)
    {
        return false;
    }
    if let Some(ws) = ws_root
        && !path.starts_with(ws)
    {
        return false;
    }
    if !ignore_set.is_empty() {
        let relative = path.strip_prefix(&config.root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            return false;
        }
    }
    true
}

fn filter_coverage_gaps(
    coverage_gaps: &mut CoverageGaps,
    runtime_paths: &mut Vec<std::path::PathBuf>,
    config: &ResolvedConfig,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    ws_root: Option<&std::path::Path>,
    ignore_set: &globset::GlobSet,
) {
    runtime_paths
        .retain(|path| path_in_health_scope(path, config, changed_files, ws_root, ignore_set));
    coverage_gaps.files.retain(|item| {
        path_in_health_scope(&item.path, config, changed_files, ws_root, ignore_set)
    });
    coverage_gaps.exports.retain(|item| {
        path_in_health_scope(&item.path, config, changed_files, ws_root, ignore_set)
    });

    runtime_paths.sort();
    runtime_paths.dedup();

    let runtime_files = runtime_paths.len();
    let untested_files = coverage_gaps.files.len();
    let covered_files = runtime_files.saturating_sub(untested_files);
    coverage_gaps.summary = scoring::build_coverage_summary(
        runtime_files,
        covered_files,
        untested_files,
        coverage_gaps.exports.len(),
    );
}

/// Build vital signs and counts from available analysis data.
fn compute_vital_signs_and_counts(
    score_output: Option<&scoring::FileScoreOutput>,
    modules: &[fallow_core::extract::ModuleInfo],
    needs_file_scores: bool,
    file_scores_slice: &[FileHealthScore],
    needs_hotspots: bool,
    hotspots: &[HotspotEntry],
    total_files: usize,
) -> (
    crate::health_types::VitalSigns,
    crate::health_types::VitalSignsCounts,
) {
    let analysis_counts = score_output.map(|o| crate::vital_signs::AnalysisCounts {
        total_exports: o.analysis_counts.total_exports,
        dead_files: o.analysis_counts.dead_files,
        dead_exports: o.analysis_counts.dead_exports,
        unused_deps: o.analysis_counts.unused_deps,
        circular_deps: o.analysis_counts.circular_deps,
        total_deps: o.analysis_counts.total_deps,
    });
    let vs_input = vital_signs::VitalSignsInput {
        modules,
        file_scores: if needs_file_scores {
            Some(file_scores_slice)
        } else {
            None
        },
        // Some(&[]) when pipeline ran but returned 0 results (-> hotspot_count: 0),
        // None when pipeline was not invoked (-> hotspot_count: null in snapshot).
        hotspots: if needs_hotspots { Some(hotspots) } else { None },
        total_files,
        analysis_counts,
    };
    let signs = vital_signs::compute_vital_signs(&vs_input);
    let counts = vital_signs::build_counts(&vs_input);
    (signs, counts)
}

/// Save a vital signs snapshot to disk if requested.
fn save_snapshot(
    opts: &HealthOptions<'_>,
    snapshot_path: &std::path::Path,
    vital_signs: &crate::health_types::VitalSigns,
    counts: &crate::health_types::VitalSignsCounts,
    hotspot_summary: Option<&crate::health_types::HotspotSummary>,
    health_score: Option<&crate::health_types::HealthScore>,
) -> Result<(), ExitCode> {
    let shallow = hotspot_summary.is_some_and(|s| s.shallow_clone);
    let snapshot = vital_signs::build_snapshot(
        vital_signs.clone(),
        counts.clone(),
        opts.root,
        shallow,
        health_score,
    );
    let explicit = if snapshot_path.as_os_str().is_empty() {
        None
    } else {
        Some(snapshot_path)
    };
    match vital_signs::save_snapshot(&snapshot, opts.root, explicit) {
        Ok(saved_path) => {
            if !opts.quiet {
                eprintln!("Saved vital signs snapshot to {}", saved_path.display());
            }
            Ok(())
        }
        Err(e) => Err(emit_error(&e, 2, opts.output)),
    }
}

/// Compute health trend from historical snapshots if requested.
fn compute_health_trend(
    opts: &HealthOptions<'_>,
    vital_signs: &crate::health_types::VitalSigns,
    counts: &crate::health_types::VitalSignsCounts,
    health_score: Option<&crate::health_types::HealthScore>,
) -> Option<crate::health_types::HealthTrend> {
    if !opts.trend {
        return None;
    }
    if opts.changed_since.is_some() && !opts.quiet {
        eprintln!(
            "warning: --trend comparison may be inaccurate with --changed-since; \
             snapshots are typically from full-project runs"
        );
    }
    let snapshots = vital_signs::load_snapshots(opts.root);
    if snapshots.is_empty() && !opts.quiet {
        eprintln!(
            "No snapshots found. Run `fallow health --save-snapshot` to save a \
             baseline, then use --trend on subsequent runs to track progress."
        );
    }
    vital_signs::compute_trend(
        vital_signs,
        counts,
        health_score.map(|s| s.score),
        &snapshots,
    )
}

/// Assemble the final `HealthReport` from all computed data.
#[expect(
    clippy::too_many_arguments,
    reason = "assembles report from many computed pieces"
)]
fn assemble_health_report(
    opts: &HealthOptions<'_>,
    effective_coverage_gaps: bool,
    findings: Vec<HealthFinding>,
    files_analyzed: usize,
    total_functions: usize,
    total_above_threshold: usize,
    max_cyclomatic: u16,
    max_cognitive: u16,
    files_scored: Option<usize>,
    average_maintainability: Option<f64>,
    vital_signs: crate::health_types::VitalSigns,
    health_score: Option<crate::health_types::HealthScore>,
    score_output: Option<scoring::FileScoreOutput>,
    hotspots: Vec<HotspotEntry>,
    hotspot_summary: Option<crate::health_types::HotspotSummary>,
    targets: Vec<RefactoringTarget>,
    target_thresholds: Option<TargetThresholds>,
    health_trend: Option<crate::health_types::HealthTrend>,
) -> HealthReport {
    let coverage_gaps = if effective_coverage_gaps {
        score_output.as_ref().map(|o| o.coverage.report.clone())
    } else {
        None
    };

    // Extract file scores for the report (apply --top after hotspot/target computation)
    let file_scores = if opts.file_scores {
        let mut scores = score_output.map(|o| o.scores).unwrap_or_default();
        if let Some(top) = opts.top {
            scores.truncate(top);
        }
        scores
    } else {
        Vec::new()
    };

    // If hotspots were only computed for targets, don't include them in the report
    let (report_hotspots, report_hotspot_summary) = if opts.hotspots {
        (hotspots, hotspot_summary)
    } else {
        (Vec::new(), None)
    };

    HealthReport {
        summary: HealthSummary {
            files_analyzed,
            functions_analyzed: total_functions,
            functions_above_threshold: total_above_threshold,
            max_cyclomatic_threshold: max_cyclomatic,
            max_cognitive_threshold: max_cognitive,
            files_scored: if opts.file_scores { files_scored } else { None },
            average_maintainability: if opts.file_scores {
                average_maintainability
            } else {
                None
            },
            coverage_model: if opts.file_scores
                || effective_coverage_gaps
                || opts.hotspots
                || opts.targets
            {
                Some(crate::health_types::CoverageModel::StaticBinary)
            } else {
                None
            },
        },
        vital_signs: Some(vital_signs),
        health_score,
        findings: if opts.complexity {
            findings
        } else {
            Vec::new()
        },
        file_scores,
        coverage_gaps,
        hotspots: report_hotspots,
        hotspot_summary: report_hotspot_summary,
        targets,
        target_thresholds,
        health_trend,
    }
}

/// Build a glob set from health ignore patterns.
fn build_ignore_set(patterns: &[String]) -> globset::GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        match globset::Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(e) => {
                eprintln!("Warning: Invalid health ignore pattern '{pattern}': {e}");
            }
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| globset::GlobSet::empty())
}

/// Collect health findings from parsed modules, applying ignore and changed-since filters.
fn collect_findings(
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    config_root: &std::path::Path,
    ignore_set: &globset::GlobSet,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    max_cyclomatic: u16,
    max_cognitive: u16,
) -> (Vec<HealthFinding>, usize, usize) {
    let mut files_analyzed = 0usize;
    let mut total_functions = 0usize;
    let mut findings: Vec<HealthFinding> = Vec::new();

    for module in modules {
        let Some(&path) = file_paths.get(&module.file_id) else {
            continue;
        };

        let relative = path.strip_prefix(config_root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            continue;
        }

        if let Some(changed) = changed_files
            && !changed.contains(path)
        {
            continue;
        }

        files_analyzed += 1;
        for fc in &module.complexity {
            total_functions += 1;
            let exceeds_cyclomatic = fc.cyclomatic > max_cyclomatic;
            let exceeds_cognitive = fc.cognitive > max_cognitive;
            if exceeds_cyclomatic || exceeds_cognitive {
                let exceeded = match (exceeds_cyclomatic, exceeds_cognitive) {
                    (true, true) => ExceededThreshold::Both,
                    (true, false) => ExceededThreshold::Cyclomatic,
                    (false, true) => ExceededThreshold::Cognitive,
                    (false, false) => unreachable!(),
                };
                findings.push(HealthFinding {
                    path: path.clone(),
                    name: fc.name.clone(),
                    line: fc.line,
                    col: fc.col,
                    cyclomatic: fc.cyclomatic,
                    cognitive: fc.cognitive,
                    line_count: fc.line_count,
                    exceeded,
                });
            }
        }
    }

    (findings, files_analyzed, total_functions)
}

/// Save health baseline to disk.
fn save_health_baseline(
    save_path: &std::path::Path,
    findings: &[HealthFinding],
    targets: &[RefactoringTarget],
    config_root: &std::path::Path,
    quiet: bool,
    output: OutputFormat,
) -> Result<(), ExitCode> {
    let baseline = HealthBaselineData::from_findings(findings, targets, config_root);
    match serde_json::to_string_pretty(&baseline) {
        Ok(json) => {
            if let Err(e) = std::fs::write(save_path, json) {
                return Err(emit_error(
                    &format!("failed to save health baseline: {e}"),
                    2,
                    output,
                ));
            }
            if !quiet {
                eprintln!("Saved health baseline to {}", save_path.display());
            }
            Ok(())
        }
        Err(e) => Err(emit_error(
            &format!("failed to serialize health baseline: {e}"),
            2,
            output,
        )),
    }
}

/// Load and apply a health baseline, filtering findings to show only new ones.
fn load_health_baseline(
    baseline_path: &std::path::Path,
    findings: &mut Vec<HealthFinding>,
    root: &std::path::Path,
    output: OutputFormat,
) -> Result<HealthBaselineData, ExitCode> {
    let json = std::fs::read_to_string(baseline_path)
        .map_err(|e| emit_error(&format!("failed to read health baseline: {e}"), 2, output))?;
    let baseline: HealthBaselineData = serde_json::from_str(&json)
        .map_err(|e| emit_error(&format!("failed to parse health baseline: {e}"), 2, output))?;
    *findings = filter_new_health_findings(std::mem::take(findings), &baseline, root);
    Ok(baseline)
}

/// Run health analysis, print results, and return exit code.
pub fn run_health(opts: &HealthOptions<'_>) -> ExitCode {
    let result = match execute_health(opts) {
        Ok(r) => r,
        Err(code) => return code,
    };
    // Build resolver for --group-by (passed through to report context)
    let _resolver = match crate::build_ownership_resolver(
        opts.group_by,
        opts.root,
        result.config.codeowners.as_deref(),
        opts.output,
    ) {
        Ok(r) => r,
        Err(code) => return code,
    };
    // Health grouping is a follow-up — for now, validate the flag and pass None
    print_health_result(
        &result,
        opts.quiet,
        opts.explain,
        opts.min_score,
        opts.summary,
    )
}

/// Result of executing health analysis without printing.
pub struct HealthResult {
    pub report: HealthReport,
    pub config: ResolvedConfig,
    pub elapsed: Duration,
}

/// Print health results and return appropriate exit code.
pub fn print_health_result(
    result: &HealthResult,
    quiet: bool,
    explain: bool,
    min_score: Option<f64>,
    summary: bool,
) -> ExitCode {
    let ctx = report::ReportContext {
        root: &result.config.root,
        rules: &result.config.rules,
        elapsed: result.elapsed,
        quiet,
        explain,
        group_by: None,
        top: None,
        summary,
    };
    let report_code = report::print_health_report(&result.report, &ctx, result.config.output);
    if report_code != ExitCode::SUCCESS {
        return report_code;
    }

    // Check --min-score threshold
    if let Some(threshold) = min_score
        && let Some(ref hs) = result.report.health_score
        && hs.score < threshold
    {
        if !quiet {
            eprintln!(
                "Health score {:.1} ({}) is below minimum threshold {:.0}",
                hs.score, hs.grade, threshold
            );
        }
        return ExitCode::from(1);
    }

    if !result.report.findings.is_empty() {
        return ExitCode::from(1);
    }

    if result.config.rules.coverage_gaps == fallow_config::Severity::Error
        && result
            .report
            .coverage_gaps
            .as_ref()
            .is_some_and(|gaps| !gaps.is_empty())
    {
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use fallow_core::extract::ModuleInfo;
    use fallow_types::discover::FileId;
    use fallow_types::extract::FunctionComplexity;
    use rustc_hash::{FxHashMap, FxHashSet};
    use std::path::{Path, PathBuf};

    /// Build a minimal `ModuleInfo` with only the fields `collect_findings` needs.
    fn make_module(file_id: FileId, complexity: Vec<FunctionComplexity>) -> ModuleInfo {
        ModuleInfo {
            file_id,
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
            line_offsets: vec![0],
            complexity,
        }
    }

    fn make_fc(name: &str, cyclomatic: u16, cognitive: u16, line_count: u32) -> FunctionComplexity {
        FunctionComplexity {
            name: name.to_string(),
            line: 1,
            col: 0,
            cyclomatic,
            cognitive,
            line_count,
        }
    }

    // ── build_ignore_set ────────────────────────────────────────

    #[test]
    fn build_ignore_set_empty_patterns() {
        let set = build_ignore_set(&[]);
        assert!(set.is_empty());
    }

    #[test]
    fn build_ignore_set_matches_glob() {
        let patterns = vec!["src/generated/**".to_string()];
        let set = build_ignore_set(&patterns);
        assert!(set.is_match(Path::new("src/generated/types.ts")));
        assert!(!set.is_match(Path::new("src/utils.ts")));
    }

    #[test]
    fn build_ignore_set_multiple_patterns() {
        let patterns = vec!["*.test.ts".to_string(), "dist/**".to_string()];
        let set = build_ignore_set(&patterns);
        assert!(set.is_match(Path::new("foo.test.ts")));
        assert!(set.is_match(Path::new("dist/index.js")));
        assert!(!set.is_match(Path::new("src/index.ts")));
    }

    #[test]
    fn build_ignore_set_skips_invalid_patterns() {
        // "[invalid" is not a valid glob — should be skipped, not panic
        let patterns = vec!["[invalid".to_string(), "*.js".to_string()];
        let set = build_ignore_set(&patterns);
        // The valid pattern should still work
        assert!(set.is_match(Path::new("foo.js")));
    }

    // ── collect_findings ────────────────────────────────────────

    #[test]
    fn collect_findings_empty_modules() {
        let (findings, files, functions) = collect_findings(
            &[],
            &FxHashMap::default(),
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert!(findings.is_empty());
        assert_eq!(files, 0);
        assert_eq!(functions, 0);
    }

    #[test]
    fn collect_findings_below_threshold() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(FileId(0), vec![make_fc("doStuff", 5, 3, 10)])];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, files, functions) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert!(findings.is_empty());
        assert_eq!(files, 1);
        assert_eq!(functions, 1);
    }

    #[test]
    fn collect_findings_exceeds_cyclomatic_only() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(
            FileId(0),
            vec![make_fc("complexFn", 25, 5, 50)],
        )];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, _, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].cyclomatic, 25);
        assert!(matches!(
            findings[0].exceeded,
            ExceededThreshold::Cyclomatic
        ));
    }

    #[test]
    fn collect_findings_exceeds_cognitive_only() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(FileId(0), vec![make_fc("nestedFn", 5, 20, 30)])];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, _, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].exceeded, ExceededThreshold::Cognitive));
    }

    #[test]
    fn collect_findings_exceeds_both() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(
            FileId(0),
            vec![make_fc("terribleFn", 25, 20, 100)],
        )];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, _, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert_eq!(findings.len(), 1);
        assert!(matches!(findings[0].exceeded, ExceededThreshold::Both));
    }

    #[test]
    fn collect_findings_multiple_functions_per_file() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(
            FileId(0),
            vec![
                make_fc("ok", 5, 3, 10),
                make_fc("bad", 25, 20, 50),
                make_fc("also_bad", 21, 5, 30),
            ],
        )];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, files, functions) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert_eq!(findings.len(), 2);
        assert_eq!(files, 1);
        assert_eq!(functions, 3);
    }

    #[test]
    fn collect_findings_ignores_matching_files() {
        let path = PathBuf::from("/project/src/generated/types.ts");
        let modules = vec![make_module(FileId(0), vec![make_fc("genFn", 25, 20, 50)])];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let ignore_set = build_ignore_set(&["src/generated/**".to_string()]);
        let (findings, files, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &ignore_set,
            None,
            20,
            15,
        );
        assert!(findings.is_empty());
        assert_eq!(files, 0);
    }

    #[test]
    fn collect_findings_filters_by_changed_files() {
        let path_a = PathBuf::from("/project/src/a.ts");
        let path_b = PathBuf::from("/project/src/b.ts");
        let modules = vec![
            make_module(FileId(0), vec![make_fc("fnA", 25, 20, 50)]),
            make_module(FileId(1), vec![make_fc("fnB", 25, 20, 50)]),
        ];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path_a);
        file_paths.insert(FileId(1), &path_b);

        let mut changed = FxHashSet::default();
        changed.insert(PathBuf::from("/project/src/a.ts"));

        let (findings, files, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            Some(&changed),
            20,
            15,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "fnA");
        assert_eq!(files, 1);
    }

    #[test]
    fn collect_findings_skips_module_without_path() {
        // Module with FileId(99) has no entry in file_paths
        let modules = vec![make_module(FileId(99), vec![make_fc("orphan", 25, 20, 50)])];
        let file_paths = FxHashMap::default();

        let (findings, files, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert!(findings.is_empty());
        assert_eq!(files, 0);
    }

    #[test]
    fn collect_findings_at_exact_threshold_not_reported() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(
            FileId(0),
            // Exactly at thresholds — should NOT be reported (> not >=)
            vec![make_fc("borderline", 20, 15, 20)],
        )];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, _, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn collect_findings_preserves_function_metadata() {
        let path = PathBuf::from("/project/src/a.ts");
        let modules = vec![make_module(
            FileId(0),
            vec![FunctionComplexity {
                name: "processData".to_string(),
                line: 42,
                col: 8,
                cyclomatic: 25,
                cognitive: 18,
                line_count: 75,
            }],
        )];
        let mut file_paths = FxHashMap::default();
        file_paths.insert(FileId(0), &path);

        let (findings, _, _) = collect_findings(
            &modules,
            &file_paths,
            Path::new("/project"),
            &globset::GlobSet::empty(),
            None,
            20,
            15,
        );
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.name, "processData");
        assert_eq!(f.line, 42);
        assert_eq!(f.col, 8);
        assert_eq!(f.cyclomatic, 25);
        assert_eq!(f.cognitive, 18);
        assert_eq!(f.line_count, 75);
        assert_eq!(f.path, PathBuf::from("/project/src/a.ts"));
    }
}
