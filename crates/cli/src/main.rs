#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "CLI binary produces intentional terminal output"
)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fallow_config::FallowConfig;

mod audit;
mod baseline;
mod check;
mod codeowners;
mod combined;
mod dupes;
mod error;
mod explain;
mod fix;
mod health;
mod health_types;
mod init;
mod list;
mod migrate;
mod regression;
mod report;
mod schema;
mod validate;
mod vital_signs;
mod watch;

use check::{CheckOptions, IssueFilters, TraceOptions};
use dupes::{DupesMode, DupesOptions};
use error::emit_error;
use health::{HealthOptions, SortBy};
use list::ListOptions;

// ── CLI definition ───────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "fallow",
    about = "Codebase analyzer for TypeScript/JavaScript — unused code, circular dependencies, code duplication, complexity hotspots, and architecture boundary violations",
    version,
    after_help = "When no command is given, runs dead-code + dupes + health together.\nUse --only/--skip to select specific analyses."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Project root directory
    #[arg(short, long, global = true)]
    root: Option<PathBuf>,

    /// Path to config file (.fallowrc.json or fallow.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Output format (alias: --output)
    #[arg(
        short,
        long,
        visible_alias = "output",
        global = true,
        default_value = "human"
    )]
    format: Format,

    /// Suppress progress output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Disable incremental caching
    #[arg(long, global = true)]
    no_cache: bool,

    /// Number of parser threads
    #[arg(long, global = true)]
    threads: Option<usize>,

    /// Only report issues in files changed since this git ref (e.g., main, HEAD~5)
    #[arg(long, visible_alias = "base", global = true)]
    changed_since: Option<String>,

    /// Compare against a previously saved baseline file
    #[arg(long, global = true)]
    baseline: Option<PathBuf>,

    /// Save the current results as a baseline file
    #[arg(long, global = true)]
    save_baseline: Option<PathBuf>,

    /// Production mode: exclude test/story/dev files, only start/build scripts,
    /// report type-only dependencies
    #[arg(long, global = true)]
    production: bool,

    /// Scope output to a single workspace package (by package name).
    /// The full cross-workspace graph is still built, but only issues within
    /// the specified package are reported.
    #[arg(short, long, global = true)]
    workspace: Option<String>,

    /// Group output by owner (.github/CODEOWNERS) or by directory (no CODEOWNERS needed).
    /// Partitions all issues into labeled sections for team-level triage and dashboards.
    #[arg(long, global = true)]
    group_by: Option<GroupBy>,

    /// Show pipeline performance timing breakdown
    #[arg(long, global = true)]
    performance: bool,

    /// Include metric definitions and rule descriptions in output.
    /// JSON: adds a `_meta` object with docs URLs, metric ranges, and interpretations.
    /// Always enabled for MCP server responses.
    #[arg(long, global = true)]
    explain: bool,

    /// Show only category counts without individual items
    #[arg(long, global = true)]
    summary: bool,

    /// CI mode: equivalent to --format sarif --fail-on-issues --quiet
    #[arg(long, global = true)]
    ci: bool,

    /// Exit with code 1 if issues are found
    #[arg(long, global = true)]
    fail_on_issues: bool,

    /// Write SARIF output to a file (in addition to the primary --format output)
    #[arg(long, global = true, value_name = "PATH")]
    sarif_file: Option<PathBuf>,

    /// Fail if issue count increased beyond tolerance compared to a regression baseline.
    /// Use --save-regression-baseline to create a baseline first, then
    /// --fail-on-regression on subsequent runs to detect regressions.
    #[arg(long, global = true)]
    fail_on_regression: bool,

    /// Allowed issue count increase before a regression is flagged.
    /// Use "N%" for percentage (e.g., "2%") or "N" for absolute count (e.g., "5").
    /// Default: "0" (any increase fails). Only used with --fail-on-regression.
    #[arg(long, global = true, value_name = "TOLERANCE", default_value = "0")]
    tolerance: String,

    /// Path to the regression baseline file for --fail-on-regression.
    /// Default: .fallow/regression-baseline.json
    #[arg(long, global = true, value_name = "PATH")]
    regression_baseline: Option<PathBuf>,

    /// Save the current issue counts as a regression baseline.
    /// Without a path: writes into the config file (.fallowrc.json / fallow.toml).
    /// With a path: writes a standalone JSON file.
    #[expect(
        clippy::option_option,
        reason = "clap pattern: None=not passed, Some(None)=flag only (write to config), Some(Some(path))=write to file"
    )]
    #[arg(long, global = true, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    save_regression_baseline: Option<Option<String>>,

    /// Run only specific analyses when no subcommand is given (comma-separated: dead-code,dupes,health)
    #[arg(long, value_delimiter = ',')]
    only: Vec<AnalysisKind>,

    /// Skip specific analyses when no subcommand is given (comma-separated: dead-code,dupes,health)
    #[arg(long, value_delimiter = ',')]
    skip: Vec<AnalysisKind>,

    /// Compute health score (0-100 with letter grade) in combined mode.
    /// Use with `--trend` to show score deltas in PR comments.
    #[arg(long)]
    score: bool,

    /// Compare current health metrics against the most recent saved snapshot
    /// and show per-metric deltas. Implies --score.
    #[arg(long)]
    trend: bool,

    /// Save a vital signs snapshot for trend tracking in combined mode.
    /// Provide a path or omit for the default `.fallow/snapshots/` location.
    #[expect(
        clippy::option_option,
        reason = "clap pattern: None=not passed, Some(None)=default path, Some(Some(path))=custom path"
    )]
    #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
    save_snapshot: Option<Option<String>>,
}

