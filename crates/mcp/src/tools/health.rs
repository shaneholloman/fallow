use crate::params::HealthParams;

use super::{push_baseline, push_global, push_scope};

/// Build CLI arguments for the `check_health` tool.
pub fn build_health_args(params: &HealthParams) -> Vec<String> {
    let mut args = vec![
        "health".to_string(),
        "--format".to_string(),
        "json".to_string(),
        "--quiet".to_string(),
        "--explain".to_string(),
    ];

    push_global(
        &mut args,
        params.root.as_deref(),
        params.config.as_deref(),
        params.no_cache,
        params.threads,
    );
    push_scope(&mut args, params.production, params.workspace.as_deref());

    if let Some(max_cyclomatic) = params.max_cyclomatic {
        args.extend(["--max-cyclomatic".to_string(), max_cyclomatic.to_string()]);
    }
    if let Some(max_cognitive) = params.max_cognitive {
        args.extend(["--max-cognitive".to_string(), max_cognitive.to_string()]);
    }
    if let Some(top) = params.top {
        args.extend(["--top".to_string(), top.to_string()]);
    }
    if let Some(ref sort) = params.sort {
        args.extend(["--sort".to_string(), sort.clone()]);
    }
    if let Some(ref changed_since) = params.changed_since {
        args.extend(["--changed-since".to_string(), changed_since.clone()]);
    }
    if params.complexity == Some(true) {
        args.push("--complexity".to_string());
    }
    if params.file_scores == Some(true) {
        args.push("--file-scores".to_string());
    }
    // --ownership and --ownership-email-mode imply --hotspots on the CLI; we
    // mirror that mapping here so MCP consumers don't need to set hotspots
    // explicitly. Skipping the duplicate `--hotspots` keeps clap happy.
    let ownership_active = params.ownership == Some(true) || params.ownership_email_mode.is_some();
    if params.hotspots == Some(true) || ownership_active {
        args.push("--hotspots".to_string());
    }
    if ownership_active {
        args.push("--ownership".to_string());
    }
    if let Some(mode) = params.ownership_email_mode {
        args.extend(["--ownership-emails".to_string(), mode.as_cli().to_string()]);
    }
    if params.targets == Some(true) {
        args.push("--targets".to_string());
    }
    if params.coverage_gaps == Some(true) {
        args.push("--coverage-gaps".to_string());
    }
    if params.score == Some(true) {
        args.push("--score".to_string());
    }
    if let Some(min_score) = params.min_score {
        args.extend(["--min-score".to_string(), min_score.to_string()]);
    }
    if let Some(ref min_severity) = params.min_severity {
        args.extend(["--min-severity".to_string(), min_severity.clone()]);
    }
    if let Some(ref since) = params.since {
        args.extend(["--since".to_string(), since.clone()]);
    }
    if let Some(min_commits) = params.min_commits {
        args.extend(["--min-commits".to_string(), min_commits.to_string()]);
    }
    if let Some(ref path) = params.save_snapshot {
        if path.is_empty() {
            args.push("--save-snapshot".to_string());
        } else {
            args.extend(["--save-snapshot".to_string(), path.clone()]);
        }
    }
    push_baseline(
        &mut args,
        params.baseline.as_deref(),
        params.save_baseline.as_deref(),
    );
    if params.trend == Some(true) {
        args.push("--trend".to_string());
    }
    if let Some(ref effort) = params.effort {
        args.extend(["--effort".to_string(), effort.clone()]);
    }
    if params.summary == Some(true) {
        args.push("--summary".to_string());
    }
    if let Some(ref coverage) = params.coverage {
        args.extend(["--coverage".to_string(), coverage.clone()]);
    }
    if let Some(ref coverage_root) = params.coverage_root {
        args.extend(["--coverage-root".to_string(), coverage_root.clone()]);
    }
    if let Some(ref production_coverage) = params.production_coverage {
        args.extend([
            "--production-coverage".to_string(),
            production_coverage.clone(),
        ]);
    }
    if let Some(min_invocations_hot) = params.min_invocations_hot {
        args.extend([
            "--min-invocations-hot".to_string(),
            min_invocations_hot.to_string(),
        ]);
    }

    args
}
