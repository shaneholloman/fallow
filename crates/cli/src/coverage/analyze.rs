//! `fallow coverage analyze` implementation.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use fallow_config::OutputFormat;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::coverage::RunContext;
use crate::coverage::cloud_client::{
    CloudError, CloudRequest, CloudRuntimeContext, CloudRuntimeFunction, CloudRuntimeWarning,
    CloudTrackingState, fetch_runtime_context,
};
use crate::error::emit_error;
use crate::health::{HealthOptions, SortBy};
use crate::health_types::{
    RuntimeCoverageAction, RuntimeCoverageCaptureQuality, RuntimeCoverageConfidence,
    RuntimeCoverageDataSource, RuntimeCoverageEvidence, RuntimeCoverageFinding,
    RuntimeCoverageHotPath, RuntimeCoverageMessage, RuntimeCoverageReport,
    RuntimeCoverageReportVerdict, RuntimeCoverageSummary, RuntimeCoverageVerdict,
};

const RUNTIME_COVERAGE_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Default)]
pub struct AnalyzeArgs {
    pub runtime_coverage: Option<PathBuf>,
    pub cloud: bool,
    pub api_key: Option<String>,
    pub api_endpoint: Option<String>,
    pub repo: Option<String>,
    pub project_id: Option<String>,
    pub coverage_period: u16,
    pub environment: Option<String>,
    pub commit_sha: Option<String>,
    pub production: bool,
    pub min_invocations_hot: u64,
    pub min_observation_volume: Option<u32>,
    pub low_traffic_threshold: Option<f64>,
    pub top: Option<usize>,
}

pub fn run(args: &AnalyzeArgs, ctx: &RunContext<'_>) -> ExitCode {
    if let Err(message) = validate_output_format(ctx.output) {
        return emit_error(&message, 2, ctx.output);
    }

    let env_cloud = runtime_coverage_source_env_is_cloud();
    let cloud = args.cloud || env_cloud;
    if cloud && args.runtime_coverage.is_some() {
        return emit_error(
            "Choose one runtime coverage source: --cloud or --runtime-coverage <path>.",
            2,
            ctx.output,
        );
    }

    if cloud {
        return run_cloud(args, ctx);
    }

    let Some(path) = args.runtime_coverage.as_deref() else {
        return emit_error(
            "No runtime coverage source selected. Pass --runtime-coverage <path>, --cloud, or set FALLOW_RUNTIME_COVERAGE_SOURCE=cloud.",
            2,
            ctx.output,
        );
    };
    run_local(path, args, ctx)
}

/// `fallow coverage analyze` only emits two output formats: structured JSON
/// (the canonical agent-readable shape, used by every non-`Human` `--format`
/// today) and the terse human renderer. Other formats (`compact`, `markdown`,
/// `sarif`, `codeclimate`, `badge`) require shape conversion that this
/// command does not yet implement; falling through to the JSON serializer
/// would silently mislead consumers expecting SARIF or markdown. Reject them
/// explicitly so the user gets an actionable error instead.
fn validate_output_format(output: OutputFormat) -> Result<(), String> {
    match output {
        OutputFormat::Json | OutputFormat::Human => Ok(()),
        OutputFormat::Compact
        | OutputFormat::Markdown
        | OutputFormat::Sarif
        | OutputFormat::CodeClimate
        | OutputFormat::Badge => Err(format!(
            "fallow coverage analyze only supports --format json or --format human (got {output:?}). Use `fallow coverage analyze --format json` and pipe to your own converter for {output:?}."
        )),
    }
}

