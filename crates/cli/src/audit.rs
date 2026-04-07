use std::process::ExitCode;
use std::time::{Duration, Instant};

use colored::Colorize;
use fallow_config::OutputFormat;

use crate::check::{CheckOptions, CheckResult, IssueFilters, TraceOptions};
use crate::dupes::{DupesMode, DupesOptions, DupesResult};
use crate::error::emit_error;
use crate::health::{HealthOptions, HealthResult, SortBy};
use crate::report;
use crate::report::plural;

// ── Types ────────────────────────────────────────────────────────

/// Verdict for the audit command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    /// No issues in changed files.
    Pass,
    /// Issues found, but all are warn-severity.
    Warn,
    /// Error-severity issues found in changed files.
    Fail,
}

/// Per-category summary counts for the audit result.
#[derive(Debug, serde::Serialize)]
pub struct AuditSummary {
    pub dead_code_issues: usize,
    pub dead_code_has_errors: bool,
    pub complexity_findings: usize,
    pub max_cyclomatic: Option<u16>,
    pub duplication_clone_groups: usize,
}

/// Full audit result containing verdict, summary, and sub-results.
pub struct AuditResult {
    pub verdict: AuditVerdict,
    pub summary: AuditSummary,
    pub changed_files_count: usize,
    pub base_ref: String,
    pub head_sha: Option<String>,
    pub output: OutputFormat,
    pub check: Option<CheckResult>,
    pub dupes: Option<DupesResult>,
    pub health: Option<HealthResult>,
    pub elapsed: Duration,
}

pub struct AuditOptions<'a> {
    pub root: &'a std::path::Path,
    pub config_path: &'a Option<std::path::PathBuf>,
    pub output: OutputFormat,
    pub no_cache: bool,
    pub threads: usize,
    pub quiet: bool,
    pub changed_since: Option<&'a str>,
    pub production: bool,
    pub workspace: Option<&'a str>,
    pub explain: bool,
    pub performance: bool,
    pub group_by: Option<crate::GroupBy>,
}

// ── Auto-detect base branch ──────────────────────────────────────

/// Try to determine the default branch for the repository.
/// Priority: `git symbolic-ref refs/remotes/origin/HEAD` → `main` → `master`.
/// Returns `None` if none of these exist.
fn auto_detect_base_branch(root: &std::path::Path) -> Option<String> {
    // Try symbolic-ref first (works when origin HEAD is set)
    if let Ok(output) = std::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(root)
        .output()
        && output.status.success()
    {
        let full_ref = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(branch) = full_ref.strip_prefix("refs/remotes/origin/") {
            return Some(branch.to_string());
        }
    }

    // Try main
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "main"])
        .current_dir(root)
        .output()
        && output.status.success()
    {
        return Some("main".to_string());
    }

    // Try master
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "master"])
        .current_dir(root)
        .output()
        && output.status.success()
    {
        return Some("master".to_string());
    }

    None
}