#[derive(Subcommand)]
enum Command {
    /// Analyze project for unused code and circular dependencies
    #[command(name = "dead-code", alias = "check")]
    Check {
        /// Only report unused files
        #[arg(long)]
        unused_files: bool,

        /// Only report unused exports
        #[arg(long)]
        unused_exports: bool,

        /// Only report unused dependencies
        #[arg(long)]
        unused_deps: bool,

        /// Only report unused type exports
        #[arg(long)]
        unused_types: bool,

        /// Only report unused enum members
        #[arg(long)]
        unused_enum_members: bool,

        /// Only report unused class members
        #[arg(long)]
        unused_class_members: bool,

        /// Only report unresolved imports
        #[arg(long)]
        unresolved_imports: bool,

        /// Only report unlisted dependencies
        #[arg(long)]
        unlisted_deps: bool,

        /// Only report duplicate exports
        #[arg(long)]
        duplicate_exports: bool,

        /// Only report circular dependencies
        #[arg(long)]
        circular_deps: bool,

        /// Only report boundary violations
        #[arg(long)]
        boundary_violations: bool,

        /// Also run duplication analysis and cross-reference with dead code
        #[arg(long)]
        include_dupes: bool,

        /// Trace why an export is used/unused (format: `FILE:EXPORT_NAME`)
        #[arg(long, value_name = "FILE:EXPORT")]
        trace: Option<String>,

        /// Trace all edges for a file (imports, exports, importers)
        #[arg(long, value_name = "PATH")]
        trace_file: Option<String>,

        /// Trace where a dependency is used
        #[arg(long, value_name = "PACKAGE")]
        trace_dependency: Option<String>,

        /// Show only the top N items per category
        #[arg(long)]
        top: Option<usize>,
    },

    /// Watch for changes and re-run analysis
    Watch {
        /// Don't clear the screen between re-analyses
        #[arg(long)]
        no_clear: bool,
    },

    /// Auto-fix issues (remove unused exports, dependencies, enum members)
    Fix {
        /// Dry run — show what would be changed without modifying files
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt (required in non-TTY environments like CI or AI agents)
        #[arg(long, alias = "force")]
        yes: bool,
    },

    /// Initialize a .fallowrc.json configuration file
    Init {
        /// Generate TOML instead of JSONC
        #[arg(long)]
        toml: bool,

        /// Scaffold a pre-commit git hook that runs fallow on changed files
        #[arg(long)]
        hooks: bool,

        /// Base branch/ref for the pre-commit hook (default: auto-detect or "main")
        #[arg(long, requires = "hooks")]
        branch: Option<String>,
    },

    /// Print the JSON Schema for fallow configuration files
    ConfigSchema,

    /// Print the JSON Schema for external plugin files
    PluginSchema,

    /// List discovered entry points and files
    List {
        /// Show entry points
        #[arg(long)]
        entry_points: bool,

        /// Show all discovered files
        #[arg(long)]
        files: bool,

        /// Show active plugins
        #[arg(long)]
        plugins: bool,

        /// Show architecture boundary zones, rules, and per-zone file counts
        #[arg(long)]
        boundaries: bool,
    },

    /// Find code duplication / clones across the project
    Dupes {
        /// Detection mode: strict, mild, weak, or semantic
        #[arg(long, default_value = "mild")]
        mode: DupesMode,

        /// Minimum token count for a clone
        #[arg(long, default_value = "50")]
        min_tokens: usize,

        /// Minimum line count for a clone
        #[arg(long, default_value = "5")]
        min_lines: usize,

        /// Fail if duplication exceeds this percentage (0 = no limit)
        #[arg(long, default_value = "0")]
        threshold: f64,

        /// Only report cross-directory duplicates
        #[arg(long)]
        skip_local: bool,

        /// Enable cross-language detection (strip TS type annotations for TS↔JS matching)
        #[arg(long)]
        cross_language: bool,

        /// Show only the N largest clone groups
        #[arg(long)]
        top: Option<usize>,

        /// Trace all clones at a specific location (format: `FILE:LINE`)
        #[arg(long, value_name = "FILE:LINE")]
        trace: Option<String>,
    },