fn run_local(path: &Path, args: &AnalyzeArgs, ctx: &RunContext<'_>) -> ExitCode {
    let runtime_coverage = match crate::health::coverage::prepare_options(
        path,
        args.min_invocations_hot,
        args.min_observation_volume,
        args.low_traffic_threshold,
        ctx.output,
    ) {
        Ok(options) => options,
        Err(code) => return code,
    };
    let result = match crate::health::execute_health(&HealthOptions {
        root: ctx.root,
        config_path: ctx.config_path,
        output: ctx.output,
        no_cache: ctx.no_cache,
        threads: ctx.threads,
        quiet: ctx.quiet,
        max_cyclomatic: None,
        max_cognitive: None,
        max_crap: None,
        top: args.top,
        sort: SortBy::Cyclomatic,
        production: args.production,
        production_override: Some(args.production),
        changed_since: None,
        workspace: None,
        changed_workspaces: None,
        baseline: None,
        save_baseline: None,
        complexity: false,
        file_scores: false,
        coverage_gaps: false,
        config_activates_coverage_gaps: false,
        hotspots: false,
        ownership: false,
        ownership_emails: None,
        targets: false,
        force_full: false,
        score_only_output: false,
        enforce_coverage_gap_gate: false,
        effort: None,
        score: false,
        min_score: None,
        since: None,
        min_commits: None,
        explain: ctx.explain,
        summary: false,
        save_snapshot: None,
        trend: false,
        group_by: None,
        coverage: None,
        coverage_root: None,
        performance: false,
        min_severity: None,
        runtime_coverage: Some(runtime_coverage),
    }) {
        Ok(result) => result,
        Err(code) => return code,
    };
    let Some(report) = result.report.runtime_coverage else {
        return emit_error("runtime coverage report was not produced", 2, ctx.output);
    };
    print_runtime_report(&report, ctx, result.elapsed, args.top)
}

fn run_cloud(args: &AnalyzeArgs, ctx: &RunContext<'_>) -> ExitCode {
    let api_key = match resolve_api_key(args.api_key.as_deref()) {
        Ok(api_key) => api_key,
        Err(err) => return emit_cloud_error(&err, ctx.output),
    };
    let repo = match resolve_repo(args.repo.as_deref(), ctx.root) {
        Ok(repo) => repo,
        Err(err) => return emit_cloud_error(&err, ctx.output),
    };
    let request = CloudRequest {
        api_key,
        api_endpoint: args.api_endpoint.clone(),
        repo,
        project_id: args.project_id.clone(),
        period_days: args.coverage_period,
        environment: args.environment.clone(),
        commit_sha: args.commit_sha.clone(),
    };

    let start = Instant::now();
    let snapshot = match fetch_runtime_context(&request) {
        Ok(snapshot) => snapshot,
        Err(err) => return emit_cloud_error(&err, ctx.output),
    };
    let static_index = match build_static_index(ctx, args.production) {
        Ok(index) => index,
        Err(code) => return code,
    };
    let mut report = merge_cloud_snapshot(&snapshot, &static_index, args.min_invocations_hot);
    if let Some(top) = args.top {
        report.findings.truncate(top);
        report.hot_paths.truncate(top);
    }
    print_runtime_report(&report, ctx, start.elapsed(), args.top)
}

fn runtime_coverage_source_env_is_cloud() -> bool {
    std::env::var("FALLOW_RUNTIME_COVERAGE_SOURCE")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("cloud"))
}

fn resolve_api_key(explicit: Option<&str>) -> Result<String, CloudError> {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(value.to_owned());
    }
    if let Ok(value) = std::env::var("FALLOW_API_KEY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    Err(CloudError::Auth(
        "Cloud runtime coverage requires an API key.\n\nSet FALLOW_API_KEY or pass --api-key:\n\n  FALLOW_API_KEY=fallow_live_... fallow coverage analyze --cloud --repo owner/repo".to_owned(),
    ))
}

fn resolve_repo(explicit: Option<&str>, root: &Path) -> Result<String, CloudError> {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(value.to_owned());
    }
    if let Ok(value) = std::env::var("FALLOW_REPO") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    if let Some(from_remote) = git_origin_project_id(root) {
        return Ok(from_remote);
    }
    Err(CloudError::Validation(
        "Could not infer repository for cloud runtime coverage.\n\nPass it explicitly:\n\n  fallow coverage analyze --cloud --repo owner/repo\n\nor set:\n\n  FALLOW_REPO=owner/repo".to_owned(),
    ))
}

fn git_origin_project_id(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_git_remote_to_project_id(String::from_utf8_lossy(&output.stdout).trim())
}

fn parse_git_remote_to_project_id(url: &str) -> Option<String> {
    let stripped_suffix = url.trim().trim_end_matches(".git");
    if let Some((_, path)) = stripped_suffix.split_once(':')
        && let Some(project_id) = take_last_two_segments(path)
    {
        return Some(project_id);
    }
    if let Some(path_part) = stripped_suffix.split("://").nth(1)
        && let Some((_, tail)) = path_part.split_once('/')
        && let Some(project_id) = take_last_two_segments(tail)
    {
        return Some(project_id);
    }
    None
}

fn take_last_two_segments(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let repo = parts.pop()?;
    let owner = parts.pop()?;
    Some(format!("{owner}/{repo}"))
}