/// Get the short SHA of HEAD for the scope display line.
fn get_head_sha(root: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

// ── Verdict computation ──────────────────────────────────────────

fn compute_verdict(
    check: Option<&CheckResult>,
    dupes: Option<&DupesResult>,
    health: Option<&HealthResult>,
) -> AuditVerdict {
    let mut has_errors = false;
    let mut has_warnings = false;

    // Dead code: use rules severity
    if let Some(result) = check {
        if crate::check::has_error_severity_issues(
            &result.results,
            &result.config.rules,
            Some(&result.config),
        ) {
            has_errors = true;
        } else if result.results.total_issues() > 0 {
            has_warnings = true;
        }
    }

    // Complexity: findings that exceeded configured thresholds are always errors.
    // Health rules don't have a warn-severity concept — any finding above the
    // threshold is a quality gate failure, matching `fallow health` exit code semantics.
    if let Some(result) = health
        && !result.report.findings.is_empty()
    {
        has_errors = true;
    }

    // Duplication: clone groups are warnings (unless threshold exceeded)
    if let Some(result) = dupes
        && !result.report.clone_groups.is_empty()
    {
        if result.threshold > 0.0 && result.report.stats.duplication_percentage > result.threshold {
            has_errors = true;
        } else {
            has_warnings = true;
        }
    }

    if has_errors {
        AuditVerdict::Fail
    } else if has_warnings {
        AuditVerdict::Warn
    } else {
        AuditVerdict::Pass
    }
}

fn build_summary(
    check: Option<&CheckResult>,
    dupes: Option<&DupesResult>,
    health: Option<&HealthResult>,
) -> AuditSummary {
    let dead_code_issues = check.map_or(0, |r| r.results.total_issues());
    let dead_code_has_errors = check.is_some_and(|r| {
        crate::check::has_error_severity_issues(&r.results, &r.config.rules, Some(&r.config))
    });
    let complexity_findings = health.map_or(0, |r| r.report.findings.len());
    let max_cyclomatic = health.and_then(|r| r.report.findings.iter().map(|f| f.cyclomatic).max());
    let duplication_clone_groups = dupes.map_or(0, |r| r.report.clone_groups.len());

    AuditSummary {
        dead_code_issues,
        dead_code_has_errors,
        complexity_findings,
        max_cyclomatic,
        duplication_clone_groups,
    }
}

// ── Execute ──────────────────────────────────────────────────────

/// Run the audit pipeline: resolve base ref, run analyses, compute verdict.
pub fn execute_audit(opts: &AuditOptions<'_>) -> Result<AuditResult, ExitCode> {
    let start = Instant::now();

    let base_ref = resolve_base_ref(opts)?;

    // Get changed files (hard error if it fails, unlike combined mode)
    let Some(changed_files) = crate::check::get_changed_files(opts.root, &base_ref) else {
        return Err(emit_error(
            &format!(
                "could not determine changed files for base ref '{base_ref}'. Verify the ref exists in this git repository"
            ),
            2,
            opts.output,
        ));
    };
    let changed_files_count = changed_files.len();

    if changed_files.is_empty() {
        return Ok(empty_audit_result(base_ref, opts, start.elapsed()));
    }

    let changed_since = Some(base_ref.as_str());

    // Run all three analyses
    let check_result = run_audit_check(opts, changed_since)?;
    let dupes_result = run_audit_dupes(opts, changed_since)?;
    let health_result = run_audit_health(opts, changed_since)?;

    let verdict = compute_verdict(
        check_result.as_ref(),
        dupes_result.as_ref(),
        health_result.as_ref(),
    );
    let summary = build_summary(
        check_result.as_ref(),
        dupes_result.as_ref(),
        health_result.as_ref(),
    );

    Ok(AuditResult {
        verdict,
        summary,
        changed_files_count,
        base_ref,
        head_sha: get_head_sha(opts.root),
        output: opts.output,
        check: check_result,
        dupes: dupes_result,
        health: health_result,
        elapsed: start.elapsed(),
    })
}

/// Resolve the base ref: explicit --changed-since / --base, or auto-detect.
fn resolve_base_ref(opts: &AuditOptions<'_>) -> Result<String, ExitCode> {
    if let Some(ref_str) = opts.changed_since {
        return Ok(ref_str.to_string());
    }
    let Some(branch) = auto_detect_base_branch(opts.root) else {
        return Err(emit_error(
            "could not detect base branch. Use --base <ref> to specify the comparison target (e.g., --base main)",
            2,
            opts.output,
        ));
    };
    // Validate auto-detected branch name (explicit --changed-since is validated in main.rs)
    if let Err(e) = crate::validate::validate_git_ref(&branch) {
        return Err(emit_error(
            &format!("auto-detected base branch '{branch}' is not a valid git ref: {e}"),
            2,
            opts.output,
        ));
    }
    Ok(branch)
}

/// Build an empty pass result when no files have changed.
fn empty_audit_result(base_ref: String, opts: &AuditOptions<'_>, elapsed: Duration) -> AuditResult {
    AuditResult {
        verdict: AuditVerdict::Pass,
        summary: AuditSummary {
            dead_code_issues: 0,
            dead_code_has_errors: false,
            complexity_findings: 0,
            max_cyclomatic: None,
            duplication_clone_groups: 0,
        },
        changed_files_count: 0,
        base_ref,
        head_sha: get_head_sha(opts.root),
        output: opts.output,
        check: None,
        dupes: None,
        health: None,
        elapsed,
    }
}

/// Run dead code analysis for the audit pipeline.
fn run_audit_check<'a>(
    opts: &'a AuditOptions<'a>,
    changed_since: Option<&'a str>,
) -> Result<Option<CheckResult>, ExitCode> {
    let filters = IssueFilters::default();
    let trace_opts = TraceOptions {
        trace_export: None,
        trace_file: None,
        trace_dependency: None,
        performance: opts.performance,
    };
    match crate::check::execute_check(&CheckOptions {
        root: opts.root,
        config_path: opts.config_path,
        output: opts.output,
        no_cache: opts.no_cache,
        threads: opts.threads,
        quiet: opts.quiet,
        fail_on_issues: false,
        filters: &filters,
        changed_since,
        baseline: None,
        save_baseline: None,
        sarif_file: None,
        production: opts.production,
        workspace: opts.workspace,
        group_by: opts.group_by,
        include_dupes: false,
        trace_opts: &trace_opts,
        explain: opts.explain,
        top: None,
        summary: false,
        regression_opts: crate::regression::RegressionOpts {
            fail_on_regression: false,
            tolerance: crate::regression::Tolerance::Absolute(0),
            regression_baseline_file: None,
            save_target: crate::regression::SaveRegressionTarget::None,
            scoped: true,
            quiet: opts.quiet,
        },
    }) {
        Ok(r) => Ok(Some(r)),
        Err(code) => Err(code),
    }
}