    /// Analyze function complexity (cyclomatic + cognitive)
    ///
    /// By default, shows all existing sections: health score, complexity findings,
    /// file scores, hotspots, and refactoring targets. When any section flag is
    /// specified, only those sections are shown.
    Health {
        /// Maximum cyclomatic complexity threshold (overrides config)
        #[arg(long)]
        max_cyclomatic: Option<u16>,

        /// Maximum cognitive complexity threshold (overrides config)
        #[arg(long)]
        max_cognitive: Option<u16>,

        /// Show only the N most complex functions
        #[arg(long)]
        top: Option<usize>,

        /// Sort by: cyclomatic, cognitive, or lines
        #[arg(long, default_value = "cyclomatic")]
        sort: SortBy,

        /// Show only complexity findings (functions exceeding thresholds).
        /// By default all sections are shown; use this to select only complexity.
        #[arg(long)]
        complexity: bool,

        /// Show only per-file health scores (fan-in, fan-out, dead code ratio, maintainability index).
        /// Requires full analysis pipeline (graph + dead code detection).
        /// Sorted by maintainability index ascending (worst first). --sort and --baseline
        /// apply to complexity findings only, not file scores.
        #[arg(long)]
        file_scores: bool,

        /// Show only static test coverage gaps: runtime files and exports with no
        /// dependency path from any discovered test root. Requires full analysis pipeline.
        #[arg(long)]
        coverage_gaps: bool,

        /// Show only hotspots: files that are both complex and frequently changing.
        /// Combines git churn history with complexity data. Requires a git repository.
        #[arg(long)]
        hotspots: bool,

        /// Show only refactoring targets: ranked recommendations based on complexity,
        /// coupling, churn, and dead code signals. Requires full analysis pipeline.
        #[arg(long)]
        targets: bool,

        /// Filter refactoring targets by effort level (low, medium, high).
        /// Implies --targets.
        #[arg(long, value_enum)]
        effort: Option<EffortFilter>,

        /// Show only the project health score (0–100) with letter grade (A/B/C/D/F).
        /// The score is included by default when no section flags are set.
        #[arg(long)]
        score: bool,

        /// Fail if the health score is below this threshold (0–100).
        /// Implies --score. Useful as a CI quality gate.
        #[arg(long, value_name = "N")]
        min_score: Option<f64>,

        /// Git history window for hotspot analysis (default: 6m).
        /// Accepts durations (6m, 90d, 1y, 2w) or ISO dates (2025-06-01).
        #[arg(long, value_name = "DURATION")]
        since: Option<String>,

        /// Minimum number of commits for a file to be included in hotspot ranking (default: 3)
        #[arg(long, value_name = "N")]
        min_commits: Option<u32>,

        /// Save a vital signs snapshot for trend tracking.
        /// Defaults to `.fallow/snapshots/{timestamp}.json` if no path is given.
        /// Forces file-scores, hotspot, and score computation for complete metrics.
        #[expect(
            clippy::option_option,
            reason = "clap pattern: None=not passed, Some(None)=flag only, Some(Some(path))=with value"
        )]
        #[arg(long, value_name = "PATH", num_args = 0..=1, default_missing_value = "")]
        save_snapshot: Option<Option<String>>,

        /// Compare current metrics against the most recent saved snapshot.
        /// Reads from `.fallow/snapshots/` and shows per-metric deltas with
        /// directional indicators. Implies --score.
        #[arg(long)]
        trend: bool,
    },

    /// Audit changed files for dead code, complexity, and duplication.
    ///
    /// Purpose-built for reviewing AI-generated code and PR quality gates.
    /// Combines dead-code + complexity + duplication scoped to changed files
    /// and returns a verdict (pass/warn/fail).
    /// Auto-detects the base branch if --changed-since/--base is not set.
    Audit,

    /// Dump the CLI interface as machine-readable JSON for agent introspection
    Schema,

    /// Migrate configuration from knip or jscpd to fallow
    Migrate {
        /// Generate TOML instead of JSONC
        #[arg(long)]
        toml: bool,

        /// Only preview the generated config without writing
        #[arg(long)]
        dry_run: bool,

        /// Path to source config file (auto-detect if not specified)
        #[arg(long, value_name = "PATH")]
        from: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    Human,
    Json,
    Sarif,
    Compact,
    Markdown,
    #[value(name = "codeclimate")]
    CodeClimate,
    Badge,
}

impl From<Format> for fallow_config::OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Human => Self::Human,
            Format::Json => Self::Json,
            Format::Sarif => Self::Sarif,
            Format::Compact => Self::Compact,
            Format::Markdown => Self::Markdown,
            Format::CodeClimate => Self::CodeClimate,
            Format::Badge => Self::Badge,
        }
    }
}

/// Analysis types for --only/--skip selection.
#[derive(Clone, PartialEq, Eq, clap::ValueEnum)]
pub enum AnalysisKind {
    #[value(alias = "check")]
    DeadCode,
    Dupes,
    Health,
}