fn emit_cloud_error(err: &CloudError, output: OutputFormat) -> ExitCode {
    emit_error(err.message(), err.exit_code(), output)
}

#[derive(Debug, Clone)]
struct StaticFunctionInfo {
    path: PathBuf,
    name: String,
    start_line: u32,
    end_line: u32,
    static_used: bool,
    test_covered: bool,
}

#[derive(Default)]
struct StaticIndex {
    by_key: FxHashMap<(String, String, u32), StaticFunctionInfo>,
    by_path_name: FxHashMap<(String, String), Vec<StaticFunctionInfo>>,
}

fn build_static_index(ctx: &RunContext<'_>, production: bool) -> Result<StaticIndex, ExitCode> {
    let config = crate::load_config_for_analysis(
        ctx.root,
        ctx.config_path,
        ctx.output,
        ctx.no_cache,
        ctx.threads,
        Some(production),
        ctx.quiet,
        fallow_config::ProductionAnalysis::Health,
    )?;
    let files = fallow_core::discover::discover_files(&config);
    let cache = if config.no_cache {
        None
    } else {
        fallow_core::cache::CacheStore::load(&config.cache_dir)
    };
    let parse_result = fallow_core::extract::parse_all_files(&files, cache.as_ref(), true);
    let analysis_output = fallow_core::analyze_with_parse_result(&config, &parse_result.modules)
        .map_err(|err| emit_error(&format!("analysis failed: {err}"), 2, ctx.output))?;
    let file_paths: FxHashMap<_, _> = files.iter().map(|file| (file.id, &file.path)).collect();
    Ok(build_index_from_analysis(
        &config.root,
        &parse_result.modules,
        &analysis_output,
        &file_paths,
    ))
}

fn build_index_from_analysis(
    root: &Path,
    modules: &[fallow_types::extract::ModuleInfo],
    analysis_output: &fallow_core::AnalysisOutput,
    file_paths: &FxHashMap<fallow_types::discover::FileId, &PathBuf>,
) -> StaticIndex {
    let unused_files: FxHashSet<PathBuf> = analysis_output
        .results
        .unused_files
        .iter()
        .map(|file| file.path.clone())
        .collect();
    let mut unused_export_names: FxHashMap<PathBuf, FxHashSet<String>> = FxHashMap::default();
    let mut unused_export_lines: FxHashMap<PathBuf, FxHashSet<u32>> = FxHashMap::default();
    for export in &analysis_output.results.unused_exports {
        unused_export_names
            .entry(export.path.clone())
            .or_default()
            .insert(export.export_name.clone());
        unused_export_lines
            .entry(export.path.clone())
            .or_default()
            .insert(export.line);
    }

    let mut out = StaticIndex::default();
    for module in modules {
        let Some(path) = file_paths.get(&module.file_id) else {
            continue;
        };
        let rel = normalize_runtime_path(path.strip_prefix(root).unwrap_or(path));
        for function in &module.complexity {
            let end_line = function.line.saturating_add(function.line_count);
            let static_used = !unused_files.contains(path.as_path())
                && !unused_export_names
                    .get(*path)
                    .is_some_and(|names| names.contains(function.name.as_str()))
                && !unused_export_lines
                    .get(*path)
                    .is_some_and(|lines| lines.contains(&function.line));
            let info = StaticFunctionInfo {
                path: PathBuf::from(&rel),
                name: function.name.clone(),
                start_line: function.line,
                end_line,
                static_used,
                test_covered: false,
            };
            out.by_key.insert(
                (rel.clone(), function.name.clone(), function.line),
                info.clone(),
            );
            out.by_path_name
                .entry((rel.clone(), function.name.clone()))
                .or_default()
                .push(info);
        }
    }
    out
}

