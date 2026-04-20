pub mod coverage;
mod hotspots;
pub mod ownership;
mod scoring;
mod targets;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use colored::Colorize;
use fallow_config::{OutputFormat, ResolvedConfig};
use rustc_hash::FxHashSet;

use crate::baseline::{
    HealthBaselineData, filter_new_health_findings, filter_new_health_targets,
    filter_new_production_coverage_findings,
};
use crate::check::{get_changed_files, resolve_workspace_scope};
use crate::error::emit_error;
pub use crate::health_types::*;
use crate::load_config;
use crate::report;
use crate::vital_signs;

use hotspots::compute_hotspots;
use scoring::compute_file_scores;

/// Pre-parsed data from the dead-code pipeline, shared with health to avoid re-analysis.
pub struct SharedParseData {
    pub files: Vec<fallow_types::discover::DiscoveredFile>,
    pub modules: Vec<fallow_types::extract::ModuleInfo>,
    /// Full analysis output (graph + results) for file scoring.
    pub analysis_output: Option<fallow_core::AnalysisOutput>,
}
use targets::{TargetAuxData, compute_refactoring_targets};

pub struct ProductionCoverageOptions {
    pub path: std::path::PathBuf,
    pub min_invocations_hot: u64,
    /// Minimum total trace volume before high-confidence `safe_to_delete` /
    /// `review_required` verdicts may be emitted. Below this the sidecar caps
    /// confidence at `medium`. `None` lets the sidecar use its spec-default
    /// (5000).
    pub min_observation_volume: Option<u32>,
    /// Fraction of total trace count below which an invoked function is
    /// classified as `low_traffic` rather than `active`. `None` lets the
    /// sidecar use its spec-default (0.001 = 0.1%).
    pub low_traffic_threshold: Option<f64>,
    pub license_jwt: String,
    pub watermark: Option<crate::health_types::ProductionCoverageWatermark>,
}

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
    pub workspace: Option<&'a [String]>,
    pub changed_workspaces: Option<&'a str>,
    pub baseline: Option<&'a std::path::Path>,
    pub save_baseline: Option<&'a std::path::Path>,
    pub complexity: bool,
    pub file_scores: bool,
    /// Explicitly include coverage gaps in the rendered report.
    pub coverage_gaps: bool,
    /// Allow config severity to enable coverage gap reporting when the caller
    /// did not explicitly select health sections.
    pub config_activates_coverage_gaps: bool,
    pub hotspots: bool,
    pub ownership: bool,
    pub ownership_emails: Option<fallow_config::EmailMode>,
    pub targets: bool,
    /// Run the full health pipeline even if some sections are hidden, so score
    /// and snapshot outputs stay accurate.
    pub force_full: bool,
    /// Render only the score/trend-oriented output, hiding supporting sections
    /// that were computed solely for score accuracy.
    pub score_only_output: bool,
    /// Enforce the configured coverage-gap severity as a failing quality gate.
    pub enforce_coverage_gap_gate: bool,
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
    /// Path to Istanbul-format coverage data (coverage-final.json) for accurate CRAP scores.
    pub coverage: Option<&'a std::path::Path>,
    /// Rebase file paths in coverage data by stripping this prefix and prepending project root.
    pub coverage_root: Option<&'a std::path::Path>,
    /// Show detailed pipeline timing breakdown.
    pub performance: bool,
    /// Only exit with error for findings at or above this severity level.
    pub min_severity: Option<FindingSeverity>,
    /// Paid production coverage sidecar input.
    pub production_coverage: Option<ProductionCoverageOptions>,
}

/// Run health analysis using pre-parsed modules from the dead-code pipeline.
///
/// Skips file discovery and parsing (saves ~1.9s on 21K-file projects).
pub fn execute_health_with_shared_parse(
    opts: &HealthOptions<'_>,
    shared: SharedParseData,
) -> Result<HealthResult, ExitCode> {
    let config = load_config(
        opts.root,
        opts.config_path,
        opts.output,
        opts.no_cache,
        opts.threads,
        opts.production,
        opts.quiet,
    )?;
    execute_health_inner(
        opts,
        config,
        shared.files,
        shared.modules,
        0.0,
        0.0,
        0.0,
        shared.analysis_output,
    )
}