/// Grouping mode for `--group-by`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum GroupBy {
    /// Group by CODEOWNERS file ownership (first owner, last matching rule).
    #[value(alias = "team", alias = "codeowner")]
    Owner,
    /// Group by first directory component of the file path.
    Directory,
    /// Group by workspace package (monorepo).
    #[value(alias = "workspace", alias = "pkg")]
    Package,
}

/// Filter refactoring targets by effort level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum EffortFilter {
    Low,
    Medium,
    High,
}

impl EffortFilter {
    /// Convert to the corresponding `EffortEstimate` for comparison.
    const fn to_estimate(self) -> health_types::EffortEstimate {
        match self {
            Self::Low => health_types::EffortEstimate::Low,
            Self::Medium => health_types::EffortEstimate::Medium,
            Self::High => health_types::EffortEstimate::High,
        }
    }
}

// See `error.rs` — `emit_error` is re-exported via `use error::emit_error`.

// ── Environment variable helpers ─────────────────────────────────

/// Read `FALLOW_FORMAT` env var and parse it into a Format value.
fn format_from_env() -> Option<Format> {
    let val = std::env::var("FALLOW_FORMAT").ok()?;
    match val.to_lowercase().as_str() {
        "json" => Some(Format::Json),
        "human" => Some(Format::Human),
        "sarif" => Some(Format::Sarif),
        "compact" => Some(Format::Compact),
        "markdown" | "md" => Some(Format::Markdown),
        "codeclimate" => Some(Format::CodeClimate),
        "badge" => Some(Format::Badge),
        _ => None,
    }
}

/// Read `FALLOW_QUIET` env var: "1" or "true" (case-insensitive) means quiet.
fn quiet_from_env() -> bool {
    std::env::var("FALLOW_QUIET")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

// ── Group-by resolver ────────────────────────────────────────────

/// Build an `OwnershipResolver` from CLI `--group-by` and config settings.
///
/// Returns `None` when no grouping is requested. Returns `Err(ExitCode)` when
/// `--group-by owner` is requested but no CODEOWNERS file can be found.
fn build_ownership_resolver(
    group_by: Option<GroupBy>,
    root: &std::path::Path,
    codeowners_path: Option<&str>,
    output: fallow_config::OutputFormat,
) -> Result<Option<report::OwnershipResolver>, ExitCode> {
    let Some(mode) = group_by else {
        return Ok(None);
    };
    match mode {
        GroupBy::Owner => match codeowners::CodeOwners::load(root, codeowners_path) {
            Ok(co) => Ok(Some(report::OwnershipResolver::Owner(co))),
            Err(e) => Err(emit_error(&e, 2, output)),
        },
        GroupBy::Directory => Ok(Some(report::OwnershipResolver::Directory)),
        GroupBy::Package => {
            let workspaces = fallow_config::discover_workspaces(root);
            if workspaces.is_empty() {
                Err(emit_error(
                    "--group-by package requires a monorepo with workspace packages \
                     (package.json workspaces, pnpm-workspace.yaml, or tsconfig references)",
                    2,
                    output,
                ))
            } else {
                Ok(Some(report::OwnershipResolver::Package(
                    report::grouping::PackageResolver::new(root, &workspaces),
                )))
            }
        }
    }
}

// ── Config loading ───────────────────────────────────────────────

#[expect(clippy::ref_option, reason = "&Option matches clap's field type")]
fn load_config(
    root: &std::path::Path,
    config_path: &Option<PathBuf>,
    output: fallow_config::OutputFormat,
    no_cache: bool,
    threads: usize,
    production: bool,
    quiet: bool,
) -> Result<fallow_config::ResolvedConfig, ExitCode> {
    let user_config = if let Some(path) = config_path {
        // Explicit --config: propagate errors
        match FallowConfig::load(path) {
            Ok(c) => Some(c),
            Err(e) => {
                let msg = format!("failed to load config '{}': {e}", path.display());
                return Err(emit_error(&msg, 2, output));
            }
        }
    } else {
        match FallowConfig::find_and_load(root) {
            Ok(found) => found.map(|(c, _)| c),
            Err(e) => {
                return Err(emit_error(&e, 2, output));
            }
        }
    };

    Ok(match user_config {
        Some(mut config) => {
            // CLI --production flag overrides config
            if production {
                config.production = true;
            }
            config.resolve(root.to_path_buf(), output, threads, no_cache, quiet)
        }
        None => FallowConfig {
            production,
            ..FallowConfig::default()
        }
        .resolve(root.to_path_buf(), output, threads, no_cache, quiet),
    })
}

// ── Format resolution ─────────────────────────────────────────────

struct FormatConfig {
    output: fallow_config::OutputFormat,
    quiet: bool,
    cli_format_was_explicit: bool,
}

fn resolve_format(cli: &Cli) -> FormatConfig {
    // Resolve output format: CLI flag > FALLOW_FORMAT env var > default ("human").
    // clap sets the default to "human", so we only override with the env var
    // when the user did NOT explicitly pass --format on the CLI.
    let cli_format_was_explicit = std::env::args()
        .any(|a| a == "--format" || a == "--output" || a.starts_with("--format=") || a == "-f");
    let format: Format = if cli_format_was_explicit {
        cli.format
    } else {
        format_from_env().unwrap_or(cli.format)
    };

    // Resolve quiet: CLI --quiet flag > FALLOW_QUIET env var > false
    let quiet = cli.quiet || quiet_from_env();

    FormatConfig {
        output: format.into(),
        quiet,
        cli_format_was_explicit,
    }
}

// ── Tracing setup ─────────────────────────────────────────────────

/// Set up tracing — use WARN level when progress spinners will be active (TTY + not quiet)
/// to prevent tracing INFO lines from corrupting spinner output on stderr.
/// In non-TTY (piped/CI), keep INFO level since there are no spinners to conflict with.
/// Watch mode always uses WARN since spinners replace the per-run INFO noise.
fn setup_tracing(quiet: bool, is_watch: bool) {
    if !quiet {
        let stderr_is_tty = std::io::IsTerminal::is_terminal(&std::io::stderr());
        let default_level = if is_watch || stderr_is_tty {
            tracing::Level::WARN
        } else {
            tracing::Level::INFO
        };
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive(default_level.into()),
            )
            .with_target(false)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .init();
    }
}

