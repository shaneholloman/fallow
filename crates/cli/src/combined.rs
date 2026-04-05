use std::process::ExitCode;
use std::time::Instant;

use colored::Colorize;
use fallow_config::OutputFormat;

use crate::check::{CheckOptions, CheckResult, IssueFilters, TraceOptions};
use crate::dupes::{DupesMode, DupesOptions, DupesResult};
use crate::health::{HealthOptions, HealthResult, SortBy};
use crate::regression;
use crate::report;
use crate::{AnalysisKind, emit_error};

pub struct CombinedOptions<'a> {
    pub root: &'a std::path::Path,
    pub config_path: &'a Option<std::path::PathBuf>,
    pub output: OutputFormat,
    pub no_cache: bool,
    pub threads: usize,
    pub quiet: bool,
    pub fail_on_issues: bool,
    pub sarif_file: Option<&'a std::path::Path>,
    pub changed_since: Option<&'a str>,
    pub baseline: Option<&'a std::path::Path>,
    pub save_baseline: Option<&'a std::path::Path>,
    pub production: bool,
    pub workspace: Option<&'a str>,
    pub group_by: Option<crate::GroupBy>,
    pub explain: bool,
    pub performance: bool,
    pub summary: bool,
    pub run_check: bool,
    pub run_dupes: bool,
    pub run_health: bool,
    pub regression_opts: regression::RegressionOpts<'a>,
}

/// Resolve which analyses to run based on --only/--skip flags.
/// Precondition: only and skip must not both be non-empty (validated in main.rs).
pub fn resolve_analyses(only: &[AnalysisKind], skip: &[AnalysisKind]) -> (bool, bool, bool) {
    if !only.is_empty() {
        (
            only.contains(&AnalysisKind::DeadCode),
            only.contains(&AnalysisKind::Dupes),
            only.contains(&AnalysisKind::Health),
        )
    } else if !skip.is_empty() {
        (
            !skip.contains(&AnalysisKind::DeadCode),
            !skip.contains(&AnalysisKind::Dupes),
            !skip.contains(&AnalysisKind::Health),
        )
    } else {
        (true, true, true)
    }
}