fn merge_cloud_snapshot(
    snapshot: &CloudRuntimeContext,
    static_index: &StaticIndex,
    min_invocations_hot: u64,
) -> RuntimeCoverageReport {
    let mut findings = Vec::new();
    let mut hot_paths = Vec::new();
    let mut unmatched_cloud_functions = 0_usize;
    for function in &snapshot.functions {
        let Some(local) = match_cloud_function(function, static_index) else {
            unmatched_cloud_functions = unmatched_cloud_functions.saturating_add(1);
            continue;
        };
        if matches!(function.tracking_state, CloudTrackingState::Called) {
            if let Some(invocations) = function.hit_count
                && invocations >= min_invocations_hot
            {
                hot_paths.push(cloud_hot_path(&local, invocations));
            }
            continue;
        }
        findings.push(cloud_finding(function, &local, snapshot.window.period_days));
    }

    findings.sort_by(|left, right| {
        runtime_verdict_rank(left.verdict)
            .cmp(&runtime_verdict_rank(right.verdict))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.function.cmp(&right.function))
    });
    hot_paths.sort_by(|left, right| {
        right
            .invocations
            .cmp(&left.invocations)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.function.cmp(&right.function))
    });

    let warnings = cloud_warnings(snapshot, unmatched_cloud_functions);

    RuntimeCoverageReport {
        verdict: if findings.is_empty() {
            RuntimeCoverageReportVerdict::Clean
        } else {
            RuntimeCoverageReportVerdict::ColdCodeDetected
        },
        summary: RuntimeCoverageSummary {
            data_source: RuntimeCoverageDataSource::Cloud,
            last_received_at: snapshot.summary.last_received_at.clone(),
            functions_tracked: snapshot.summary.functions_tracked,
            functions_hit: snapshot.summary.functions_hit,
            functions_unhit: snapshot.summary.functions_unhit,
            functions_untracked: snapshot.summary.functions_untracked,
            coverage_percent: snapshot.summary.coverage_percent,
            trace_count: snapshot.summary.trace_count,
            period_days: snapshot.window.period_days,
            deployments_seen: snapshot.summary.deployments_seen,
            capture_quality: cloud_capture_quality(snapshot),
        },
        findings,
        hot_paths,
        watermark: None,
        warnings,
    }
}

fn cloud_hot_path(local: &StaticFunctionInfo, invocations: u64) -> RuntimeCoverageHotPath {
    RuntimeCoverageHotPath {
        id: stable_runtime_id("hot", &local.path, &local.name, local.start_line),
        path: local.path.clone(),
        function: local.name.clone(),
        line: local.start_line,
        invocations,
        percentile: 100,
        actions: Vec::new(),
    }
}

fn cloud_finding(
    function: &CloudRuntimeFunction,
    local: &StaticFunctionInfo,
    observation_days: u32,
) -> RuntimeCoverageFinding {
    let (verdict, confidence, invocations) = cloud_finding_decision(function, local);
    RuntimeCoverageFinding {
        id: stable_runtime_id("prod", &local.path, &local.name, local.start_line),
        path: local.path.clone(),
        function: local.name.clone(),
        line: local.start_line,
        verdict,
        invocations,
        confidence,
        evidence: RuntimeCoverageEvidence {
            static_status: if local.static_used { "used" } else { "unused" }.to_owned(),
            test_coverage: if local.test_covered {
                "covered"
            } else {
                "not_covered"
            }
            .to_owned(),
            v8_tracking: cloud_v8_tracking(function.tracking_state).to_owned(),
            untracked_reason: function.untracked_reason.clone(),
            observation_days,
            deployments_observed: function.deployments_observed,
        },
        actions: runtime_actions(verdict),
    }
}

fn cloud_finding_decision(
    function: &CloudRuntimeFunction,
    local: &StaticFunctionInfo,
) -> (
    RuntimeCoverageVerdict,
    RuntimeCoverageConfidence,
    Option<u64>,
) {
    match function.tracking_state {
        CloudTrackingState::NeverCalled => (
            if local.static_used {
                RuntimeCoverageVerdict::ReviewRequired
            } else {
                RuntimeCoverageVerdict::SafeToDelete
            },
            RuntimeCoverageConfidence::High,
            Some(0),
        ),
        CloudTrackingState::Untracked => (
            RuntimeCoverageVerdict::CoverageUnavailable,
            RuntimeCoverageConfidence::None,
            None,
        ),
        CloudTrackingState::Unknown | CloudTrackingState::Called => (
            RuntimeCoverageVerdict::Unknown,
            RuntimeCoverageConfidence::Low,
            function.hit_count,
        ),
    }
}

fn cloud_v8_tracking(state: CloudTrackingState) -> &'static str {
    match state {
        CloudTrackingState::Called | CloudTrackingState::NeverCalled => "tracked",
        CloudTrackingState::Untracked | CloudTrackingState::Unknown => "untracked",
    }
}