pub fn execute_health(opts: &HealthOptions<'_>) -> Result<HealthResult, ExitCode> {
    let t = Instant::now();
    let config = load_config(
        opts.root,
        opts.config_path,
        opts.output,
        opts.no_cache,
        opts.threads,
        opts.production,
        opts.quiet,
    )?;
    let config_ms = t.elapsed().as_secs_f64() * 1000.0;

    // Discover and parse files
    let t = Instant::now();
    let files = fallow_core::discover::discover_files(&config);
    let discover_ms = t.elapsed().as_secs_f64() * 1000.0;

    let cache = if config.no_cache {
        None
    } else {
        fallow_core::cache::CacheStore::load(&config.cache_dir)
    };
    let t = Instant::now();
    let parse_result = fallow_core::extract::parse_all_files(&files, cache.as_ref(), true);
    let parse_ms = t.elapsed().as_secs_f64() * 1000.0;

    execute_health_inner(
        opts,
        config,
        files,
        parse_result.modules,
        config_ms,
        discover_ms,
        parse_ms,
        None,
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "health pipeline orchestration with many optional features"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "owned files/modules transferred from shared parse or local parse"
)]
#[expect(
    clippy::too_many_arguments,
    reason = "inner function receives all pipeline state from two entry points"
)]
fn execute_health_inner(
    opts: &HealthOptions<'_>,
    config: ResolvedConfig,
    files: Vec<fallow_types::discover::DiscoveredFile>,
    modules: Vec<fallow_types::extract::ModuleInfo>,
    config_ms: f64,
    discover_ms: f64,
    parse_ms: f64,
    pre_computed_analysis: Option<fallow_core::AnalysisOutput>,
) -> Result<HealthResult, ExitCode> {
    let start = Instant::now();

    // Resolve thresholds: CLI flags override config
    let max_cyclomatic = opts.max_cyclomatic.unwrap_or(config.health.max_cyclomatic);
    let max_cognitive = opts.max_cognitive.unwrap_or(config.health.max_cognitive);

    let ignore_set = build_ignore_set(&config.health.ignore);
    let changed_files = opts
        .changed_since
        .and_then(|git_ref| get_changed_files(opts.root, git_ref));
    let ws_roots = resolve_workspace_scope(
        opts.root,
        opts.workspace,
        opts.changed_workspaces,
        opts.output,
    )?;

    // Build FileId -> path lookup for O(1) access
    let file_paths: rustc_hash::FxHashMap<_, _> = files.iter().map(|f| (f.id, &f.path)).collect();

    // Collect and filter complexity findings
    let t = Instant::now();
    let (mut findings, files_analyzed, total_functions) = collect_findings(
        &modules,
        &file_paths,
        &config.root,
        &ignore_set,
        changed_files.as_ref(),
        max_cyclomatic,
        max_cognitive,
    );
    if let Some(ref ws) = ws_roots {
        findings.retain(|f| ws.iter().any(|r| f.path.starts_with(r)));
    }
    sort_findings(&mut findings, &opts.sort);
    let complexity_ms = t.elapsed().as_secs_f64() * 1000.0;
    let total_above_threshold = findings.len();

    // Count severity tiers before baseline filtering and --top truncation
    let (mut sev_critical, mut sev_high, mut sev_moderate) = (0usize, 0usize, 0usize);
    for f in &findings {
        match f.severity {
            FindingSeverity::Critical => sev_critical += 1,
            FindingSeverity::High => sev_high += 1,
            FindingSeverity::Moderate => sev_moderate += 1,
        }
    }

    // Load baseline for filtering (save happens after targets are computed)
    let loaded_baseline = if let Some(load_path) = opts.baseline {
        Some(load_health_baseline(
            load_path,
            &mut findings,
            &config.root,
            opts.quiet,
            opts.output,
        )?)
    } else {
        None
    };
    if let Some(top) = opts.top {
        findings.truncate(top);
    }

    // Coverage gaps have two separate concerns:
    // - reporting: include the section in the rendered health output
    // - gating: fail the command when config severity is `error`
    //
    // Config severity may enable reporting for top-level `health` when the user
    // did not explicitly choose sections, but it must not override callers that
    // intentionally set `coverage_gaps: false` (combined mode, audit, score-only).
    let config_coverage_enabled = config.rules.coverage_gaps != fallow_config::Severity::Off;
    let report_coverage_gaps =
        opts.coverage_gaps || (opts.config_activates_coverage_gaps && config_coverage_enabled);
    let enforce_coverage_gaps = opts.enforce_coverage_gap_gate
        && config.rules.coverage_gaps == fallow_config::Severity::Error;

    // Load Istanbul coverage data for accurate CRAP scoring.
    // Priority: explicit --coverage flag > auto-detected coverage-final.json.
    let istanbul_coverage = if let Some(coverage_path) = opts.coverage {
        match scoring::load_istanbul_coverage(coverage_path, opts.coverage_root, Some(&config.root))
        {
            Ok(cov) => Some(cov),
            Err(e) => {
                emit_error(&format!("coverage: {e}"), 2, opts.output);
                return Err(ExitCode::from(2));
            }
        }
    } else if let Some(auto_path) = scoring::auto_detect_coverage(&config.root) {
        // Auto-detected coverage file: best-effort, don't fail if it can't be parsed.
        // Note in CI environments so pipelines know scores may vary with coverage presence.
        if std::env::var("CI").is_ok_and(|v| !v.is_empty()) {
            eprintln!(
                "note: using auto-detected coverage at {}; pass --coverage explicitly for deterministic CI scores",
                auto_path.display()
            );
        }
        scoring::load_istanbul_coverage(&auto_path, opts.coverage_root, Some(&config.root)).ok()
    } else {
        None
    };

    // Compute file-level health scores (needed by hotspots and targets too)
    let needs_file_scores = opts.file_scores
        || report_coverage_gaps
        || enforce_coverage_gaps
        || opts.hotspots
        || opts.targets
        || opts.force_full;
    // Run file scoring and churn fetch in parallel when both are needed.
    // Churn fetch involves a `git log` shell-out that dominates health timing.
    let needs_churn = opts.hotspots || opts.targets || opts.force_full;
    let (file_score_result, file_scores_ms, churn_fetch) = if needs_file_scores && needs_churn {
        std::thread::scope(|s| {
            let churn_handle = s.spawn(|| hotspots::fetch_churn_data(opts, &config.cache_dir));
            let t = Instant::now();
            let score_result = compute_filtered_file_scores(
                &config,
                &modules,
                &file_paths,
                changed_files.as_ref(),
                ws_roots.as_deref(),
                &ignore_set,
                opts.output,
                istanbul_coverage.as_ref(),
                pre_computed_analysis,
            );
            let fs_ms = t.elapsed().as_secs_f64() * 1000.0;
            let churn = churn_handle.join().expect("churn thread panicked");
            (score_result, fs_ms, churn)
        })
    } else {
        let t = Instant::now();
        let score_result = if needs_file_scores {
            compute_filtered_file_scores(
                &config,
                &modules,
                &file_paths,
                changed_files.as_ref(),
                ws_roots.as_deref(),
                &ignore_set,
                opts.output,
                istanbul_coverage.as_ref(),
                pre_computed_analysis,
            )
        } else {
            Ok((None, None, None))
        };
        let fs_ms = t.elapsed().as_secs_f64() * 1000.0;
        let churn = if needs_churn {
            hotspots::fetch_churn_data(opts, &config.cache_dir)
        } else {
            None
        };
        (score_result, fs_ms, churn)
    };
    let (git_churn_ms, git_churn_cache_hit) = churn_fetch
        .as_ref()
        .map_or((0.0, false), |cf| (cf.git_log_ms, cf.cache_hit));
    let (score_output, files_scored, average_maintainability) = file_score_result?;

    // Print churn cache note on cold miss (only when cache is enabled)
    if let Some(ref cf) = churn_fetch
        && !cf.cache_hit
        && !opts.no_cache
        && !opts.quiet
        && cf.git_log_ms > 500.0
    {
        eprintln!(
            "{}",
            format!(
                "  note: git churn analysis took {:.1}s (cached for next run at same HEAD)",
                cf.git_log_ms / 1000.0
            )
            .dimmed()
        );
    }

    let file_scores_slice = score_output
        .as_ref()
        .map_or(&[] as &[_], |o| o.scores.as_slice());

    // Compute hotspot analysis using pre-fetched churn data
    let t = Instant::now();
    let (hotspots, hotspot_summary) = if let Some(churn_data) = churn_fetch {
        compute_hotspots(
            opts,
            &config,
            file_scores_slice,
            &ignore_set,
            ws_roots.as_deref(),
            churn_data,
        )
    } else {
        (Vec::new(), None)
    };
    let hotspots_ms = t.elapsed().as_secs_f64() * 1000.0;

    // Compute refactoring targets
    let t = Instant::now();
    let (targets, target_thresholds) = compute_targets(
        opts,
        score_output.as_ref(),
        file_scores_slice,
        &hotspots,
        loaded_baseline.as_ref(),
        &config.root,
    );
    let targets_ms = t.elapsed().as_secs_f64() * 1000.0;

    let mut production_coverage = if let Some(ref production_options) = opts.production_coverage {
        Some(coverage::analyze(
            production_options,
            &config.root,
            &modules,
            &file_paths,
            &ignore_set,
            changed_files.as_ref(),
            ws_roots.as_deref(),
            opts.top,
            opts.quiet,
            opts.output,
        )?)
    } else {
        None
    };
    if let Some(report) = production_coverage.as_mut() {
        apply_production_coverage_filters(
            report,
            loaded_baseline.as_ref(),
            &config.root,
            opts.top,
            changed_files.as_ref(),
        );
    }

    if let Some(save_path) = opts.save_baseline {
        save_health_baseline(
            save_path,
            &findings,
            production_coverage
                .as_ref()
                .map_or(&[], |report| report.findings.as_slice()),
            &targets,
            &config.root,
            opts.quiet,
            opts.output,
        )?;
    }

    // Compute vital signs (always needed for report summary)
    let (mut vital_signs, mut counts) = compute_vital_signs_and_counts(
        score_output.as_ref(),
        &modules,
        needs_file_scores,
        file_scores_slice,
        opts.hotspots || opts.targets,
        &hotspots,
        files.len(),
    );

    // Run duplication analysis when --score is active to populate the duplication penalty.
    let t = Instant::now();
    if opts.score {
        let dupes_report =
            fallow_core::duplicates::find_duplicates(&config.root, &files, &config.duplicates);
        let pct = dupes_report.stats.duplication_percentage;
        vital_signs.duplication_pct = Some((pct * 10.0).round() / 10.0);
        // Update duplicated_lines on both the snapshot counts and the embedded vital signs
        // counts so JSON consumers can see raw numerator alongside the percentage.
        // total_lines is already populated unconditionally from parsed modules.
        counts.duplicated_lines = Some(dupes_report.stats.duplicated_lines);
        if let Some(ref mut vc) = vital_signs.counts {
            vc.duplicated_lines = Some(dupes_report.stats.duplicated_lines);
        }
    }
    let duplication_ms = t.elapsed().as_secs_f64() * 1000.0;

    let health_score = if opts.score {
        Some(vital_signs::compute_health_score(&vital_signs, files.len()))
    } else {
        None
    };

    // Collect large functions (>60 LOC) when the risk profile warrants it
    let large_functions = collect_large_functions(
        &vital_signs,
        &modules,
        &file_paths,
        &config.root,
        &ignore_set,
        changed_files.as_ref(),
        ws_roots.as_deref(),
    );

    // Determine coverage model for snapshot and report
    let active_coverage_model = if istanbul_coverage.is_some() {
        Some(crate::health_types::CoverageModel::Istanbul)
    } else {
        Some(crate::health_types::CoverageModel::StaticEstimated)
    };

    if let Some(ref snapshot_path) = opts.save_snapshot {
        save_snapshot(
            opts,
            snapshot_path,
            &vital_signs,
            &counts,
            hotspot_summary.as_ref(),
            health_score.as_ref(),
            active_coverage_model,
        )?;
    }

    let health_trend = compute_health_trend(opts, &vital_signs, &counts, health_score.as_ref());

    // Assemble final report
    let coverage_gaps_has_findings = score_output
        .as_ref()
        .is_some_and(|output| !output.coverage.report.is_empty());

    let report = assemble_health_report(
        opts,
        report_coverage_gaps,
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
        istanbul_coverage.is_some(),
        production_coverage,
        large_functions,
        sev_critical,
        sev_high,
        sev_moderate,
    );

    let timings = if opts.performance {
        Some(HealthTimings {
            config_ms,
            discover_ms,
            parse_ms,
            complexity_ms,
            file_scores_ms,
            git_churn_ms,
            git_churn_cache_hit,
            hotspots_ms,
            duplication_ms,
            targets_ms,
            total_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    } else {
        None
    };

    Ok(HealthResult {
        report,
        config,
        elapsed: start.elapsed(),
        timings,
        coverage_gaps_has_findings,
        should_fail_on_coverage_gaps: enforce_coverage_gaps,
    })
}

fn apply_production_coverage_filters(
    report: &mut crate::health_types::ProductionCoverageReport,
    baseline: Option<&HealthBaselineData>,
    root: &std::path::Path,
    top: Option<usize>,
    changed_files: Option<&FxHashSet<PathBuf>>,
) {
    if let Some(baseline) = baseline {
        report.findings = filter_new_production_coverage_findings(
            std::mem::take(&mut report.findings),
            baseline,
            root,
        );
    }

    if let Some(changed_files) = changed_files {
        report
            .hot_paths
            .retain(|hot_path| changed_files.contains(&hot_path.path));
    }

    refresh_production_coverage_verdict(report, changed_files.is_some());

    if let Some(top) = top {
        report.findings.truncate(top);
        report.hot_paths.truncate(top);
    }
}

fn refresh_production_coverage_verdict(
    report: &mut crate::health_types::ProductionCoverageReport,
    changed_review: bool,
) {
    let has_cold_signal = report.findings.iter().any(|finding| {
        matches!(
            finding.verdict,
            crate::health_types::ProductionCoverageVerdict::SafeToDelete
                | crate::health_types::ProductionCoverageVerdict::ReviewRequired
                | crate::health_types::ProductionCoverageVerdict::LowTraffic
        )
    });
    let has_changed_hot_path = changed_review && !report.hot_paths.is_empty();

    report.verdict = if matches!(
        report.verdict,
        crate::health_types::ProductionCoverageReportVerdict::LicenseExpiredGrace
    ) || matches!(
        report.watermark,
        Some(crate::health_types::ProductionCoverageWatermark::LicenseExpiredGrace)
    ) {
        crate::health_types::ProductionCoverageReportVerdict::LicenseExpiredGrace
    } else if has_cold_signal {
        crate::health_types::ProductionCoverageReportVerdict::ColdCodeDetected
    } else if has_changed_hot_path {
        crate::health_types::ProductionCoverageReportVerdict::HotPathChangesNeeded
    } else {
        crate::health_types::ProductionCoverageReportVerdict::Clean
    };
}

/// Sort findings by the specified criteria.
fn sort_findings(findings: &mut [HealthFinding], sort: &SortBy) {
    match sort {
        SortBy::Cyclomatic => findings.sort_by_key(|f| std::cmp::Reverse(f.cyclomatic)),
        SortBy::Cognitive => findings.sort_by_key(|f| std::cmp::Reverse(f.cognitive)),
        SortBy::Lines => findings.sort_by_key(|f| std::cmp::Reverse(f.line_count)),
    }
}

/// `(score_output, files_scored, average_maintainability)`.
type FileScoreResult = (Option<scoring::FileScoreOutput>, Option<usize>, Option<f64>);

/// Compute file scores, applying workspace and ignore filters.
#[expect(
    clippy::too_many_arguments,
    reason = "filter pipeline requires all these inputs"
)]
fn compute_filtered_file_scores(
    config: &ResolvedConfig,
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    ws_roots: Option<&[std::path::PathBuf]>,
    ignore_set: &globset::GlobSet,
    output: OutputFormat,
    istanbul_coverage: Option<&scoring::IstanbulCoverage>,
    pre_computed: Option<fallow_core::AnalysisOutput>,
) -> Result<FileScoreResult, ExitCode> {
    let analysis_output = if let Some(pre) = pre_computed {
        pre
    } else {
        fallow_core::analyze_with_parse_result(config, modules)
            .map_err(|e| emit_error(&format!("analysis failed: {e}"), 2, output))?
    };
    match compute_file_scores(
        modules,
        file_paths,
        changed_files,
        analysis_output,
        istanbul_coverage,
    ) {
        Ok(mut output) => {
            if let Some(ws) = ws_roots {
                output
                    .scores
                    .retain(|s| ws.iter().any(|r| s.path.starts_with(r)));
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
                ws_roots,
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
    ws_roots: Option<&[std::path::PathBuf]>,
    ignore_set: &globset::GlobSet,
) -> bool {
    if let Some(changed) = changed_files
        && !changed.contains(path)
    {
        return false;
    }
    if let Some(ws) = ws_roots
        && !ws.iter().any(|r| path.starts_with(r))
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
    ws_roots: Option<&[std::path::PathBuf]>,
    ignore_set: &globset::GlobSet,
) {
    runtime_paths
        .retain(|path| path_in_health_scope(path, config, changed_files, ws_roots, ignore_set));
    coverage_gaps.files.retain(|item| {
        path_in_health_scope(&item.path, config, changed_files, ws_roots, ignore_set)
    });
    coverage_gaps.exports.retain(|item| {
        path_in_health_scope(&item.path, config, changed_files, ws_roots, ignore_set)
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
    coverage_model: Option<crate::health_types::CoverageModel>,
) -> Result<(), ExitCode> {
    let shallow = hotspot_summary.is_some_and(|s| s.shallow_clone);
    let snapshot = vital_signs::build_snapshot(
        vital_signs.clone(),
        counts.clone(),
        opts.root,
        shallow,
        health_score,
        coverage_model,
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
    report_coverage_gaps: bool,
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
    has_istanbul_coverage: bool,
    production_coverage: Option<crate::health_types::ProductionCoverageReport>,
    large_functions: Vec<LargeFunctionEntry>,
    sev_critical: usize,
    sev_high: usize,
    sev_moderate: usize,
) -> HealthReport {
    let coverage_gaps = if report_coverage_gaps {
        score_output.as_ref().map(|o| o.coverage.report.clone())
    } else {
        None
    };

    // Extract Istanbul match stats before score_output is consumed
    let (ist_matched, ist_total) = score_output
        .as_ref()
        .map_or((0, 0), |o| (o.istanbul_matched, o.istanbul_total));

    // Extract file scores for the report (apply --top after hotspot/target computation)
    let file_scores = if opts.score_only_output {
        Vec::new()
    } else if opts.file_scores {
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

    let summary_files_scored = if opts.score_only_output || !opts.file_scores {
        None
    } else {
        files_scored
    };
    let summary_average_maintainability = if opts.score_only_output || !opts.file_scores {
        None
    } else {
        average_maintainability
    };
    let summary_coverage_model = if opts.score_only_output {
        None
    } else if opts.file_scores || report_coverage_gaps || opts.hotspots || opts.targets {
        Some(if has_istanbul_coverage {
            crate::health_types::CoverageModel::Istanbul
        } else {
            crate::health_types::CoverageModel::StaticEstimated
        })
    } else {
        None
    };
    let summary_istanbul_matched = if opts.score_only_output || !has_istanbul_coverage {
        None
    } else {
        Some(ist_matched)
    };
    let summary_istanbul_total = if opts.score_only_output || !has_istanbul_coverage {
        None
    } else {
        Some(ist_total)
    };

    HealthReport {
        summary: HealthSummary {
            files_analyzed,
            functions_analyzed: total_functions,
            functions_above_threshold: total_above_threshold,
            max_cyclomatic_threshold: max_cyclomatic,
            max_cognitive_threshold: max_cognitive,
            files_scored: summary_files_scored,
            average_maintainability: summary_average_maintainability,
            coverage_model: summary_coverage_model,
            istanbul_matched: summary_istanbul_matched,
            istanbul_total: summary_istanbul_total,
            severity_critical_count: sev_critical,
            severity_high_count: sev_high,
            severity_moderate_count: sev_moderate,
        },
        vital_signs: if opts.score_only_output {
            None
        } else {
            Some(vital_signs)
        },
        health_score,
        findings: if opts.complexity {
            findings
        } else {
            Vec::new()
        },
        file_scores,
        coverage_gaps: if opts.score_only_output {
            None
        } else {
            coverage_gaps
        },
        hotspots: report_hotspots,
        hotspot_summary: if opts.score_only_output {
            None
        } else {
            report_hotspot_summary
        },
        production_coverage,
        large_functions: if opts.score_only_output {
            Vec::new()
        } else {
            large_functions
        },
        targets: if opts.score_only_output {
            Vec::new()
        } else {
            targets
        },
        target_thresholds: if opts.score_only_output {
            None
        } else {
            target_thresholds
        },
        health_trend,
    }
}

/// Collect functions exceeding 60 LOC when the unit size risk profile warrants it.
///
/// Only populated when `very_high_risk >= 3%` in the unit size profile (same threshold
/// that triggers showing the risk profile line). Sorted by line count descending.
fn collect_large_functions(
    vital_signs: &crate::health_types::VitalSigns,
    modules: &[fallow_core::extract::ModuleInfo],
    file_paths: &rustc_hash::FxHashMap<fallow_core::discover::FileId, &std::path::PathBuf>,
    config_root: &std::path::Path,
    ignore_set: &globset::GlobSet,
    changed_files: Option<&rustc_hash::FxHashSet<std::path::PathBuf>>,
    ws_roots: Option<&[std::path::PathBuf]>,
) -> Vec<LargeFunctionEntry> {
    let dominated = vital_signs
        .unit_size_profile
        .as_ref()
        .is_some_and(|p| p.very_high_risk >= 3.0);
    if !dominated {
        return Vec::new();
    }

    let mut entries = Vec::new();
    for module in modules {
        let Some(&path) = file_paths.get(&module.file_id) else {
            continue;
        };
        let relative = path.strip_prefix(config_root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            continue;
        }
        if let Some(changed) = changed_files
            && !changed.contains(path.as_path())
        {
            continue;
        }
        if let Some(ws) = ws_roots
            && !ws.iter().any(|r| path.starts_with(r))
        {
            continue;
        }
        for func in &module.complexity {
            if func.line_count > 60 {
                entries.push(LargeFunctionEntry {
                    path: path.clone(),
                    name: func.name.clone(),
                    line: func.line,
                    line_count: func.line_count,
                });
            }
        }
    }
    entries.sort_by_key(|e| std::cmp::Reverse(e.line_count));
    entries
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
            if fallow_core::suppress::is_suppressed(
                &module.suppressions,
                fc.line,
                fallow_core::suppress::IssueKind::Complexity,
            ) {
                continue;
            }
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
                    param_count: fc.param_count,
                    exceeded,
                    severity: compute_finding_severity(
                        fc.cognitive,
                        fc.cyclomatic,
                        DEFAULT_COGNITIVE_HIGH,
                        DEFAULT_COGNITIVE_CRITICAL,
                        DEFAULT_CYCLOMATIC_HIGH,
                        DEFAULT_CYCLOMATIC_CRITICAL,
                    ),
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
    production_coverage_findings: &[crate::health_types::ProductionCoverageFinding],
    targets: &[RefactoringTarget],
    config_root: &std::path::Path,
    quiet: bool,
    output: OutputFormat,
) -> Result<(), ExitCode> {
    let baseline = HealthBaselineData::from_findings(
        findings,
        production_coverage_findings,
        targets,
        config_root,
    );
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
    quiet: bool,
    output: OutputFormat,
) -> Result<HealthBaselineData, ExitCode> {
    let json = std::fs::read_to_string(baseline_path)
        .map_err(|e| emit_error(&format!("failed to read health baseline: {e}"), 2, output))?;
    let baseline: HealthBaselineData = serde_json::from_str(&json)
        .map_err(|e| emit_error(&format!("failed to parse health baseline: {e}"), 2, output))?;
    let baseline_entries = baseline.findings.len();
    let before = findings.len();
    *findings = filter_new_health_findings(std::mem::take(findings), &baseline, root);
    let matched = before.saturating_sub(findings.len());
    if !quiet {
        eprintln!(
            "Comparing against health baseline: {}",
            baseline_path.display()
        );
    }
    if baseline_entries > 0 && matched == 0 && !quiet {
        eprintln!(
            "Warning: health baseline has {baseline_entries} entries but matched \
             0 current findings. Your paths may have changed, or the baseline \
             was saved on a different machine. Re-save with: \
             --save-baseline {}",
            baseline_path.display(),
        );
    }
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
    if let Some(ref timings) = result.timings {
        report::print_health_performance(timings, opts.output);
    }
    print_health_result(
        &result,
        opts.quiet,
        opts.explain,
        opts.min_score,
        opts.min_severity,
        opts.summary,
    )
}

/// Result of executing health analysis without printing.
pub struct HealthResult {
    pub report: HealthReport,
    pub config: ResolvedConfig,
    pub elapsed: Duration,
    pub timings: Option<HealthTimings>,
    pub coverage_gaps_has_findings: bool,
    pub should_fail_on_coverage_gaps: bool,
}

/// Print health results and return appropriate exit code.
pub fn print_health_result(
    result: &HealthResult,
    quiet: bool,
    explain: bool,
    min_score: Option<f64>,
    min_severity: Option<FindingSeverity>,
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
        baseline_matched: None,
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

    // Check findings against --min-severity filter
    let has_failing_findings = if let Some(min_sev) = min_severity {
        result.report.findings.iter().any(|f| f.severity >= min_sev)
    } else {
        !result.report.findings.is_empty()
    };
    let has_failing_production_coverage =
        result
            .report
            .production_coverage
            .as_ref()
            .is_some_and(|report| {
                report.findings.iter().any(|finding| {
                    matches!(
                        finding.verdict,
                        crate::health_types::ProductionCoverageVerdict::SafeToDelete
                            | crate::health_types::ProductionCoverageVerdict::ReviewRequired
                    )
                })
            });
    if has_failing_findings || has_failing_production_coverage {
        return ExitCode::from(1);
    }

    if result.should_fail_on_coverage_gaps && result.coverage_gaps_has_findings {
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
            flag_uses: vec![],
            class_heritage: vec![],
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
            param_count: 0,
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
                param_count: 2,
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

    fn fx_summary(
        tracked: usize,
        hit: usize,
        unhit: usize,
        untracked: usize,
    ) -> crate::health_types::ProductionCoverageSummary {
        #[expect(
            clippy::cast_precision_loss,
            reason = "test fixture totals are tiny — f64 precision is fine"
        )]
        let coverage_percent = if tracked == 0 {
            0.0
        } else {
            (hit as f64 / tracked as f64) * 100.0
        };
        crate::health_types::ProductionCoverageSummary {
            functions_tracked: tracked,
            functions_hit: hit,
            functions_unhit: unhit,
            functions_untracked: untracked,
            coverage_percent,
            trace_count: 512,
            period_days: 7,
            deployments_seen: 2,
        }
    }

    fn fx_evidence(
        static_status: &str,
        test_coverage: &str,
        v8_tracking: &str,
    ) -> crate::health_types::ProductionCoverageEvidence {
        crate::health_types::ProductionCoverageEvidence {
            static_status: static_status.to_owned(),
            test_coverage: test_coverage.to_owned(),
            v8_tracking: v8_tracking.to_owned(),
            untracked_reason: None,
            observation_days: 7,
            deployments_observed: 2,
        }
    }

    #[test]
    fn production_coverage_top_applies_after_baseline_filtering() {
        let root = Path::new("/project");
        let baseline = HealthBaselineData {
            findings: vec![],
            production_coverage_findings: vec![
                "fallow:prod:aaaaaaaa".to_owned(),
                "fallow:prod:bbbbbbbb".to_owned(),
            ],
            target_keys: vec![],
        };
        let mut report = crate::health_types::ProductionCoverageReport {
            verdict: crate::health_types::ProductionCoverageReportVerdict::ColdCodeDetected,
            summary: fx_summary(3, 0, 2, 1),
            findings: vec![
                crate::health_types::ProductionCoverageFinding {
                    id: "fallow:prod:aaaaaaaa".to_owned(),
                    path: PathBuf::from("/project/src/a.ts"),
                    function: "alpha".to_owned(),
                    line: 10,
                    verdict: crate::health_types::ProductionCoverageVerdict::ReviewRequired,
                    invocations: Some(0),
                    confidence: crate::health_types::ProductionCoverageConfidence::Medium,
                    evidence: fx_evidence("used", "not_covered", "tracked"),
                    actions: vec![],
                },
                crate::health_types::ProductionCoverageFinding {
                    id: "fallow:prod:bbbbbbbb".to_owned(),
                    path: PathBuf::from("/project/src/b.ts"),
                    function: "beta".to_owned(),
                    line: 20,
                    verdict: crate::health_types::ProductionCoverageVerdict::CoverageUnavailable,
                    invocations: None,
                    confidence: crate::health_types::ProductionCoverageConfidence::None,
                    evidence: fx_evidence("used", "not_covered", "untracked"),
                    actions: vec![],
                },
                crate::health_types::ProductionCoverageFinding {
                    id: "fallow:prod:cccccccc".to_owned(),
                    path: PathBuf::from("/project/src/c.ts"),
                    function: "gamma".to_owned(),
                    line: 30,
                    verdict: crate::health_types::ProductionCoverageVerdict::ReviewRequired,
                    invocations: Some(0),
                    confidence: crate::health_types::ProductionCoverageConfidence::Medium,
                    evidence: fx_evidence("used", "not_covered", "tracked"),
                    actions: vec![],
                },
            ],
            hot_paths: vec![
                crate::health_types::ProductionCoverageHotPath {
                    id: "fallow:hot:11111111".to_owned(),
                    path: PathBuf::from("/project/src/hot-a.ts"),
                    function: "hotAlpha".to_owned(),
                    line: 1,
                    invocations: 500,
                    percentile: 99,
                    actions: vec![],
                },
                crate::health_types::ProductionCoverageHotPath {
                    id: "fallow:hot:22222222".to_owned(),
                    path: PathBuf::from("/project/src/hot-b.ts"),
                    function: "hotBeta".to_owned(),
                    line: 2,
                    invocations: 250,
                    percentile: 50,
                    actions: vec![],
                },
            ],
            watermark: None,
            warnings: vec![],
        };

        apply_production_coverage_filters(&mut report, Some(&baseline), root, Some(1), None);

        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].function, "gamma");
        assert_eq!(
            report.verdict,
            crate::health_types::ProductionCoverageReportVerdict::ColdCodeDetected
        );
        assert_eq!(report.summary.functions_tracked, 3);
        assert_eq!(report.summary.functions_hit, 0);
        assert_eq!(report.summary.functions_unhit, 2);
        assert_eq!(report.summary.functions_untracked, 1);
        assert!((report.summary.coverage_percent - 0.0).abs() < 0.05);
        assert_eq!(report.hot_paths.len(), 1);
        assert_eq!(report.hot_paths[0].function, "hotAlpha");
    }

    #[test]
    fn production_coverage_baseline_refreshes_to_clean_when_only_baselined_findings_remain() {
        let root = Path::new("/project");
        let baseline = HealthBaselineData {
            findings: vec![],
            production_coverage_findings: vec!["fallow:prod:aaaaaaaa".to_owned()],
            target_keys: vec![],
        };
        let mut report = crate::health_types::ProductionCoverageReport {
            verdict: crate::health_types::ProductionCoverageReportVerdict::ColdCodeDetected,
            summary: fx_summary(2, 1, 1, 0),
            findings: vec![crate::health_types::ProductionCoverageFinding {
                id: "fallow:prod:aaaaaaaa".to_owned(),
                path: PathBuf::from("/project/src/a.ts"),
                function: "alpha".to_owned(),
                line: 10,
                verdict: crate::health_types::ProductionCoverageVerdict::ReviewRequired,
                invocations: Some(0),
                confidence: crate::health_types::ProductionCoverageConfidence::Medium,
                evidence: fx_evidence("used", "not_covered", "tracked"),
                actions: vec![],
            }],
            hot_paths: vec![],
            watermark: None,
            warnings: vec![],
        };

        apply_production_coverage_filters(&mut report, Some(&baseline), root, None, None);

        assert!(report.findings.is_empty());
        assert_eq!(
            report.verdict,
            crate::health_types::ProductionCoverageReportVerdict::Clean
        );
        assert_eq!(report.summary.functions_tracked, 2);
        assert_eq!(report.summary.functions_hit, 1);
        assert_eq!(report.summary.functions_unhit, 1);
        assert_eq!(report.summary.functions_untracked, 0);
        assert!((report.summary.coverage_percent - 50.0).abs() < 0.05);
    }

    #[test]
    fn production_coverage_changed_review_uses_hot_path_verdict() {
        let root = Path::new("/project");
        let mut changed_files = FxHashSet::default();
        changed_files.insert(PathBuf::from("/project/src/hot.ts"));
        let mut report = crate::health_types::ProductionCoverageReport {
            verdict: crate::health_types::ProductionCoverageReportVerdict::Clean,
            summary: fx_summary(2, 2, 0, 0),
            findings: vec![],
            hot_paths: vec![crate::health_types::ProductionCoverageHotPath {
                id: "fallow:hot:33333333".to_owned(),
                path: PathBuf::from("/project/src/hot.ts"),
                function: "renderHotPath".to_owned(),
                line: 7,
                invocations: 9_500,
                percentile: 99,
                actions: vec![],
            }],
            watermark: None,
            warnings: vec![],
        };

        apply_production_coverage_filters(&mut report, None, root, None, Some(&changed_files));

        assert_eq!(
            report.verdict,
            crate::health_types::ProductionCoverageReportVerdict::HotPathChangesNeeded
        );
    }

    #[test]
    fn production_coverage_changed_review_ignores_unmodified_hot_paths() {
        let root = Path::new("/project");
        let mut changed_files = FxHashSet::default();
        changed_files.insert(PathBuf::from("/project/src/other.ts"));
        let mut report = crate::health_types::ProductionCoverageReport {
            verdict: crate::health_types::ProductionCoverageReportVerdict::Clean,
            summary: fx_summary(2, 2, 0, 0),
            findings: vec![],
            hot_paths: vec![crate::health_types::ProductionCoverageHotPath {
                id: "fallow:hot:44444444".to_owned(),
                path: PathBuf::from("/project/src/hot.ts"),
                function: "renderHotPath".to_owned(),
                line: 7,
                invocations: 9_500,
                percentile: 90,
                actions: vec![],
            }],
            watermark: None,
            warnings: vec![],
        };

        apply_production_coverage_filters(&mut report, None, root, None, Some(&changed_files));

        assert!(report.hot_paths.is_empty());
        assert_eq!(
            report.verdict,
            crate::health_types::ProductionCoverageReportVerdict::Clean
        );
    }
}