/// Run duplication analysis for the audit pipeline.
fn run_audit_dupes<'a>(
    opts: &'a AuditOptions<'a>,
    changed_since: Option<&'a str>,
) -> Result<Option<DupesResult>, ExitCode> {
    match crate::dupes::execute_dupes(&DupesOptions {
        root: opts.root,
        config_path: opts.config_path,
        output: opts.output,
        no_cache: opts.no_cache,
        threads: opts.threads,
        quiet: opts.quiet,
        mode: DupesMode::Mild,
        min_tokens: 50,
        min_lines: 5,
        threshold: 0.0,
        skip_local: false,
        cross_language: false,
        top: None,
        baseline_path: None,
        save_baseline_path: None,
        production: opts.production,
        trace: None,
        changed_since,
        explain: opts.explain,
        summary: false,
        group_by: opts.group_by,
    }) {
        Ok(r) => Ok(Some(r)),
        Err(code) => Err(code),
    }
}

/// Run complexity analysis for the audit pipeline (findings only, no scores/hotspots/targets).
fn run_audit_health<'a>(
    opts: &'a AuditOptions<'a>,
    changed_since: Option<&'a str>,
) -> Result<Option<HealthResult>, ExitCode> {
    match crate::health::execute_health(&HealthOptions {
        root: opts.root,
        config_path: opts.config_path,
        output: opts.output,
        no_cache: opts.no_cache,
        threads: opts.threads,
        quiet: opts.quiet,
        max_cyclomatic: None,
        max_cognitive: None,
        top: None,
        sort: SortBy::Cyclomatic,
        production: opts.production,
        changed_since,
        workspace: opts.workspace,
        baseline: None,
        save_baseline: None,
        complexity: true,
        file_scores: false,
        coverage_gaps: false,
        hotspots: false,
        targets: false,
        effort: None,
        score: false,
        min_score: None,
        since: None,
        min_commits: None,
        explain: opts.explain,
        summary: false,
        save_snapshot: None,
        trend: false,
        group_by: opts.group_by,
    }) {
        Ok(r) => Ok(Some(r)),
        Err(code) => Err(code),
    }
}

