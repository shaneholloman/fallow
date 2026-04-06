use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Default, Deserialize, JsonSchema)]
pub struct AnalyzeParams {
    /// Root directory of the project to analyze. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file (.fallowrc.json or fallow.toml).
    pub config: Option<String>,

    /// Only analyze production code (excludes tests, stories, dev files).
    pub production: Option<bool>,

    /// Scope analysis to a specific workspace package name.
    pub workspace: Option<String>,

    /// Issue types to include. When set, only these types are reported.
    /// Valid values: unused-files, unused-exports, unused-types, unused-deps,
    /// unused-enum-members, unused-class-members, unresolved-imports,
    /// unlisted-deps, duplicate-exports, circular-deps, boundary-violations.
    pub issue_types: Option<Vec<String>>,

    #[schemars(
        description = "Set to true to check only boundary violations. Convenience alias for issue_types: [\"boundary-violations\"]"
    )]
    pub boundary_violations: Option<bool>,

    /// Compare results against a saved baseline file. Only new issues (not in the baseline) are reported.
    pub baseline: Option<String>,

    /// Save current results as a baseline file for future comparisons.
    pub save_baseline: Option<String>,

    /// Fail if issue counts regressed compared to the regression baseline.
    pub fail_on_regression: Option<bool>,

    /// Regression tolerance. Accepts a percentage ("2%") or absolute count ("5").
    pub tolerance: Option<String>,

    /// Path to a regression baseline file to compare against.
    pub regression_baseline: Option<String>,

    /// Save current results as a regression baseline file for future comparisons.
    pub save_regression_baseline: Option<String>,

    /// Group results by owner (from CODEOWNERS) or directory. Values: "owner", "directory".
    pub group_by: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CheckChangedParams {
    /// Root directory of the project to analyze. Defaults to current working directory.
    pub root: Option<String>,

    /// Git ref to compare against (e.g., "main", "HEAD~5", a commit SHA).
    /// Only files changed since this ref are reported.
    pub since: String,

    /// Path to fallow config file.
    pub config: Option<String>,

    /// Only analyze production code.
    pub production: Option<bool>,

    /// Scope analysis to a specific workspace package name.
    pub workspace: Option<String>,

    /// Compare results against a saved baseline file. Only new issues (not in the baseline) are reported.
    pub baseline: Option<String>,

    /// Save current results as a baseline file for future comparisons.
    pub save_baseline: Option<String>,

    /// Fail if issue counts regressed compared to the regression baseline.
    pub fail_on_regression: Option<bool>,

    /// Regression tolerance. Accepts a percentage ("2%") or absolute count ("5").
    pub tolerance: Option<String>,

    /// Path to a regression baseline file to compare against.
    pub regression_baseline: Option<String>,

    /// Save current results as a regression baseline file for future comparisons.
    pub save_regression_baseline: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct FindDupesParams {
    /// Root directory of the project to analyze. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file (.fallowrc.json or fallow.toml).
    pub config: Option<String>,

    /// Scope analysis to a specific workspace package name.
    pub workspace: Option<String>,

    /// Detection mode: "strict" (exact tokens), "mild" (normalized identifiers),
    /// "weak" (structural only), or "semantic" (type-aware). Defaults to "mild".
    pub mode: Option<String>,

    /// Minimum token count for a clone to be reported. Default: 50.
    pub min_tokens: Option<u32>,

    /// Minimum line count for a clone to be reported. Default: 5.
    pub min_lines: Option<u32>,

    /// Fail if duplication percentage exceeds this value. 0 = no limit.
    pub threshold: Option<f64>,

    /// Skip file-local duplicates, only report cross-file clones.
    pub skip_local: Option<bool>,

    /// Enable cross-language detection (strip TS type annotations for TS<->JS matching).
    pub cross_language: Option<bool>,

    /// Show only the N largest clone groups.
    pub top: Option<usize>,

    /// Compare results against a saved baseline file. Only new issues (not in the baseline) are reported.
    pub baseline: Option<String>,

    /// Save current results as a baseline file for future comparisons.
    pub save_baseline: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,

    #[schemars(
        description = "Only report issues in files changed since this git ref (branch, tag, or commit SHA)"
    )]
    pub changed_since: Option<String>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct FixParams {
    /// Root directory of the project. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file.
    pub config: Option<String>,

    /// Only analyze production code (excludes tests, stories, dev files).
    pub production: Option<bool>,

    /// Scope analysis to a specific workspace package name.
    pub workspace: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct ProjectInfoParams {
    /// Root directory of the project. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file.
    pub config: Option<String>,

    #[schemars(description = "Show detected entry points")]
    pub entry_points: Option<bool>,
    #[schemars(description = "Show all discovered source files")]
    pub files: Option<bool>,
    #[schemars(description = "Show active framework plugins")]
    pub plugins: Option<bool>,
    #[schemars(description = "Show architecture boundary zones, rules, and per-zone file counts")]
    pub boundaries: Option<bool>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct HealthParams {
    /// Root directory of the project to analyze. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file (.fallowrc.json or fallow.toml).
    pub config: Option<String>,

    /// Maximum cyclomatic complexity threshold. Functions exceeding this are reported.
    pub max_cyclomatic: Option<u16>,

    /// Maximum cognitive complexity threshold. Functions exceeding this are reported.
    pub max_cognitive: Option<u16>,

    /// Number of top results to return, sorted by complexity.
    pub top: Option<usize>,

    /// Sort order for results (e.g., "cyclomatic", "cognitive").
    pub sort: Option<String>,

    /// Git ref to compare against. Only files changed since this ref are analyzed.
    pub changed_since: Option<String>,

    /// Show only complexity findings. By default all sections are shown; use this to select only complexity.
    pub complexity: Option<bool>,

    /// Show only per-file health scores (fan-in, fan-out, dead code ratio, maintainability index).
    pub file_scores: Option<bool>,

    /// Show only hotspots: files that are both complex and frequently changing.
    pub hotspots: Option<bool>,

    /// Show only refactoring targets: ranked recommendations based on complexity, coupling, churn, and dead code.
    pub targets: Option<bool>,

    /// Show static test coverage gaps: runtime files and exports with no test dependency path.
    pub coverage_gaps: Option<bool>,

    /// Show only the project health score (0–100) with letter grade (A/B/C/D/F).
    /// Forces full pipeline for maximum accuracy.
    pub score: Option<bool>,

    /// Fail if the health score is below this threshold (0–100). Implies --score.
    pub min_score: Option<f64>,

    /// Git history window for hotspot analysis. Accepts durations (6m, 90d, 1y) or ISO dates.
    pub since: Option<String>,

    /// Minimum commits for a file to appear in hotspot ranking.
    pub min_commits: Option<u32>,

    /// Scope output to a single workspace package.
    pub workspace: Option<String>,

    /// Only analyze production code (excludes tests, stories, dev files).
    pub production: Option<bool>,

    /// Save a vital signs snapshot. Provide a file path, or omit value for default (`.fallow/snapshots/{timestamp}.json`).
    pub save_snapshot: Option<String>,

    /// Compare results against a saved baseline file. Only new issues (not in the baseline) are reported.
    pub baseline: Option<String>,

    /// Save current results as a baseline file for future comparisons.
    pub save_baseline: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,

    /// Compare current metrics against the most recent saved snapshot and show per-metric deltas.
    /// Implies --score. Reads from `.fallow/snapshots/`.
    pub trend: Option<bool>,

    /// Analysis effort level. Controls the depth of analysis: "low" (fast, surface-level),
    /// "medium" (balanced, default), "high" (thorough, includes all heuristics).
    pub effort: Option<String>,

    /// Include a natural-language summary of findings alongside the structured JSON output.
    pub summary: Option<bool>,
}

#[derive(Default, Deserialize, JsonSchema)]
pub struct AuditParams {
    /// Root directory of the project to analyze. Defaults to current working directory.
    pub root: Option<String>,

    /// Path to fallow config file (.fallowrc.json or fallow.toml).
    pub config: Option<String>,

    /// Git ref to compare against (e.g., "main", "HEAD~5").
    /// Auto-detects the default branch if not specified.
    pub base: Option<String>,

    /// Only analyze production code (excludes tests, stories, dev files).
    pub production: Option<bool>,

    /// Scope analysis to a specific workspace package name.
    pub workspace: Option<String>,

    /// Disable the incremental parse cache. Forces a full re-parse of all files.
    pub no_cache: Option<bool>,

    /// Number of parser threads. Defaults to available CPU cores.
    pub threads: Option<usize>,
}

/// Parameters for `list_boundaries`.
#[derive(Debug, Default, serde::Deserialize, schemars::JsonSchema)]
pub struct ListBoundariesParams {
    #[schemars(description = "Project root directory (defaults to current working directory)")]
    pub root: Option<String>,
    #[schemars(description = "Path to a fallow config file")]
    pub config: Option<String>,
    #[schemars(description = "Disable the incremental parse cache")]
    pub no_cache: Option<bool>,
    #[schemars(description = "Number of threads for file parsing (defaults to CPU core count)")]
    pub threads: Option<usize>,
}