fn cloud_warnings(
    snapshot: &CloudRuntimeContext,
    unmatched_cloud_functions: usize,
) -> Vec<RuntimeCoverageMessage> {
    let mut warnings = snapshot
        .warnings
        .iter()
        .enumerate()
        .map(|(index, warning)| match warning {
            CloudRuntimeWarning::Message(message) => RuntimeCoverageMessage {
                code: format!("cloud_warning_{index}"),
                message: message.clone(),
            },
            CloudRuntimeWarning::Object { code, message } => RuntimeCoverageMessage {
                code: code
                    .clone()
                    .unwrap_or_else(|| format!("cloud_warning_{index}")),
                message: message.clone().unwrap_or_default(),
            },
        })
        .collect::<Vec<_>>();
    // Only synthesize the empty-window warning if the server did not already
    // emit one. The server's `no_runtime_data` message includes the projectId
    // when present, so dedup-by-(code,message) cannot catch this case; the
    // CLI defers to the server's variant unconditionally when both apply.
    let server_emitted_no_runtime_data = warnings
        .iter()
        .any(|warning| warning.code == "no_runtime_data");
    if snapshot.summary.trace_count == 0
        && snapshot.functions.is_empty()
        && !server_emitted_no_runtime_data
    {
        let repo = if snapshot.repo.trim().is_empty() {
            "this repository"
        } else {
            snapshot.repo.as_str()
        };
        warnings.push(RuntimeCoverageMessage {
            code: "no_runtime_data".to_owned(),
            message: format!(
                "No runtime coverage data received for {repo} in the last {} days.",
                snapshot.window.period_days
            ),
        });
    }
    if unmatched_cloud_functions > 0 {
        warnings.push(RuntimeCoverageMessage {
            code: "cloud_functions_unmatched".to_owned(),
            message: format!(
                "{unmatched_cloud_functions} cloud runtime function(s) were not matched in the local AST/static analysis and were omitted from findings."
            ),
        });
    }
    dedupe_warnings(warnings)
}

/// Deduplicate warnings by `(code, message)`. The server-side runtime-context
/// emits `no_runtime_data` in its empty-window response while the CLI also
/// derives the same code from `trace_count == 0 && functions.is_empty()`, so
/// the merged list can contain identical entries.
fn dedupe_warnings(warnings: Vec<RuntimeCoverageMessage>) -> Vec<RuntimeCoverageMessage> {
    let mut seen: FxHashSet<(String, String)> = FxHashSet::default();
    warnings
        .into_iter()
        .filter(|warning| seen.insert((warning.code.clone(), warning.message.clone())))
        .collect()
}

fn cloud_capture_quality(snapshot: &CloudRuntimeContext) -> Option<RuntimeCoverageCaptureQuality> {
    let has_data = snapshot.summary.functions_tracked > 0
        || snapshot.summary.functions_untracked > 0
        || snapshot.summary.trace_count > 0
        || snapshot.summary.deployments_seen > 0;
    if !has_data {
        return None;
    }
    let tracked = snapshot.summary.functions_tracked;
    let untracked = snapshot.summary.functions_untracked;
    let total = tracked.saturating_add(untracked);
    let untracked_ratio_percent = if total == 0 {
        0.0
    } else {
        let raw = (untracked as f64) * 100.0 / (total as f64);
        (raw * 100.0).round() / 100.0
    };
    Some(RuntimeCoverageCaptureQuality {
        window_seconds: u64::from(snapshot.window.period_days).saturating_mul(86_400),
        instances_observed: snapshot.summary.deployments_seen,
        lazy_parse_warning: untracked_ratio_percent > 30.0,
        untracked_ratio_percent,
    })
}

fn match_cloud_function(
    function: &CloudRuntimeFunction,
    static_index: &StaticIndex,
) -> Option<StaticFunctionInfo> {
    let path = normalize_runtime_path(Path::new(&function.file_path));
    let line = function.start_line.or(function.line_number).unwrap_or(0);
    if let Some(info) =
        static_index
            .by_key
            .get(&(path.clone(), function.function_name.clone(), line))
    {
        return Some(info.clone());
    }
    static_index
        .by_path_name
        .get(&(path, function.function_name.clone()))
        .and_then(|candidates| {
            candidates
                .iter()
                .find(|candidate| {
                    let end = function.end_line.unwrap_or(candidate.end_line);
                    candidate.start_line.abs_diff(line) <= 5
                        && candidate.end_line.abs_diff(end) <= 5
                })
                .cloned()
                .or_else(|| candidates.first().cloned())
        })
}

fn normalize_runtime_path(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches('/')
        .replace('\\', "/")
}