// ── Print ────────────────────────────────────────────────────────

/// Print audit results and return the appropriate exit code.
#[must_use]
pub fn print_audit_result(result: &AuditResult, quiet: bool, explain: bool) -> ExitCode {
    let output = result.output;

    let format_exit = match output {
        OutputFormat::Json => print_audit_json(result),
        OutputFormat::Human | OutputFormat::Compact | OutputFormat::Markdown => {
            print_audit_human(result, quiet, explain, output);
            ExitCode::SUCCESS
        }
        OutputFormat::Sarif => print_audit_sarif(result),
        OutputFormat::CodeClimate => print_audit_codeclimate(result),
        OutputFormat::Badge => {
            eprintln!("Error: badge format is not supported for the audit command");
            return ExitCode::from(2);
        }
    };

    if format_exit != ExitCode::SUCCESS {
        return format_exit;
    }

    match result.verdict {
        AuditVerdict::Fail => ExitCode::from(1),
        AuditVerdict::Pass | AuditVerdict::Warn => ExitCode::SUCCESS,
    }
}

// ── Human format ─────────────────────────────────────────────────

fn print_audit_human(result: &AuditResult, quiet: bool, explain: bool, output: OutputFormat) {
    let show_headers = matches!(output, OutputFormat::Human) && !quiet;

    // Scope line (stderr)
    if !quiet {
        let scope = format_scope_line(result);
        eprintln!();
        eprintln!("{scope}");
    }

    let has_check_issues = result.summary.dead_code_issues > 0;
    let has_health_findings = result.summary.complexity_findings > 0;
    let has_dupe_groups = result.summary.duplication_clone_groups > 0;
    let has_any_findings = has_check_issues || has_health_findings || has_dupe_groups;

    // On fail/warn with findings: show detail sections (reuse existing renderers)
    if has_any_findings {
        // Vital signs summary line (stdout) — only when verdict is pass/warn
        if result.verdict != AuditVerdict::Fail && !quiet {
            print_audit_vital_signs(result);
        }

        if has_check_issues && let Some(ref check) = result.check {
            if show_headers {
                eprintln!();
                eprintln!("── Dead Code ──────────────────────────────────────");
            }
            crate::check::print_check_result(check, quiet, explain, false, None, None, false);
        }

        if has_dupe_groups && let Some(ref dupes) = result.dupes {
            if show_headers {
                eprintln!();
                eprintln!("── Duplication ────────────────────────────────────");
            }
            crate::dupes::print_dupes_result(dupes, quiet, explain, false);
        }

        if has_health_findings && let Some(ref health) = result.health {
            if show_headers {
                eprintln!();
                eprintln!("── Complexity ─────────────────────────────────────");
            }
            crate::health::print_health_result(health, quiet, explain, None, false);
        }
    }

    // Status line (stderr) — always last
    if !quiet {
        print_audit_status_line(result);
    }
}

/// Format the scope context line.
fn format_scope_line(result: &AuditResult) -> String {
    let sha_suffix = result
        .head_sha
        .as_ref()
        .map_or(String::new(), |sha| format!(" ({sha}..HEAD)"));
    format!(
        "Audit scope: {} changed file{} vs {}{}",
        result.changed_files_count,
        plural(result.changed_files_count),
        result.base_ref,
        sha_suffix
    )
}