pub fn run_combined(opts: &CombinedOptions<'_>) -> ExitCode {
    let start = Instant::now();
    let mut max_exit: u8 = 0;

    let mut check_result: Option<CheckResult> = None;
    let mut dupes_result: Option<DupesResult> = None;
    let mut health_result: Option<HealthResult> = None;

    // Run check (dead code analysis)
    if opts.run_check {
        let filters = IssueFilters::default();
        let trace_opts = TraceOptions {
            trace_export: None,
            trace_file: None,
            trace_dependency: None,
            performance: opts.performance,
        };
        let check_opts = CheckOptions {
            root: opts.root,
            config_path: opts.config_path,
            output: opts.output,
            no_cache: opts.no_cache,
            threads: opts.threads,
            quiet: opts.quiet,
            fail_on_issues: opts.fail_on_issues,
            filters: &filters,
            changed_since: opts.changed_since,
            baseline: opts.baseline,
            save_baseline: opts.save_baseline,
            sarif_file: opts.sarif_file,
            production: opts.production,
            workspace: opts.workspace,
            group_by: opts.group_by,
            include_dupes: false,
            trace_opts: &trace_opts,
            explain: opts.explain,
            top: None,
            summary: opts.summary,
            regression_opts: opts.regression_opts,
        };
        match crate::check::execute_check(&check_opts) {
            Ok(result) => {
                check_result = Some(result);
            }
            Err(code) => return code,
        }
    }

    // Run dupes (duplication analysis)
    if opts.run_dupes {
        let dupes_opts = DupesOptions {
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
            changed_since: opts.changed_since,
            explain: opts.explain,
            summary: opts.summary,
            group_by: opts.group_by,
        };
        match crate::dupes::execute_dupes(&dupes_opts) {
            Ok(result) => {
                dupes_result = Some(result);
            }
            Err(code) => return code,
        }
    }

    // Run health (complexity analysis)
    if opts.run_health {
        let health_opts = build_health_opts(opts);
        match crate::health::execute_health(&health_opts) {
            Ok(result) => {
                health_result = Some(result);
            }
            Err(code) => return code,
        }
    }

    let total_elapsed = start.elapsed();

    // Build ownership resolver once for human/compact/markdown rendering.
    // Structured formats (JSON/SARIF/CodeClimate) have their own envelope and skip grouping.
    let codeowners_cfg = check_result
        .as_ref()
        .map(|r| &r.config)
        .or_else(|| health_result.as_ref().map(|r| &r.config))
        .or_else(|| dupes_result.as_ref().map(|r| &r.config))
        .and_then(|c| c.codeowners.as_deref());
    let resolver = match crate::build_ownership_resolver(
        opts.group_by,
        opts.root,
        codeowners_cfg,
        opts.output,
    ) {
        Ok(r) => r,
        Err(code) => return code,
    };

    // Print combined report
    match opts.output {
        OutputFormat::Json => {
            // JSON: single combined object with check/dupes/health keys
            let code = print_combined_json(
                check_result.as_ref(),
                dupes_result.as_ref(),
                health_result.as_ref(),
                total_elapsed,
                opts.explain,
            );
            if code != ExitCode::SUCCESS {
                return code;
            }
        }
        OutputFormat::Sarif => {
            // SARIF: multi-run document with one run per analysis
            let code = print_combined_sarif(
                check_result.as_ref(),
                dupes_result.as_ref(),
                health_result.as_ref(),
            );
            if code != ExitCode::SUCCESS {
                return code;
            }
        }
        OutputFormat::CodeClimate => {
            // CodeClimate: single JSON array merging all analyses
            let code = print_combined_codeclimate(
                check_result.as_ref(),
                dupes_result.as_ref(),
                health_result.as_ref(),
            );
            if code != ExitCode::SUCCESS {
                return code;
            }
        }
        _ => {
            // Human/Compact/Markdown: print each section sequentially
            let show_headers = matches!(opts.output, OutputFormat::Human) && !opts.quiet;

            // Orientation header: vital signs + analysis scope + start-here nudge
            if show_headers && let Some(ref result) = health_result {
                print_orientation_header(result, check_result.as_ref());
            } else if show_headers && let Some(ref result) = check_result {
                // No health results (e.g., --only dead-code): show entry-point summary alone
                print_entry_point_summary(&result.results);
            }

            if let Some(ref result) = check_result {
                if show_headers {
                    eprintln!();
                    eprintln!("── Dead Code ──────────────────────────────────────");
                }
                let code = crate::check::print_check_result(
                    result,
                    opts.quiet,
                    opts.explain,
                    false,
                    resolver,
                    None,
                    opts.summary,
                );
                max_exit = max_exit.max(exit_code_to_u8(code));
            }

            if let Some(ref result) = dupes_result {
                if show_headers {
                    eprintln!();
                    eprintln!("── Duplication ────────────────────────────────────");
                }
                let code = crate::dupes::print_dupes_result(
                    result,
                    opts.quiet,
                    opts.explain,
                    opts.summary,
                );
                max_exit = max_exit.max(exit_code_to_u8(code));
            }

            if let Some(ref result) = health_result {
                if show_headers {
                    eprintln!();
                    eprintln!("── Complexity ─────────────────────────────────────");
                }
                let code = crate::health::print_health_result(
                    result,
                    opts.quiet,
                    opts.explain,
                    None,
                    opts.summary,
                );
                max_exit = max_exit.max(exit_code_to_u8(code));
            }
        }
    }

    // Regression exit code (applies regardless of output format)
    if let Some(ref result) = check_result
        && let Some(ref outcome) = result.regression
    {
        if !opts.quiet {
            regression::print_regression_outcome(outcome);
        }
        if outcome.is_failure() {
            max_exit = max_exit.max(1);
        }
    }

    // Summary on failure
    if max_exit > 0 && !opts.quiet {
        let mut parts = Vec::new();
        if let Some(ref r) = check_result {
            let issues = r.results.total_issues();
            if issues > 0 {
                let delta_suffix = r
                    .baseline_deltas
                    .as_ref()
                    .map_or_else(String::new, |d| match d.total_delta.cmp(&0) {
                        std::cmp::Ordering::Greater => {
                            format!(", +{} since baseline", d.total_delta)
                        }
                        std::cmp::Ordering::Less => format!(", {} since baseline", d.total_delta),
                        std::cmp::Ordering::Equal => ", \u{00b1}0 since baseline".to_string(),
                    });
                parts.push(format!("check ({issues} issues{delta_suffix})"));
            }
        }
        if let Some(ref r) = dupes_result {
            let groups = r.report.clone_groups.len();
            if groups > 0 {
                parts.push(format!("dupes ({groups} clone groups)"));
            }
        }
        if let Some(ref r) = health_result {
            let above = r.report.summary.functions_above_threshold;
            if above > 0 {
                parts.push(format!("health ({above} above threshold)"));
            }
        }
        if !parts.is_empty() {
            // Repeat start-here nudge so it's visible at the bottom of scrolled output
            let nudge = health_result
                .as_ref()
                .filter(|r| !r.report.targets.is_empty())
                .map(|r| {
                    // Prefer non-test/fixture target; skip nudge if all targets are noise
                    if let Some(top) = r.report.targets.iter().find(|t| !is_test_path(&t.path)) {
                        let name = top
                            .path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();
                        format!(" \u{2014} start with {name}")
                    } else {
                        String::new()
                    }
                })
                .unwrap_or_default();
            eprintln!("\nFailed: {}{nudge}", parts.join(", "));
        }
    }

    ExitCode::from(max_exit)
}