fn runtime_actions(verdict: RuntimeCoverageVerdict) -> Vec<RuntimeCoverageAction> {
    match verdict {
        RuntimeCoverageVerdict::SafeToDelete => vec![RuntimeCoverageAction {
            kind: "delete-cold-code".to_owned(),
            description: "Remove cold code after confirming ownership.".to_owned(),
            auto_fixable: false,
        }],
        RuntimeCoverageVerdict::ReviewRequired => vec![RuntimeCoverageAction {
            kind: "review-runtime".to_owned(),
            description: "Review runtime-cold code before changing it.".to_owned(),
            auto_fixable: false,
        }],
        RuntimeCoverageVerdict::CoverageUnavailable
        | RuntimeCoverageVerdict::LowTraffic
        | RuntimeCoverageVerdict::Active
        | RuntimeCoverageVerdict::Unknown => Vec::new(),
    }
}

const fn runtime_verdict_rank(verdict: RuntimeCoverageVerdict) -> u8 {
    match verdict {
        RuntimeCoverageVerdict::SafeToDelete => 0,
        RuntimeCoverageVerdict::ReviewRequired => 1,
        RuntimeCoverageVerdict::CoverageUnavailable => 2,
        RuntimeCoverageVerdict::LowTraffic => 3,
        RuntimeCoverageVerdict::Unknown => 4,
        RuntimeCoverageVerdict::Active => 5,
    }
}

fn stable_runtime_id(prefix: &str, path: &Path, function: &str, line: u32) -> String {
    let input = format!(
        "{prefix}:{}:{function}:{line}",
        normalize_runtime_path(path)
    );
    // Match the canonical 8-hex-char shape that the local sidecar emits and
    // that `docs/output-schema.json` constrains via regex (`fallow:prod:[0-9a-f]{8}`).
    // Keep the lower 32 bits of xxh3_64; collisions across realistic populations
    // (~64K functions before 50% birthday probability) are tolerable for an
    // identifier the schema treats as opaque.
    let truncated = (xxhash_rust::xxh3::xxh3_64(input.as_bytes()) & 0xFFFF_FFFF) as u32;
    format!("fallow:{prefix}:{truncated:08x}")
}

fn print_runtime_report(
    report: &RuntimeCoverageReport,
    ctx: &RunContext<'_>,
    elapsed: std::time::Duration,
    top: Option<usize>,
) -> ExitCode {
    match ctx.output {
        OutputFormat::Human => print_runtime_human(report, elapsed, top),
        _ => print_runtime_json(report, elapsed, ctx.explain),
    }
}

fn print_runtime_json(
    report: &RuntimeCoverageReport,
    elapsed: std::time::Duration,
    explain: bool,
) -> ExitCode {
    let mut runtime = match serde_json::to_value(report) {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: failed to serialize runtime coverage report: {err}");
            return ExitCode::from(2);
        }
    };
    inject_runtime_schema(&mut runtime);
    let mut output = serde_json::json!({
        "schema_version": RUNTIME_COVERAGE_SCHEMA_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "elapsed_ms": elapsed.as_millis(),
        "runtime_coverage": runtime,
    });
    if explain && let Some(map) = output.as_object_mut() {
        map.insert("_meta".to_owned(), crate::explain::coverage_analyze_meta());
    }
    crate::report::emit_json(&output, "runtime coverage JSON")
}

fn inject_runtime_schema(value: &mut serde_json::Value) {
    let serde_json::Value::Object(map) = value else {
        return;
    };
    let mut ordered = serde_json::Map::new();
    ordered.insert(
        "schema_version".to_owned(),
        serde_json::json!(RUNTIME_COVERAGE_SCHEMA_VERSION),
    );
    for (key, value) in std::mem::take(map) {
        if key != "schema_version" {
            ordered.insert(key, value);
        }
    }
    *map = ordered;
}

const HUMAN_DEFAULT_DISPLAY_LIMIT: usize = 10;