/// Print a dimmed vital-signs line summarizing warn-only findings.
fn print_audit_vital_signs(result: &AuditResult) {
    let mut parts = Vec::new();
    parts.push(format!("dead code {}", result.summary.dead_code_issues));
    if let Some(max) = result.summary.max_cyclomatic {
        parts.push(format!(
            "complexity {} (warn, max cyclomatic: {max})",
            result.summary.complexity_findings
        ));
    } else {
        parts.push(format!("complexity {}", result.summary.complexity_findings));
    }
    parts.push(format!(
        "duplication {}",
        result.summary.duplication_clone_groups
    ));

    let line = parts.join(" \u{00b7} ");
    println!(
        "{} {} {}",
        "\u{25a0}".dimmed(),
        "Metrics:".dimmed(),
        line.dimmed()
    );
}

/// Build summary parts for the status line (shared between warn and fail).
fn build_status_parts(summary: &AuditSummary) -> Vec<String> {
    let mut parts = Vec::new();
    if summary.dead_code_issues > 0 {
        let n = summary.dead_code_issues;
        parts.push(format!("dead code: {n} issue{}", plural(n)));
    }
    if summary.complexity_findings > 0 {
        let n = summary.complexity_findings;
        parts.push(format!("complexity: {n} finding{}", plural(n)));
    }
    if summary.duplication_clone_groups > 0 {
        let n = summary.duplication_clone_groups;
        parts.push(format!("duplication: {n} clone group{}", plural(n)));
    }
    parts
}

/// Print the final status line on stderr.
fn print_audit_status_line(result: &AuditResult) {
    let elapsed_str = format!("{:.2}s", result.elapsed.as_secs_f64());
    let n = result.changed_files_count;
    let files_str = format!("{n} changed file{}", plural(n));

    match result.verdict {
        AuditVerdict::Pass => {
            eprintln!(
                "{}",
                format!("\u{2713} No issues in {files_str} ({elapsed_str})")
                    .green()
                    .bold()
            );
        }
        AuditVerdict::Warn => {
            let summary = build_status_parts(&result.summary).join(" \u{00b7} ");
            eprintln!(
                "{}",
                format!("\u{2713} {summary} (warn) \u{00b7} {files_str} ({elapsed_str})")
                    .green()
                    .bold()
            );
        }
        AuditVerdict::Fail => {
            let summary = build_status_parts(&result.summary).join(" \u{00b7} ");
            eprintln!(
                "{}",
                format!("\u{2717} {summary} \u{00b7} {files_str} ({elapsed_str})")
                    .red()
                    .bold()
            );
        }
    }
}

// ── JSON format ──────────────────────────────────────────────────