// ── Input validation ──────────────────────────────────────────────

fn validate_inputs(
    cli: &Cli,
    output: fallow_config::OutputFormat,
) -> Result<(PathBuf, usize), ExitCode> {
    // Validate control characters in key string inputs
    if let Some(ref config_path) = cli.config
        && let Some(s) = config_path.to_str()
        && let Err(e) = validate::validate_no_control_chars(s, "--config")
    {
        return Err(emit_error(&e, 2, output));
    }
    if let Some(ref ws) = cli.workspace
        && let Err(e) = validate::validate_no_control_chars(ws, "--workspace")
    {
        return Err(emit_error(&e, 2, output));
    }
    if let Some(ref git_ref) = cli.changed_since
        && let Err(e) = validate::validate_no_control_chars(git_ref, "--changed-since")
    {
        return Err(emit_error(&e, 2, output));
    }

    // Validate and resolve root
    let raw_root = cli
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to get current directory"));
    let root = match validate::validate_root(&raw_root) {
        Ok(r) => r,
        Err(e) => {
            return Err(emit_error(&e, 2, output));
        }
    };

    // Validate --changed-since early
    if let Some(ref git_ref) = cli.changed_since
        && let Err(e) = validate::validate_git_ref(git_ref)
    {
        return Err(emit_error(
            &format!("invalid --changed-since: {e}"),
            2,
            output,
        ));
    }

    let threads = cli.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(4)
    });

    // Configure rayon global thread pool to match --threads, ensuring parsing
    // and import resolution use the same thread count as file walking.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build_global();

    Ok((root, threads))
}

/// Apply CI defaults: if `--ci` is set, override format to SARIF (unless explicit),
/// enable fail-on-issues, and set quiet. Returns (output, quiet, `fail_on_issues`).
fn apply_ci_defaults(
    ci: bool,
    mut fail_on_issues: bool,
    output: fallow_config::OutputFormat,
    quiet: bool,
    cli_format_was_explicit: bool,
) -> (fallow_config::OutputFormat, bool, bool) {
    if ci {
        let ci_output = if !cli_format_was_explicit && format_from_env().is_none() {
            fallow_config::OutputFormat::Sarif
        } else {
            output
        };
        fail_on_issues = true;
        (ci_output, true, fail_on_issues)
    } else {
        (output, quiet, fail_on_issues)
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn build_regression_opts<'a>(
    fail_on_regression: bool,
    tolerance: regression::Tolerance,
    regression_baseline: Option<&'a std::path::Path>,
    save_regression_file: Option<&'a std::path::PathBuf>,
    save_to_config: bool,
    scoped: bool,
    quiet: bool,
) -> regression::RegressionOpts<'a> {
    regression::RegressionOpts {
        fail_on_regression,
        tolerance,
        regression_baseline_file: regression_baseline,
        save_target: if let Some(path) = save_regression_file {
            regression::SaveRegressionTarget::File(path)
        } else if save_to_config {
            regression::SaveRegressionTarget::Config
        } else {
            regression::SaveRegressionTarget::None
        },
        scoped,
        quiet,
    }
}