/// Print combined JSON output wrapping check, dupes, and health results.
#[expect(
    clippy::cast_possible_truncation,
    reason = "elapsed milliseconds won't exceed u64::MAX"
)]
fn print_combined_json(
    check: Option<&CheckResult>,
    dupes: Option<&DupesResult>,
    health: Option<&HealthResult>,
    elapsed: std::time::Duration,
    _explain: bool,
) -> ExitCode {
    let mut combined = serde_json::Map::new();
    combined.insert("schema_version".into(), serde_json::Value::Number(3.into()));
    combined.insert(
        "version".into(),
        serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    combined.insert(
        "elapsed_ms".into(),
        serde_json::Value::Number(serde_json::Number::from(elapsed.as_millis() as u64)),
    );

    if let Some(result) = check {
        match report::build_json(&result.results, &result.config.root, result.elapsed) {
            Ok(mut json) => {
                if let Some(ref outcome) = result.regression
                    && let serde_json::Value::Object(ref mut map) = json
                {
                    map.insert("regression".to_string(), outcome.to_json());
                }
                if let Some(ref deltas) = result.baseline_deltas
                    && let serde_json::Value::Object(ref mut map) = json
                {
                    map.insert(
                        "baseline_deltas".to_string(),
                        report::build_baseline_deltas_json(
                            deltas.total_delta,
                            deltas
                                .per_category
                                .iter()
                                .map(|(cat, d)| (cat.as_str(), d.current, d.baseline, d.delta)),
                        ),
                    );
                }
                combined.insert("check".into(), json);
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

    if let Some(result) = dupes {
        match serde_json::to_value(&result.report) {
            Ok(json) => {
                combined.insert("dupes".into(), json);
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

    if let Some(result) = health {
        match serde_json::to_value(&result.report) {
            Ok(json) => {
                combined.insert("health".into(), json);
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

    match serde_json::to_string_pretty(&serde_json::Value::Object(combined)) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => emit_error(
            &format!("JSON serialization error: {e}"),
            2,
            OutputFormat::Json,
        ),
    }
}

/// Print combined SARIF with multiple runs (one per analysis).
fn print_combined_sarif(
    check: Option<&CheckResult>,
    dupes: Option<&DupesResult>,
    health: Option<&HealthResult>,
) -> ExitCode {
    let mut all_runs = Vec::new();

    if let Some(result) = check {
        let sarif = report::build_sarif(&result.results, &result.config.root, &result.config.rules);
        if let Some(runs) = sarif.get("runs").and_then(|r| r.as_array()) {
            all_runs.extend(runs.iter().cloned());
        }
    }

    // Duplication SARIF builder is pub(super) — serialize the report as a simple run
    if let Some(result) = dupes.filter(|r| !r.report.clone_groups.is_empty()) {
        let run = serde_json::json!({
            "tool": {
                "driver": {
                    "name": "fallow",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/fallow-rs/fallow",
                }
            },
            "automationDetails": { "id": "fallow/dupes" },
            "results": result.report.clone_groups.iter().enumerate().map(|(i, g)| {
                serde_json::json!({
                    "ruleId": "fallow/code-duplication",
                    "level": "warning",
                    "message": { "text": format!("Clone group {} ({} lines, {} instances)", i + 1, g.line_count, g.instances.len()) },
                })
            }).collect::<Vec<_>>()
        });
        all_runs.push(run);
    }

    if let Some(result) = health {
        let sarif = report::build_health_sarif(&result.report, &result.config.root);
        if let Some(runs) = sarif.get("runs").and_then(|r| r.as_array()) {
            all_runs.extend(runs.iter().cloned());
        }
    }

    let combined = serde_json::json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": all_runs,
    });

    match serde_json::to_string_pretty(&combined) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => emit_error(
            &format!("SARIF serialization error: {e}"),
            2,
            OutputFormat::Sarif,
        ),
    }
}

/// Print combined `CodeClimate` output merging all analyses into one JSON array.
fn print_combined_codeclimate(
    check: Option<&CheckResult>,
    dupes: Option<&DupesResult>,
    health: Option<&HealthResult>,
) -> ExitCode {
    let mut all_issues = Vec::new();

    if let Some(result) = check
        && let serde_json::Value::Array(items) =
            report::build_codeclimate(&result.results, &result.config.root, &result.config.rules)
    {
        all_issues.extend(items);
    }

    if let Some(result) = dupes
        && let serde_json::Value::Array(items) =
            report::build_duplication_codeclimate(&result.report, &result.config.root)
    {
        all_issues.extend(items);
    }

    if let Some(result) = health
        && let serde_json::Value::Array(items) =
            report::build_health_codeclimate(&result.report, &result.config.root)
    {
        all_issues.extend(items);
    }

    match serde_json::to_string_pretty(&serde_json::Value::Array(all_issues)) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => emit_error(
            &format!("CodeClimate serialization error: {e}"),
            2,
            OutputFormat::CodeClimate,
        ),
    }
}

fn build_health_opts<'a>(opts: &'a CombinedOptions<'a>) -> HealthOptions<'a> {
    HealthOptions {
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
        changed_since: opts.changed_since,
        workspace: opts.workspace,
        baseline: None,
        save_baseline: None,
        complexity: true,
        file_scores: true,
        hotspots: true,
        targets: true,
        effort: None,
        score: false,
        min_score: None,
        since: None,
        min_commits: None,
        explain: opts.explain,
        summary: opts.summary,
        save_snapshot: None,
        trend: false,
        group_by: opts.group_by,
    }
}

/// Print orientation header: vital signs summary + start-here nudge.
///
/// Renders a compact one-or-two-line block at the top of combined mode output
/// so users immediately see the project's vital signs and top refactoring target.
fn print_orientation_header(health: &HealthResult, check: Option<&CheckResult>) {
    // Vital signs line (skip when trend table is active — it replaces vital signs)
    if let Some(ref vs) = health.report.vital_signs
        && health.report.health_trend.is_none()
    {
        let mut parts = Vec::new();
        if let Some(dfp) = vs.dead_file_pct {
            if let Some(ref c) = vs.counts {
                parts.push(format!(
                    "dead files {dfp:.1}% ({} of {})",
                    c.dead_files, c.total_files
                ));
            } else {
                parts.push(format!("dead files {dfp:.1}%"));
            }
        }
        if let Some(dep) = vs.dead_export_pct {
            if let Some(ref c) = vs.counts {
                parts.push(format!(
                    "dead exports {dep:.1}% ({} of {})",
                    c.dead_exports, c.total_exports
                ));
            } else {
                parts.push(format!("dead exports {dep:.1}%"));
            }
        }
        if let Some(mi) = vs.maintainability_avg {
            let label = if mi >= 85.0 {
                "good"
            } else if mi >= 65.0 {
                "moderate"
            } else {
                "low"
            };
            parts.push(format!("MI {mi:.1} ({label})"));
        }
        if let Some(hc) = vs.hotspot_count
            && hc > 0
        {
            parts.push(format!(
                "{hc} churn hotspot{}",
                if hc == 1 { "" } else { "s" }
            ));
        }
        if let Some(cd) = vs.circular_dep_count
            && cd > 0
        {
            parts.push(format!(
                "{cd} circular dep{}",
                if cd == 1 { "" } else { "s" }
            ));
        }
        if !parts.is_empty() {
            eprintln!();
            eprintln!(
                "{} {}",
                "\u{25a0}".dimmed(),
                parts.join(" \u{00b7} ").dimmed()
            );
        }
    }

    // Analysis scope: file count + active plugins
    let files = health.report.summary.files_analyzed;
    let config = check.map_or(&health.config, |c| &c.config);
    let plugin_count = config.external_plugins.len();
    if files > 0 {
        use std::fmt::Write as _;
        let mut scope = format!("  {files} files analyzed");
        if plugin_count > 0 {
            let names: Vec<&str> = config
                .external_plugins
                .iter()
                .take(5)
                .map(|p| p.name.as_str())
                .collect();
            let _ = write!(
                scope,
                ", {plugin_count} plugin{}",
                if plugin_count == 1 { "" } else { "s" }
            );
            let _ = write!(scope, " ({})", names.join(", "));
            if plugin_count > 5 {
                let _ = write!(scope, " +{}", plugin_count - 5);
            }
        }
        eprintln!("{}", scope.dimmed());
    }

    // Entry-point detection summary
    if let Some(result) = check {
        print_entry_point_summary(&result.results);
    }

    // "Start here" nudge: point to top refactoring target
    if !health.report.targets.is_empty() {
        let target_count = health.report.targets.len();
        let total_issues = check.map_or(0, |c| c.results.total_issues());

        if total_issues > 500 {
            // Scale-aware: suggest scoping instead of a specific file
            eprintln!(
                "{}",
                format!(
                    "  {target_count} refactoring target{} \u{2014} try `fallow dead-code --workspace <name>` to scope",
                    if target_count == 1 { "" } else { "s" },
                )
                .dimmed()
            );
        } else {
            // Prefer non-test target; skip nudge if all targets are noise
            if let Some(top) = health
                .report
                .targets
                .iter()
                .find(|t| !is_test_path(&t.path))
            {
                let file_name = top
                    .path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                eprintln!(
                    "{}",
                    format!(
                        "  {target_count} refactoring target{} \u{2014} start with {file_name} ({})",
                        if target_count == 1 { "" } else { "s" },
                        top.category.label()
                    )
                    .dimmed()
                );
            } else {
                eprintln!(
                    "{}",
                    format!(
                        "  {target_count} refactoring target{}",
                        if target_count == 1 { "" } else { "s" },
                    )
                    .dimmed()
                );
            }
        }
    }
}

/// Check if a path is a test, fixture, or generated file that shouldn't be
/// recommended as a refactoring starting point.
fn is_test_path(path: &std::path::Path) -> bool {
    // Check directory components for test/fixture/example directories
    if path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        matches!(
            s.as_ref(),
            "test"
                | "tests"
                | "__tests__"
                | "__test__"
                | "spec"
                | "specs"
                | "__mocks__"
                | "__fixtures__"
                | "fixtures"
                | "examples"
                | "example"
                | "__snapshots__"
                | "snapshots"
                | "benchmark"
                | "benchmarks"
                | "bench"
                | "e2e"
                | "playground"
                | "playgrounds"
        )
    }) {
        return true;
    }
    // Check file name patterns
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.contains(".test.")
            || name.contains(".spec.")
            || name.contains(".fixture.")
            || name.contains(".e2e.")
            || name.contains(".bench.")
            || name.contains(".story.")
            || name.contains(".stories.")
        {
            return true;
        }
        // Generated file heuristic: single letter + digits (a0.js, b1.mjs)
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem.len() <= 3
            && stem.starts_with(|c: char| c.is_ascii_lowercase())
            && stem[1..].bytes().all(|b| b.is_ascii_digit())
        {
            return true;
        }
    }
    false
}

/// Print entry-point detection summary to stderr.
///
/// Shows a dimmed informational line so users can verify that fallow found the
/// right entry points. When zero entry points are detected, emits a warning
/// with a remediation command.
pub fn print_entry_point_summary(results: &fallow_core::results::AnalysisResults) {
    let Some(ref summary) = results.entry_point_summary else {
        return;
    };
    if summary.total == 0 {
        eprintln!(
            "{}",
            "  \u{26a0} No entry points detected \u{2014} exports may appear unused. Run: fallow list --entry-points"
                .yellow()
        );
        return;
    }
    use std::fmt::Write as _;
    let mut line = format!(
        "  {} entry point{} detected",
        summary.total,
        if summary.total == 1 { "" } else { "s" }
    );
    if !summary.by_source.is_empty() {
        let parts: Vec<String> = summary
            .by_source
            .iter()
            .map(|(source, count)| format!("{count} {source}"))
            .collect();
        let _ = write!(line, " ({})", parts.join(", "));
    }
    eprintln!("{}", line.dimmed());
}
/// Convert an ExitCode to u8 for comparison.
/// ExitCode doesn't implement Ord, so we use this workaround.
fn exit_code_to_u8(code: ExitCode) -> u8 {
    u8::from(code != ExitCode::SUCCESS)
}