#[expect(
    clippy::cast_possible_truncation,
    reason = "elapsed milliseconds won't exceed u64::MAX"
)]
fn print_audit_json(result: &AuditResult) -> ExitCode {
    let mut obj = serde_json::Map::new();
    obj.insert("schema_version".into(), serde_json::Value::Number(3.into()));
    obj.insert(
        "version".into(),
        serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    obj.insert(
        "command".into(),
        serde_json::Value::String("audit".to_string()),
    );
    obj.insert(
        "verdict".into(),
        serde_json::to_value(result.verdict).unwrap_or(serde_json::Value::Null),
    );
    obj.insert(
        "changed_files_count".into(),
        serde_json::Value::Number(result.changed_files_count.into()),
    );
    obj.insert(
        "base_ref".into(),
        serde_json::Value::String(result.base_ref.clone()),
    );
    if let Some(ref sha) = result.head_sha {
        obj.insert("head_sha".into(), serde_json::Value::String(sha.clone()));
    }
    obj.insert(
        "elapsed_ms".into(),
        serde_json::Value::Number(serde_json::Number::from(result.elapsed.as_millis() as u64)),
    );

    // Summary
    if let Ok(summary_val) = serde_json::to_value(&result.summary) {
        obj.insert("summary".into(), summary_val);
    }

    // Full sub-results
    if let Some(ref check) = result.check {
        match report::build_json(&check.results, &check.config.root, check.elapsed) {
            Ok(json) => {
                obj.insert("dead_code".into(), json);
            }
            Err(e) => {
                return emit_error(
                    &format!("JSON serialization error: {e}"),
                    2,
                    OutputFormat::Json,
                );
            }
        }
    }

    if let Some(ref dupes) = result.dupes {
        match serde_json::to_value(&dupes.report) {
            Ok(mut json) => {
                report::inject_dupes_actions(&mut json);
                obj.insert("duplication".into(), json);
            }
            Err(e) => {
                return emit_error(
                    &format!("JSON serialization error: {e}"),
                    2,
                    OutputFormat::Json,
                );
            }
        }
    }

    if let Some(ref health) = result.health {
        match serde_json::to_value(&health.report) {
            Ok(mut json) => {
                report::inject_health_actions(&mut json);
                obj.insert("complexity".into(), json);
            }
            Err(e) => {
                return emit_error(
                    &format!("JSON serialization error: {e}"),
                    2,
                    OutputFormat::Json,
                );
            }
        }
    }

    report::emit_json(&serde_json::Value::Object(obj), "audit")
}

// ── SARIF format ─────────────────────────────────────────────────

fn print_audit_sarif(result: &AuditResult) -> ExitCode {
    let mut all_runs = Vec::new();

    if let Some(ref check) = result.check {
        let sarif = report::build_sarif(&check.results, &check.config.root, &check.config.rules);
        if let Some(runs) = sarif.get("runs").and_then(|r| r.as_array()) {
            all_runs.extend(runs.iter().cloned());
        }
    }

    if let Some(ref dupes) = result.dupes
        && !dupes.report.clone_groups.is_empty()
    {
        let run = serde_json::json!({
            "tool": {
                "driver": {
                    "name": "fallow",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/fallow-rs/fallow",
                }
            },
            "automationDetails": { "id": "fallow/audit/dupes" },
            "results": dupes.report.clone_groups.iter().enumerate().map(|(i, g)| {
                serde_json::json!({
                    "ruleId": "fallow/code-duplication",
                    "level": "warning",
                    "message": { "text": format!("Clone group {} ({} lines, {} instances)", i + 1, g.line_count, g.instances.len()) },
                })
            }).collect::<Vec<_>>()
        });
        all_runs.push(run);
    }

    if let Some(ref health) = result.health {
        let sarif = report::build_health_sarif(&health.report, &health.config.root);
        if let Some(runs) = sarif.get("runs").and_then(|r| r.as_array()) {
            all_runs.extend(runs.iter().cloned());
        }
    }

    let combined = serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": all_runs,
    });

    report::emit_json(&combined, "SARIF audit")
}

// ── CodeClimate format ───────────────────────────────────────────

fn print_audit_codeclimate(result: &AuditResult) -> ExitCode {
    let mut all_issues = Vec::new();

    if let Some(ref check) = result.check
        && let serde_json::Value::Array(items) =
            report::build_codeclimate(&check.results, &check.config.root, &check.config.rules)
    {
        all_issues.extend(items);
    }

    if let Some(ref dupes) = result.dupes
        && let serde_json::Value::Array(items) =
            report::build_duplication_codeclimate(&dupes.report, &dupes.config.root)
    {
        all_issues.extend(items);
    }

    if let Some(ref health) = result.health
        && let serde_json::Value::Array(items) =
            report::build_health_codeclimate(&health.report, &health.config.root)
    {
        all_issues.extend(items);
    }

    report::emit_json(&serde_json::Value::Array(all_issues), "CodeClimate audit")
}

// ── Entry point ──────────────────────────────────────────────────

/// Run the full audit command: execute analyses, print results, return exit code.
pub fn run_audit(opts: &AuditOptions<'_>) -> ExitCode {
    match execute_audit(opts) {
        Ok(result) => print_audit_result(&result, opts.quiet, opts.explain),
        Err(code) => code,
    }
}