// ── Main ─────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    // Handle schema commands before tracing setup (no side effects)
    if matches!(cli.command, Some(Command::Schema)) {
        return schema::run_schema();
    }
    if matches!(cli.command, Some(Command::ConfigSchema)) {
        return init::run_config_schema();
    }
    if matches!(cli.command, Some(Command::PluginSchema)) {
        return init::run_plugin_schema();
    }

    let fmt = resolve_format(&cli);
    setup_tracing(
        fmt.quiet,
        matches!(cli.command, Some(Command::Watch { .. })),
    );

    let (root, threads) = match validate_inputs(&cli, fmt.output) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let FormatConfig {
        output,
        quiet,
        cli_format_was_explicit,
    } = fmt;

    // Validate --ci/--fail-on-issues/--sarif-file are not used with irrelevant commands
    if (cli.ci || cli.fail_on_issues || cli.sarif_file.is_some())
        && matches!(
            cli.command,
            Some(
                Command::Init { .. }
                    | Command::ConfigSchema
                    | Command::PluginSchema
                    | Command::Schema
                    | Command::List { .. }
                    | Command::Migrate { .. }
            )
        )
    {
        return emit_error(
            "--ci, --fail-on-issues, and --sarif-file are only valid with dead-code, dupes, health, or bare invocation",
            2,
            output,
        );
    }

    // Validate --only/--skip are not used with a subcommand
    if (!cli.only.is_empty() || !cli.skip.is_empty()) && cli.command.is_some() {
        return emit_error(
            "--only and --skip can only be used without a subcommand",
            2,
            output,
        );
    }
    if !cli.only.is_empty() && !cli.skip.is_empty() {
        return emit_error("--only and --skip are mutually exclusive", 2, output);
    }

    // Parse regression tolerance
    let tolerance = match regression::Tolerance::parse(&cli.tolerance) {
        Ok(t) => t,
        Err(e) => return emit_error(&format!("invalid --tolerance: {e}"), 2, output),
    };

    // Resolve save-regression-baseline target
    let save_regression_file: Option<std::path::PathBuf> =
        cli.save_regression_baseline.as_ref().and_then(|opt| {
            opt.as_ref()
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from)
        });
    let save_to_config = cli.save_regression_baseline.is_some() && save_regression_file.is_none();

    let command = cli.command.take();
    match command {
        None => dispatch_bare_command(
            &cli,
            &root,
            output,
            quiet,
            cli_format_was_explicit,
            threads,
            tolerance,
            save_regression_file.as_ref(),
            save_to_config,
        ),
        Some(cmd) => dispatch_subcommand(
            cmd,
            &cli,
            &root,
            output,
            quiet,
            cli_format_was_explicit,
            threads,
            tolerance,
            save_regression_file.as_ref(),
            save_to_config,
        ),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch forwards many flags"
)]
fn dispatch_bare_command(
    cli: &Cli,
    root: &std::path::Path,
    output: fallow_config::OutputFormat,
    quiet: bool,
    cli_format_was_explicit: bool,
    threads: usize,
    tolerance: regression::Tolerance,
    save_regression_file: Option<&std::path::PathBuf>,
    save_to_config: bool,
) -> ExitCode {
    let (output, quiet, fail_on_issues) = apply_ci_defaults(
        cli.ci,
        cli.fail_on_issues,
        output,
        quiet,
        cli_format_was_explicit,
    );
    let (run_check, run_dupes, run_health) = combined::resolve_analyses(&cli.only, &cli.skip);
    combined::run_combined(&combined::CombinedOptions {
        root,
        config_path: &cli.config,
        output,
        no_cache: cli.no_cache,
        threads,
        quiet,
        fail_on_issues,
        sarif_file: cli.sarif_file.as_deref(),
        changed_since: cli.changed_since.as_deref(),
        baseline: cli.baseline.as_deref(),
        save_baseline: cli.save_baseline.as_deref(),
        production: cli.production,
        workspace: cli.workspace.as_deref(),
        group_by: cli.group_by,
        explain: cli.explain,
        performance: cli.performance,
        summary: cli.summary,
        run_check,
        run_dupes,
        run_health,
        score: cli.score || cli.trend,
        trend: cli.trend,
        save_snapshot: cli.save_snapshot.as_ref(),
        regression_opts: build_regression_opts(
            cli.fail_on_regression,
            tolerance,
            cli.regression_baseline.as_deref(),
            save_regression_file,
            save_to_config,
            cli.changed_since.is_some() || cli.workspace.is_some(),
            quiet,
        ),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch forwards many flags"
)]
fn dispatch_subcommand(
    command: Command,
    cli: &Cli,
    root: &std::path::Path,
    output: fallow_config::OutputFormat,
    quiet: bool,
    cli_format_was_explicit: bool,
    threads: usize,
    tolerance: regression::Tolerance,
    save_regression_file: Option<&std::path::PathBuf>,
    save_to_config: bool,
) -> ExitCode {
    match command {
        Command::Check {
            unused_files,
            unused_exports,
            unused_deps,
            unused_types,
            unused_enum_members,
            unused_class_members,
            unresolved_imports,
            unlisted_deps,
            duplicate_exports,
            circular_deps,
            boundary_violations,
            include_dupes,
            trace,
            trace_file,
            trace_dependency,
            top,
        } => {
            let (output, quiet, fail_on_issues) = apply_ci_defaults(
                cli.ci,
                cli.fail_on_issues,
                output,
                quiet,
                cli_format_was_explicit,
            );
            let filters = IssueFilters {
                unused_files,
                unused_exports,
                unused_deps,
                unused_types,
                unused_enum_members,
                unused_class_members,
                unresolved_imports,
                unlisted_deps,
                duplicate_exports,
                circular_deps,
                boundary_violations,
            };
            let trace_opts = TraceOptions {
                trace_export: trace,
                trace_file,
                trace_dependency,
                performance: cli.performance,
            };
            check::run_check(&CheckOptions {
                root,
                config_path: &cli.config,
                output,
                no_cache: cli.no_cache,
                threads,
                quiet,
                fail_on_issues,
                filters: &filters,
                changed_since: cli.changed_since.as_deref(),
                baseline: cli.baseline.as_deref(),
                save_baseline: cli.save_baseline.as_deref(),
                sarif_file: cli.sarif_file.as_deref(),
                production: cli.production,
                workspace: cli.workspace.as_deref(),
                group_by: cli.group_by,
                include_dupes,
                trace_opts: &trace_opts,
                explain: cli.explain,
                top,
                summary: cli.summary,
                regression_opts: build_regression_opts(
                    cli.fail_on_regression,
                    tolerance,
                    cli.regression_baseline.as_deref(),
                    save_regression_file,
                    save_to_config,
                    cli.changed_since.is_some() || cli.workspace.is_some(),
                    quiet,
                ),
            })
        }
        Command::Watch { no_clear } => watch::run_watch(&watch::WatchOptions {
            root,
            config_path: &cli.config,
            output,
            no_cache: cli.no_cache,
            threads,
            quiet,
            production: cli.production,
            clear_screen: !no_clear,
            explain: cli.explain,
        }),
        Command::Fix { dry_run, yes } => fix::run_fix(&fix::FixOptions {
            root,
            config_path: &cli.config,
            output,
            no_cache: cli.no_cache,
            threads,
            quiet,
            dry_run,
            yes,
            production: cli.production,
        }),
        Command::Init {
            toml,
            hooks,
            branch,
        } => init::run_init(&init::InitOptions {
            root,
            use_toml: toml,
            hooks,
            branch: branch.as_deref(),
        }),
        Command::ConfigSchema => init::run_config_schema(),
        Command::PluginSchema => init::run_plugin_schema(),
        Command::List {
            entry_points,
            files,
            plugins,
            boundaries,
        } => list::run_list(&ListOptions {
            root,
            config_path: &cli.config,
            output,
            threads,
            no_cache: cli.no_cache,
            entry_points,
            files,
            plugins,
            boundaries,
            production: cli.production,
        }),
        Command::Dupes {
            mode,
            min_tokens,
            min_lines,
            threshold,
            skip_local,
            cross_language,
            top,
            trace,
        } => {
            let (output, quiet, _fail_on_issues) = apply_ci_defaults(
                cli.ci,
                cli.fail_on_issues,
                output,
                quiet,
                cli_format_was_explicit,
            );
            dupes::run_dupes(&DupesOptions {
                root,
                config_path: &cli.config,
                output,
                no_cache: cli.no_cache,
                threads,
                quiet,
                mode,
                min_tokens,
                min_lines,
                threshold,
                skip_local,
                cross_language,
                top,
                baseline_path: cli.baseline.as_deref(),
                save_baseline_path: cli.save_baseline.as_deref(),
                production: cli.production,
                trace: trace.as_deref(),
                changed_since: cli.changed_since.as_deref(),
                explain: cli.explain,
                summary: cli.summary,
                group_by: cli.group_by,
            })
        }
        Command::Health {
            max_cyclomatic,
            max_cognitive,
            top,
            sort,
            complexity,
            file_scores,
            coverage_gaps,
            hotspots,
            targets,
            effort,
            score,
            min_score,
            since,
            min_commits,
            save_snapshot,
            trend,
        } => dispatch_health(
            cli,
            root,
            output,
            quiet,
            cli_format_was_explicit,
            threads,
            max_cyclomatic,
            max_cognitive,
            top,
            sort,
            complexity,
            file_scores,
            coverage_gaps,
            hotspots,
            targets,
            effort,
            score,
            min_score,
            since.as_deref(),
            min_commits,
            save_snapshot.as_ref(),
            trend,
        ),
        Command::Audit => audit::run_audit(&audit::AuditOptions {
            root,
            config_path: &cli.config,
            output,
            no_cache: cli.no_cache,
            threads,
            quiet,
            changed_since: cli.changed_since.as_deref(),
            production: cli.production,
            workspace: cli.workspace.as_deref(),
            explain: cli.explain,
            performance: cli.performance,
            group_by: cli.group_by,
        }),
        Command::Schema => unreachable!("handled above"),
        Command::Migrate {
            toml,
            dry_run,
            from,
        } => migrate::run_migrate(root, toml, dry_run, from.as_deref()),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "CLI dispatch forwards many flags"
)]
fn dispatch_health(
    cli: &Cli,
    root: &std::path::Path,
    output: fallow_config::OutputFormat,
    quiet: bool,
    cli_format_was_explicit: bool,
    threads: usize,
    max_cyclomatic: Option<u16>,
    max_cognitive: Option<u16>,
    top: Option<usize>,
    sort: health::SortBy,
    complexity: bool,
    file_scores: bool,
    coverage_gaps: bool,
    hotspots: bool,
    targets: bool,
    effort: Option<EffortFilter>,
    score: bool,
    min_score: Option<f64>,
    since: Option<&str>,
    min_commits: Option<u32>,
    save_snapshot: Option<&Option<String>>,
    trend: bool,
) -> ExitCode {
    let (output, quiet, _fail_on_issues) = apply_ci_defaults(
        cli.ci,
        cli.fail_on_issues,
        output,
        quiet,
        cli_format_was_explicit,
    );
    // --effort implies --targets
    let targets = targets || effort.is_some();
    // --min-score, --save-snapshot, --trend, and --format badge imply --score
    let badge_format = matches!(output, fallow_config::OutputFormat::Badge);
    let score = score || min_score.is_some() || trend || badge_format;
    let snapshot_requested = save_snapshot.is_some();
    // No section flags = show all (including score). Any flag set = show only those.
    // --save-snapshot and --trend are orthogonal (not section flags) but force score.
    let any_section = complexity || file_scores || coverage_gaps || hotspots || targets || score;
    let eff_score = if any_section { score } else { true } || snapshot_requested;
    // Score needs full pipeline for accuracy
    let force_full = snapshot_requested || eff_score;
    let eff_file_scores = if any_section { file_scores } else { true } || force_full;
    let eff_coverage_gaps = if any_section { coverage_gaps } else { false };
    let eff_hotspots = if any_section { hotspots } else { true } || force_full;
    let eff_complexity = if any_section { complexity } else { true };
    let eff_targets = if any_section { targets } else { true };
    health::run_health(&HealthOptions {
        root,
        config_path: &cli.config,
        output,
        no_cache: cli.no_cache,
        threads,
        quiet,
        max_cyclomatic,
        max_cognitive,
        top,
        sort,
        production: cli.production,
        changed_since: cli.changed_since.as_deref(),
        workspace: cli.workspace.as_deref(),
        baseline: cli.baseline.as_deref(),
        save_baseline: cli.save_baseline.as_deref(),
        complexity: eff_complexity,
        file_scores: eff_file_scores,
        coverage_gaps: eff_coverage_gaps,
        hotspots: eff_hotspots,
        targets: eff_targets,
        effort: effort.map(EffortFilter::to_estimate),
        score: eff_score,
        min_score,
        since,
        min_commits,
        explain: cli.explain,
        summary: cli.summary,
        save_snapshot: save_snapshot.map(|opt| PathBuf::from(opt.as_deref().unwrap_or_default())),
        trend,
        group_by: cli.group_by,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── emit_error ──────────────────────────────────────────────────

    #[test]
    fn emit_error_returns_given_exit_code() {
        let code = emit_error("test error", 2, fallow_config::OutputFormat::Human);
        assert_eq!(code, ExitCode::from(2));
    }

    // ── format/quiet parsing logic ─────────────────────────────────
    // Note: format_from_env() and quiet_from_env() read process-global
    // env vars, so we test the underlying parsing logic directly to
    // avoid unsafe set_var/remove_var and parallel test interference.

    #[test]
    fn format_parsing_covers_all_variants() {
        // The format_from_env function lowercases then matches.
        // Test the same logic inline.
        let parse = |s: &str| -> Option<Format> {
            match s.to_lowercase().as_str() {
                "json" => Some(Format::Json),
                "human" => Some(Format::Human),
                "sarif" => Some(Format::Sarif),
                "compact" => Some(Format::Compact),
                "markdown" | "md" => Some(Format::Markdown),
                "codeclimate" => Some(Format::CodeClimate),
                "badge" => Some(Format::Badge),
                _ => None,
            }
        };
        assert!(matches!(parse("json"), Some(Format::Json)));
        assert!(matches!(parse("JSON"), Some(Format::Json)));
        assert!(matches!(parse("human"), Some(Format::Human)));
        assert!(matches!(parse("sarif"), Some(Format::Sarif)));
        assert!(matches!(parse("compact"), Some(Format::Compact)));
        assert!(matches!(parse("markdown"), Some(Format::Markdown)));
        assert!(matches!(parse("md"), Some(Format::Markdown)));
        assert!(matches!(parse("codeclimate"), Some(Format::CodeClimate)));
        assert!(matches!(parse("badge"), Some(Format::Badge)));
        assert!(parse("xml").is_none());
        assert!(parse("").is_none());
    }

    #[test]
    fn quiet_parsing_logic() {
        let parse = |s: &str| -> bool { s == "1" || s.eq_ignore_ascii_case("true") };
        assert!(parse("1"));
        assert!(parse("true"));
        assert!(parse("TRUE"));
        assert!(parse("True"));
        assert!(!parse("0"));
        assert!(!parse("false"));
        assert!(!parse("yes"));
    }
}