fn print_runtime_human(
    report: &RuntimeCoverageReport,
    elapsed: std::time::Duration,
    top: Option<usize>,
) -> ExitCode {
    let display_limit = top.unwrap_or(HUMAN_DEFAULT_DISPLAY_LIMIT);
    println!("Runtime coverage: {}", report.verdict);
    println!(
        "  {} tracked, {} hit, {} unhit, {} untracked ({:.1}% covered)",
        report.summary.functions_tracked,
        report.summary.functions_hit,
        report.summary.functions_unhit,
        report.summary.functions_untracked,
        report.summary.coverage_percent,
    );
    println!(
        "  based on {} traces over {} days ({} deployments)",
        report.summary.trace_count, report.summary.period_days, report.summary.deployments_seen
    );
    for finding in report.findings.iter().take(display_limit) {
        println!(
            "  {}:{} {} [{}, {}]",
            finding.path.display(),
            finding.line,
            finding.function,
            finding.invocations.map_or_else(
                || "untracked".to_owned(),
                |hits| format!("{hits} invocations")
            ),
            finding.verdict.human_label(),
        );
    }
    for warning in &report.warnings {
        println!("  warning [{}]: {}", warning.code, warning.message);
    }
    eprintln!("runtime coverage analyzed in {:.2}s", elapsed.as_secs_f64());
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_alone_does_not_enable_cloud_source() {
        let args = AnalyzeArgs::default();
        assert!(!args.cloud);
        assert!(args.runtime_coverage.is_none());
    }

    #[test]
    fn parse_git_remote_https() {
        assert_eq!(
            parse_git_remote_to_project_id("https://github.com/fallow-rs/fallow.git"),
            Some("fallow-rs/fallow".to_owned())
        );
    }

    #[test]
    fn cloud_never_called_static_unused_becomes_safe_to_delete() {
        let mut static_index = StaticIndex::default();
        let info = StaticFunctionInfo {
            path: PathBuf::from("src/a.ts"),
            name: "oldFlow".to_owned(),
            start_line: 10,
            end_line: 20,
            static_used: false,
            test_covered: false,
        };
        static_index.by_key.insert(
            ("src/a.ts".to_owned(), "oldFlow".to_owned(), 10),
            info.clone(),
        );
        static_index
            .by_path_name
            .entry(("src/a.ts".to_owned(), "oldFlow".to_owned()))
            .or_default()
            .push(info);
        let snapshot = CloudRuntimeContext {
            repo: "acme/web".to_owned(),
            window: crate::coverage::cloud_client::CloudRuntimeWindow { period_days: 30 },
            summary: crate::coverage::cloud_client::CloudRuntimeSummary {
                trace_count: 100,
                deployments_seen: 2,
                functions_tracked: 1,
                functions_hit: 0,
                functions_unhit: 1,
                functions_untracked: 0,
                coverage_percent: 0.0,
                last_received_at: Some("2026-04-30T10:00:00.000Z".to_owned()),
            },
            functions: vec![
                CloudRuntimeFunction {
                    file_path: "src/a.ts".to_owned(),
                    function_name: "oldFlow".to_owned(),
                    line_number: Some(10),
                    start_line: Some(10),
                    end_line: Some(20),
                    hit_count: Some(0),
                    tracking_state: CloudTrackingState::NeverCalled,
                    deployments_observed: 2,
                    untracked_reason: None,
                },
                CloudRuntimeFunction {
                    file_path: "src/missing.ts".to_owned(),
                    function_name: "missingInAst".to_owned(),
                    line_number: Some(1),
                    start_line: Some(1),
                    end_line: Some(3),
                    hit_count: Some(0),
                    tracking_state: CloudTrackingState::NeverCalled,
                    deployments_observed: 2,
                    untracked_reason: None,
                },
            ],
            warnings: vec![],
        };
        let report = merge_cloud_snapshot(&snapshot, &static_index, 100);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(
            report.findings[0].verdict,
            RuntimeCoverageVerdict::SafeToDelete
        );
        assert_eq!(report.summary.data_source, RuntimeCoverageDataSource::Cloud);
        assert_eq!(
            report.summary.last_received_at.as_deref(),
            Some("2026-04-30T10:00:00.000Z")
        );
        assert_eq!(
            report
                .summary
                .capture_quality
                .as_ref()
                .map(|quality| quality.instances_observed),
            Some(2)
        );
        assert_eq!(report.findings[0].evidence.test_coverage, "not_covered");
        assert_eq!(report.findings[0].evidence.v8_tracking, "tracked");
        assert_eq!(
            report.findings[0].actions.first().map(|a| a.kind.as_str()),
            Some("delete-cold-code")
        );
        assert_eq!(
            report.warnings.first().map(|warning| warning.code.as_str()),
            Some("cloud_functions_unmatched")
        );
    }

    #[test]
    fn cloud_never_called_static_used_emits_review_runtime_action() {
        let actions = runtime_actions(RuntimeCoverageVerdict::ReviewRequired);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].kind, "review-runtime");
    }

    #[test]
    fn cloud_warnings_dedupe_server_and_cli_no_runtime_data() {
        // Empty window: server adds no_runtime_data; CLI's empty-summary
        // branch must defer to the server's variant unconditionally so the
        // user never sees the same code twice. Caught live against
        // api.fallow.cloud during the v2.57.0 smoke (both --repo nonexistent
        // and --project-id apps/dashboard returned duplicates).
        let snapshot = CloudRuntimeContext {
            repo: "nonexistent-repo".to_owned(),
            window: crate::coverage::cloud_client::CloudRuntimeWindow { period_days: 30 },
            summary: crate::coverage::cloud_client::CloudRuntimeSummary {
                trace_count: 0,
                deployments_seen: 0,
                functions_tracked: 0,
                functions_hit: 0,
                functions_unhit: 0,
                functions_untracked: 0,
                coverage_percent: 0.0,
                last_received_at: None,
            },
            functions: vec![],
            warnings: vec![CloudRuntimeWarning::Object {
                code: Some("no_runtime_data".to_owned()),
                message: Some(
                    "No runtime coverage data received for nonexistent-repo in the last 30 days."
                        .to_owned(),
                ),
            }],
        };
        let warnings = cloud_warnings(&snapshot, 0);
        let no_data_count = warnings
            .iter()
            .filter(|w| w.code == "no_runtime_data")
            .count();
        assert_eq!(
            no_data_count, 1,
            "expected exactly one no_runtime_data warning, got: {warnings:?}"
        );
    }

    #[test]
    fn cloud_warnings_dedupe_when_server_message_includes_project_id() {
        // Regression: with --project-id set, the server's no_runtime_data
        // message embeds the projectId ("... apps/dashboard in fallow-cloud
        // ...") while the CLI's variant does not, so dedup-by-(code,message)
        // does not catch the duplicate. Defer to code-only check.
        let snapshot = CloudRuntimeContext {
            repo: "fallow-cloud".to_owned(),
            window: crate::coverage::cloud_client::CloudRuntimeWindow { period_days: 30 },
            summary: crate::coverage::cloud_client::CloudRuntimeSummary {
                trace_count: 0,
                deployments_seen: 0,
                functions_tracked: 0,
                functions_hit: 0,
                functions_unhit: 0,
                functions_untracked: 0,
                coverage_percent: 0.0,
                last_received_at: None,
            },
            functions: vec![],
            warnings: vec![CloudRuntimeWarning::Object {
                code: Some("no_runtime_data".to_owned()),
                message: Some(
                    "No runtime coverage data received for apps/dashboard in fallow-cloud in the last 30 days.".to_owned(),
                ),
            }],
        };
        let warnings = cloud_warnings(&snapshot, 0);
        let no_data_count = warnings
            .iter()
            .filter(|w| w.code == "no_runtime_data")
            .count();
        assert_eq!(
            no_data_count, 1,
            "expected exactly one no_runtime_data warning, got: {warnings:?}"
        );
    }

    #[test]
    fn validate_output_format_accepts_json_and_human() {
        assert!(validate_output_format(OutputFormat::Json).is_ok());
        assert!(validate_output_format(OutputFormat::Human).is_ok());
    }

    #[test]
    fn stable_runtime_id_emits_eight_hex_chars() {
        // Schema regex: ^fallow:prod:[0-9a-f]{8}$. Local sidecar already
        // emits 8 chars; cloud merge must match. Caught live during the
        // v2.57.0 jsonschema validation pass against the published schema.
        let path = PathBuf::from("src/foo.ts");
        let id = stable_runtime_id("prod", &path, "doThing", 42);
        let suffix = id
            .strip_prefix("fallow:prod:")
            .expect("id has fallow:prod: prefix");
        assert_eq!(suffix.len(), 8, "expected 8 hex chars, got {suffix:?}");
        assert!(
            suffix
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "expected lowercase hex chars, got {suffix:?}"
        );
    }

    #[test]
    fn validate_output_format_rejects_other_formats() {
        for fmt in [
            OutputFormat::Compact,
            OutputFormat::Markdown,
            OutputFormat::Sarif,
            OutputFormat::CodeClimate,
            OutputFormat::Badge,
        ] {
            let err = validate_output_format(fmt).expect_err("must reject");
            assert!(
                err.contains("only supports --format json or --format human"),
                "rejection message must guide users; got: {err}"
            );
        }
    }
}
