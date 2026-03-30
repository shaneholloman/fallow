use std::path::Path;
use std::process::ExitCode;

use fallow_core::results::AnalysisResults;

// ── Tolerance ───────────────────────────────────────────────────

/// How much increase is allowed before a regression is flagged.
#[derive(Debug, Clone, Copy)]
pub enum Tolerance {
    /// Percentage increase relative to the baseline total (e.g., 2.0 means 2%).
    Percentage(f64),
    /// Absolute increase in issue count.
    Absolute(usize),
}

impl Tolerance {
    /// Parse a tolerance string: `"2%"` for percentage, `"5"` for absolute.
    /// Default when no value is given: `Absolute(0)` (zero tolerance).
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid number or percentage,
    /// or if a percentage value is negative.
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Ok(Self::Absolute(0));
        }
        if let Some(pct_str) = s.strip_suffix('%') {
            let pct: f64 = pct_str
                .trim()
                .parse()
                .map_err(|_| format!("invalid tolerance percentage: {s}"))?;
            if pct < 0.0 {
                return Err(format!("tolerance percentage must be non-negative: {s}"));
            }
            Ok(Self::Percentage(pct))
        } else {
            let abs: usize = s
                .parse()
                .map_err(|_| format!("invalid tolerance value: {s} (use a number or N%)"))?;
            Ok(Self::Absolute(abs))
        }
    }

    /// Check whether the delta exceeds this tolerance.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "percentage of a count is bounded by the count itself"
    )]
    fn exceeded(&self, baseline_total: usize, current_total: usize) -> bool {
        if current_total <= baseline_total {
            return false;
        }
        let delta = current_total - baseline_total;
        match *self {
            Self::Percentage(pct) => {
                if baseline_total == 0 {
                    // Any increase from zero is a regression when pct tolerance is used
                    return delta > 0;
                }
                let allowed = (baseline_total as f64 * pct / 100.0).floor() as usize;
                delta > allowed
            }
            Self::Absolute(abs) => delta > abs,
        }
    }
}

// ── Regression baseline ─────────────────────────────────────────

/// Regression baseline: stores issue counts per type for comparison.
///
/// Unlike `BaselineData` which stores individual issue identities for suppression,
/// this stores counts for "did the total go up?" regression detection.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct RegressionBaseline {
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Fallow version that produced this baseline.
    pub fallow_version: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Git SHA at baseline time, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Dead code issue counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<CheckCounts>,
    /// Duplication counts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dupes: Option<DupesCounts>,
}

const REGRESSION_SCHEMA_VERSION: u32 = 1;

/// Per-type issue counts for dead code analysis.
///
/// All fields use `#[serde(default)]` for forward compatibility: when fallow adds a new
/// issue type, old baselines will deserialize with the new field defaulting to zero
/// instead of failing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckCounts {
    #[serde(default)]
    pub total_issues: usize,
    #[serde(default)]
    pub unused_files: usize,
    #[serde(default)]
    pub unused_exports: usize,
    #[serde(default)]
    pub unused_types: usize,
    #[serde(default)]
    pub unused_dependencies: usize,
    #[serde(default)]
    pub unused_dev_dependencies: usize,
    #[serde(default)]
    pub unused_optional_dependencies: usize,
    #[serde(default)]
    pub unused_enum_members: usize,
    #[serde(default)]
    pub unused_class_members: usize,
    #[serde(default)]
    pub unresolved_imports: usize,
    #[serde(default)]
    pub unlisted_dependencies: usize,
    #[serde(default)]
    pub duplicate_exports: usize,
    #[serde(default)]
    pub circular_dependencies: usize,
    #[serde(default)]
    pub type_only_dependencies: usize,
    #[serde(default)]
    pub test_only_dependencies: usize,
}

impl CheckCounts {
    #[must_use]
    pub const fn from_results(results: &AnalysisResults) -> Self {
        Self {
            total_issues: results.total_issues(),
            unused_files: results.unused_files.len(),
            unused_exports: results.unused_exports.len(),
            unused_types: results.unused_types.len(),
            unused_dependencies: results.unused_dependencies.len(),
            unused_dev_dependencies: results.unused_dev_dependencies.len(),
            unused_optional_dependencies: results.unused_optional_dependencies.len(),
            unused_enum_members: results.unused_enum_members.len(),
            unused_class_members: results.unused_class_members.len(),
            unresolved_imports: results.unresolved_imports.len(),
            unlisted_dependencies: results.unlisted_dependencies.len(),
            duplicate_exports: results.duplicate_exports.len(),
            circular_dependencies: results.circular_dependencies.len(),
            type_only_dependencies: results.type_only_dependencies.len(),
            test_only_dependencies: results.test_only_dependencies.len(),
        }
    }

    /// Convert from config-embedded baseline.
    #[must_use]
    pub const fn from_config_baseline(b: &fallow_config::RegressionBaseline) -> Self {
        Self {
            total_issues: b.total_issues,
            unused_files: b.unused_files,
            unused_exports: b.unused_exports,
            unused_types: b.unused_types,
            unused_dependencies: b.unused_dependencies,
            unused_dev_dependencies: b.unused_dev_dependencies,
            unused_optional_dependencies: b.unused_optional_dependencies,
            unused_enum_members: b.unused_enum_members,
            unused_class_members: b.unused_class_members,
            unresolved_imports: b.unresolved_imports,
            unlisted_dependencies: b.unlisted_dependencies,
            duplicate_exports: b.duplicate_exports,
            circular_dependencies: b.circular_dependencies,
            type_only_dependencies: b.type_only_dependencies,
            test_only_dependencies: b.test_only_dependencies,
        }
    }

    /// Convert to config-embeddable baseline.
    #[must_use]
    pub const fn to_config_baseline(&self) -> fallow_config::RegressionBaseline {
        fallow_config::RegressionBaseline {
            total_issues: self.total_issues,
            unused_files: self.unused_files,
            unused_exports: self.unused_exports,
            unused_types: self.unused_types,
            unused_dependencies: self.unused_dependencies,
            unused_dev_dependencies: self.unused_dev_dependencies,
            unused_optional_dependencies: self.unused_optional_dependencies,
            unused_enum_members: self.unused_enum_members,
            unused_class_members: self.unused_class_members,
            unresolved_imports: self.unresolved_imports,
            unlisted_dependencies: self.unlisted_dependencies,
            duplicate_exports: self.duplicate_exports,
            circular_dependencies: self.circular_dependencies,
            type_only_dependencies: self.type_only_dependencies,
            test_only_dependencies: self.test_only_dependencies,
        }
    }

    /// Per-type deltas (current - baseline) for display. Only includes types with changes.
    fn deltas(&self, current: &Self) -> Vec<(&'static str, isize)> {
        let pairs: Vec<(&str, usize, usize)> = vec![
            ("unused_files", self.unused_files, current.unused_files),
            (
                "unused_exports",
                self.unused_exports,
                current.unused_exports,
            ),
            ("unused_types", self.unused_types, current.unused_types),
            (
                "unused_dependencies",
                self.unused_dependencies,
                current.unused_dependencies,
            ),
            (
                "unused_dev_dependencies",
                self.unused_dev_dependencies,
                current.unused_dev_dependencies,
            ),
            (
                "unused_optional_dependencies",
                self.unused_optional_dependencies,
                current.unused_optional_dependencies,
            ),
            (
                "unused_enum_members",
                self.unused_enum_members,
                current.unused_enum_members,
            ),
            (
                "unused_class_members",
                self.unused_class_members,
                current.unused_class_members,
            ),
            (
                "unresolved_imports",
                self.unresolved_imports,
                current.unresolved_imports,
            ),
            (
                "unlisted_dependencies",
                self.unlisted_dependencies,
                current.unlisted_dependencies,
            ),
            (
                "duplicate_exports",
                self.duplicate_exports,
                current.duplicate_exports,
            ),
            (
                "circular_dependencies",
                self.circular_dependencies,
                current.circular_dependencies,
            ),
            (
                "type_only_dependencies",
                self.type_only_dependencies,
                current.type_only_dependencies,
            ),
            (
                "test_only_dependencies",
                self.test_only_dependencies,
                current.test_only_dependencies,
            ),
        ];
        pairs
            .into_iter()
            .filter_map(|(name, baseline, current)| {
                let delta = current as isize - baseline as isize;
                if delta != 0 {
                    Some((name, delta))
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Duplication counts for regression baseline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DupesCounts {
    #[serde(default)]
    pub clone_groups: usize,
    #[serde(default)]
    pub duplication_percentage: f64,
}

// ── Regression outcome ──────────────────────────────────────────

/// Result of a regression check.
#[derive(Debug)]
pub enum RegressionOutcome {
    /// No regression — current issues are within tolerance.
    Pass {
        baseline_total: usize,
        current_total: usize,
    },
    /// Regression exceeded tolerance.
    Exceeded {
        baseline_total: usize,
        current_total: usize,
        tolerance: Tolerance,
        /// Per-type deltas for human output.
        type_deltas: Vec<(&'static str, isize)>,
    },
    /// Regression check was skipped (e.g., --changed-since active).
    Skipped { reason: &'static str },
}

impl RegressionOutcome {
    /// Whether this outcome should cause a non-zero exit code.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Exceeded { .. })
    }

    /// Build a JSON value for the regression outcome (added to JSON output envelope).
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Pass {
                baseline_total,
                current_total,
            } => serde_json::json!({
                "status": "pass",
                "baseline_total": baseline_total,
                "current_total": current_total,
                "delta": *current_total as isize - *baseline_total as isize,
                "exceeded": false,
            }),
            Self::Exceeded {
                baseline_total,
                current_total,
                tolerance,
                ..
            } => {
                let (tolerance_value, tolerance_kind) = match tolerance {
                    Tolerance::Percentage(pct) => (*pct, "percentage"),
                    Tolerance::Absolute(abs) => (*abs as f64, "absolute"),
                };
                serde_json::json!({
                    "status": "exceeded",
                    "baseline_total": baseline_total,
                    "current_total": current_total,
                    "delta": *current_total as isize - *baseline_total as isize,
                    "tolerance": tolerance_value,
                    "tolerance_kind": tolerance_kind,
                    "exceeded": true,
                })
            }
            Self::Skipped { reason } => serde_json::json!({
                "status": "skipped",
                "reason": reason,
                "exceeded": false,
            }),
        }
    }
}

// ── Public API ──────────────────────────────────────────────────

/// Where to save the regression baseline.
#[derive(Clone, Copy)]
pub enum SaveRegressionTarget<'a> {
    /// Don't save.
    None,
    /// Save into the config file (.fallowrc.json / fallow.toml).
    Config,
    /// Save to an explicit file path.
    File(&'a Path),
}

/// Options for regression detection.
#[derive(Clone, Copy)]
pub struct RegressionOpts<'a> {
    pub fail_on_regression: bool,
    pub tolerance: Tolerance,
    /// Explicit regression baseline file path (overrides config).
    pub regression_baseline_file: Option<&'a Path>,
    /// Where to save the regression baseline.
    pub save_target: SaveRegressionTarget<'a>,
    /// Whether --changed-since or --workspace is active (makes counts incomparable).
    pub scoped: bool,
    pub quiet: bool,
}

/// Check whether a path is likely gitignored by running `git check-ignore`.
/// Returns `false` if git is unavailable or the check fails (conservative).
fn is_likely_gitignored(path: &Path, root: &Path) -> bool {
    std::process::Command::new("git")
        .args(["check-ignore", "-q"])
        .arg(path)
        .current_dir(root)
        .output()
        .ok()
        .is_some_and(|o| o.status.success())
}

/// Get the current git SHA, if available.
fn current_git_sha(root: &Path) -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Save the current analysis results as a regression baseline.
///
/// # Errors
///
/// Returns an error if the baseline cannot be serialized or written to disk.
pub fn save_regression_baseline(
    path: &Path,
    root: &Path,
    check_counts: Option<&CheckCounts>,
    dupes_counts: Option<&DupesCounts>,
) -> Result<(), ExitCode> {
    let baseline = RegressionBaseline {
        schema_version: REGRESSION_SCHEMA_VERSION,
        fallow_version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono_now(),
        git_sha: current_git_sha(root),
        check: check_counts.cloned(),
        dupes: dupes_counts.cloned(),
    };
    let json = serde_json::to_string_pretty(&baseline).map_err(|e| {
        eprintln!("Error: failed to serialize regression baseline: {e}");
        ExitCode::from(2)
    })?;
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, json).map_err(|e| {
        eprintln!("Error: failed to save regression baseline: {e}");
        ExitCode::from(2)
    })?;
    // Always print save confirmation — this is a side effect the user must verify,
    // not progress noise that --quiet should suppress.
    eprintln!("Regression baseline saved to {}", path.display());
    // Warn if the saved path appears to be gitignored
    if is_likely_gitignored(path, root) {
        eprintln!(
            "Warning: '{}' may be gitignored. Commit this file so CI can compare against it.",
            path.display()
        );
    }
    Ok(())
}

/// Save regression baseline counts into the project's config file.
///
/// Reads the existing config, adds/updates the `regression.baseline` section,
/// and writes it back. For JSONC files, comments are preserved using a targeted
/// insertion/replacement strategy.
///
/// # Errors
///
/// Returns an error if the config file cannot be read, updated, or written back.
pub fn save_baseline_to_config(config_path: &Path, counts: &CheckCounts) -> Result<(), ExitCode> {
    // If the config file doesn't exist yet, create a minimal one
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let is_toml = config_path.extension().is_some_and(|ext| ext == "toml");
            if is_toml {
                String::new()
            } else {
                "{}".to_string()
            }
        }
        Err(e) => {
            eprintln!(
                "Error: failed to read config file '{}': {e}",
                config_path.display()
            );
            return Err(ExitCode::from(2));
        }
    };

    let baseline = counts.to_config_baseline();
    let is_toml = config_path.extension().is_some_and(|ext| ext == "toml");

    let updated = if is_toml {
        Ok(update_toml_regression(&content, &baseline))
    } else {
        update_json_regression(&content, &baseline)
    }
    .map_err(|e| {
        eprintln!(
            "Error: failed to update config file '{}': {e}",
            config_path.display()
        );
        ExitCode::from(2)
    })?;

    std::fs::write(config_path, updated).map_err(|e| {
        eprintln!(
            "Error: failed to write config file '{}': {e}",
            config_path.display()
        );
        ExitCode::from(2)
    })?;

    eprintln!(
        "Regression baseline saved to {} (regression.baseline section)",
        config_path.display()
    );
    Ok(())
}

/// Update a JSONC config file with regression baseline, preserving comments.
/// Find a JSON key in content, skipping `//` line comments and `/* */` block comments.
/// Returns the byte offset of the opening `"` of the key.
fn find_json_key(content: &str, key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(&needle) {
        let abs_pos = search_from + pos;
        // Check if this match is inside a // comment line
        let line_start = content[..abs_pos].rfind('\n').map_or(0, |i| i + 1);
        let line_prefix = content[line_start..abs_pos].trim_start();
        if line_prefix.starts_with("//") {
            search_from = abs_pos + needle.len();
            continue;
        }
        // Check if inside a /* */ block comment
        let before = &content[..abs_pos];
        let last_open = before.rfind("/*");
        let last_close = before.rfind("*/");
        if let Some(open_pos) = last_open
            && last_close.is_none_or(|close_pos| close_pos < open_pos)
        {
            search_from = abs_pos + needle.len();
            continue;
        }
        return Some(abs_pos);
    }
    None
}

fn update_json_regression(
    content: &str,
    baseline: &fallow_config::RegressionBaseline,
) -> Result<String, String> {
    let baseline_json =
        serde_json::to_string_pretty(baseline).map_err(|e| format!("serialization error: {e}"))?;

    // Indent the baseline JSON by 4 spaces (nested inside "regression": { "baseline": ... })
    let indented: String = baseline_json
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("    {line}")
            } else {
                format!("\n    {line}")
            }
        })
        .collect();

    let regression_block = format!("  \"regression\": {{\n    \"baseline\": {indented}\n  }}");

    // Check if "regression" key already exists — replace it.
    // Only match "regression" that appears as a JSON key (preceded by whitespace or line start),
    // not inside comments or string values.
    if let Some(start) = find_json_key(content, "regression") {
        let after_key = &content[start..];
        if let Some(brace_start) = after_key.find('{') {
            let abs_brace = start + brace_start;
            let mut depth = 0;
            let mut end = abs_brace;
            let mut found_close = false;
            for (i, ch) in content[abs_brace..].char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = abs_brace + i + 1;
                            found_close = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if !found_close {
                return Err("malformed JSON: unmatched brace in regression object".to_string());
            }
            let mut result = String::new();
            result.push_str(&content[..start]);
            result.push_str(&regression_block[2..]); // skip leading "  " — reuse original indent
            result.push_str(&content[end..]);
            return Ok(result);
        }
    }

    // No existing regression key — insert before the last `}`
    if let Some(last_brace) = content.rfind('}') {
        // Find the last non-whitespace character before the closing brace
        let before_brace = content[..last_brace].trim_end();
        let needs_comma = !before_brace.ends_with('{') && !before_brace.ends_with(',');

        let mut result = String::new();
        result.push_str(before_brace);
        if needs_comma {
            result.push(',');
        }
        result.push('\n');
        result.push_str(&regression_block);
        result.push('\n');
        result.push_str(&content[last_brace..]);
        Ok(result)
    } else {
        Err("config file has no closing brace".to_string())
    }
}

/// Update a TOML config file with regression baseline.
fn update_toml_regression(content: &str, baseline: &fallow_config::RegressionBaseline) -> String {
    use std::fmt::Write;
    // Build the TOML section
    let mut section = String::from("[regression.baseline]\n");
    let _ = writeln!(section, "totalIssues = {}", baseline.total_issues);
    let _ = writeln!(section, "unusedFiles = {}", baseline.unused_files);
    let _ = writeln!(section, "unusedExports = {}", baseline.unused_exports);
    let _ = writeln!(section, "unusedTypes = {}", baseline.unused_types);
    let _ = writeln!(
        section,
        "unusedDependencies = {}",
        baseline.unused_dependencies
    );
    let _ = writeln!(
        section,
        "unusedDevDependencies = {}",
        baseline.unused_dev_dependencies
    );
    let _ = writeln!(
        section,
        "unusedOptionalDependencies = {}",
        baseline.unused_optional_dependencies
    );
    let _ = writeln!(
        section,
        "unusedEnumMembers = {}",
        baseline.unused_enum_members
    );
    let _ = writeln!(
        section,
        "unusedClassMembers = {}",
        baseline.unused_class_members
    );
    let _ = writeln!(
        section,
        "unresolvedImports = {}",
        baseline.unresolved_imports
    );
    let _ = writeln!(
        section,
        "unlistedDependencies = {}",
        baseline.unlisted_dependencies
    );
    let _ = writeln!(section, "duplicateExports = {}", baseline.duplicate_exports);
    let _ = writeln!(
        section,
        "circularDependencies = {}",
        baseline.circular_dependencies
    );
    let _ = writeln!(
        section,
        "typeOnlyDependencies = {}",
        baseline.type_only_dependencies
    );
    let _ = writeln!(
        section,
        "testOnlyDependencies = {}",
        baseline.test_only_dependencies
    );

    // Check if [regression.baseline] already exists — replace it
    if let Some(start) = content.find("[regression.baseline]") {
        // Find the next section header or end of file
        let after = &content[start + "[regression.baseline]".len()..];
        let end_offset = after.find("\n[").map_or(content.len(), |i| {
            start + "[regression.baseline]".len() + i + 1
        });

        let mut result = String::new();
        result.push_str(&content[..start]);
        result.push_str(&section);
        if end_offset < content.len() {
            result.push_str(&content[end_offset..]);
        }
        result
    } else {
        // Append the section
        let mut result = content.to_string();
        if !result.ends_with('\n') {
            result.push('\n');
        }
        result.push('\n');
        result.push_str(&section);
        result
    }
}

/// Load a regression baseline from disk.
///
/// # Errors
///
/// Returns an error if the file does not exist, cannot be read, or contains invalid JSON.
pub fn load_regression_baseline(path: &Path) -> Result<RegressionBaseline, ExitCode> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            eprintln!(
                "Error: no regression baseline found at '{}'.\n\
                 Run with --save-regression-baseline on your main branch to create one.",
                path.display()
            );
        } else {
            eprintln!(
                "Error: failed to read regression baseline '{}': {e}",
                path.display()
            );
        }
        ExitCode::from(2)
    })?;
    serde_json::from_str(&content).map_err(|e| {
        eprintln!(
            "Error: failed to parse regression baseline '{}': {e}",
            path.display()
        );
        ExitCode::from(2)
    })
}

/// Compare current check results against a regression baseline.
///
/// Resolution order for the baseline:
/// 1. Explicit file via `--regression-baseline <PATH>`
/// 2. Config-embedded `regression.baseline` section
/// 3. Error with actionable message
///
/// # Errors
///
/// Returns an error if the baseline file cannot be loaded, is missing check data,
/// or no baseline source is available.
pub fn compare_check_regression(
    results: &AnalysisResults,
    opts: &RegressionOpts<'_>,
    config_baseline: Option<&fallow_config::RegressionBaseline>,
) -> Result<Option<RegressionOutcome>, ExitCode> {
    if !opts.fail_on_regression {
        return Ok(None);
    }

    // Skip if results are scoped (counts not comparable to full-project baseline)
    if opts.scoped {
        let reason = "--changed-since or --workspace is active; regression check skipped \
                      (counts not comparable to full-project baseline)";
        if !opts.quiet {
            eprintln!("Warning: {reason}");
        }
        return Ok(Some(RegressionOutcome::Skipped { reason }));
    }

    // Resolution order: explicit file > config section > error
    let baseline_counts: CheckCounts = if let Some(baseline_path) = opts.regression_baseline_file {
        // Explicit --regression-baseline <PATH>: load from file
        let baseline = load_regression_baseline(baseline_path)?;
        let Some(counts) = baseline.check else {
            eprintln!(
                "Error: regression baseline '{}' has no check data",
                baseline_path.display()
            );
            return Err(ExitCode::from(2));
        };
        counts
    } else if let Some(config_baseline) = config_baseline {
        // Config-embedded baseline: read from .fallowrc.json / fallow.toml
        CheckCounts::from_config_baseline(config_baseline)
    } else {
        eprintln!(
            "Error: no regression baseline found.\n\
             Either add a `regression.baseline` section to your config file\n\
             (run with --save-regression-baseline to generate it),\n\
             or provide an explicit file via --regression-baseline <PATH>."
        );
        return Err(ExitCode::from(2));
    };

    let current_total = results.total_issues();
    let baseline_total = baseline_counts.total_issues;

    if opts.tolerance.exceeded(baseline_total, current_total) {
        let current_counts = CheckCounts::from_results(results);
        let type_deltas = baseline_counts.deltas(&current_counts);
        Ok(Some(RegressionOutcome::Exceeded {
            baseline_total,
            current_total,
            tolerance: opts.tolerance,
            type_deltas,
        }))
    } else {
        Ok(Some(RegressionOutcome::Pass {
            baseline_total,
            current_total,
        }))
    }
}

/// Print regression outcome to stderr (human-readable summary).
pub fn print_regression_outcome(outcome: &RegressionOutcome) {
    match outcome {
        RegressionOutcome::Pass {
            baseline_total,
            current_total,
        } => {
            let delta = *current_total as isize - *baseline_total as isize;
            let sign = if delta >= 0 { "+" } else { "" };
            eprintln!(
                "Regression check passed: {current_total} issues (baseline: {baseline_total}, \
                 delta: {sign}{delta})"
            );
        }
        RegressionOutcome::Exceeded {
            baseline_total,
            current_total,
            tolerance,
            type_deltas,
        } => {
            let delta = *current_total as isize - *baseline_total as isize;
            let tol_str = match tolerance {
                Tolerance::Percentage(pct) => format!("{pct}%"),
                Tolerance::Absolute(abs) => format!("{abs}"),
            };
            eprintln!(
                "Regression detected: {current_total} issues (baseline: {baseline_total}, \
                 delta: +{delta}, tolerance: {tol_str})"
            );
            for (name, d) in type_deltas {
                let sign = if *d > 0 { "+" } else { "" };
                eprintln!("  {name}: {sign}{d}");
            }
        }
        RegressionOutcome::Skipped { .. } => {
            // Warning already printed in compare_* functions
        }
    }
}

/// ISO 8601 UTC timestamp without external dependencies.
fn chrono_now() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Manual UTC decomposition — avoids chrono dependency
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    // Days since epoch to Y-M-D (civil date algorithm)
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fallow_core::results::*;
    use std::path::PathBuf;

    // ── Tolerance parsing ───────────────────────────────────────────

    #[test]
    fn parse_percentage_tolerance() {
        let t = Tolerance::parse("2%").unwrap();
        assert!(matches!(t, Tolerance::Percentage(p) if (p - 2.0).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_absolute_tolerance() {
        let t = Tolerance::parse("5").unwrap();
        assert!(matches!(t, Tolerance::Absolute(5)));
    }

    #[test]
    fn parse_zero_tolerance() {
        let t = Tolerance::parse("0").unwrap();
        assert!(matches!(t, Tolerance::Absolute(0)));
    }

    #[test]
    fn parse_empty_defaults_to_zero() {
        let t = Tolerance::parse("").unwrap();
        assert!(matches!(t, Tolerance::Absolute(0)));
    }

    #[test]
    fn parse_invalid_percentage() {
        assert!(Tolerance::parse("abc%").is_err());
    }

    #[test]
    fn parse_negative_percentage() {
        assert!(Tolerance::parse("-1%").is_err());
    }

    #[test]
    fn parse_invalid_absolute() {
        assert!(Tolerance::parse("abc").is_err());
    }

    // ── Tolerance::exceeded ────────────────────────────────────────

    #[test]
    fn zero_tolerance_detects_any_increase() {
        let t = Tolerance::Absolute(0);
        assert!(t.exceeded(10, 11));
        assert!(!t.exceeded(10, 10));
        assert!(!t.exceeded(10, 9));
    }

    #[test]
    fn absolute_tolerance_allows_within_range() {
        let t = Tolerance::Absolute(3);
        assert!(!t.exceeded(10, 12)); // delta=2, allowed=3
        assert!(!t.exceeded(10, 13)); // delta=3, allowed=3
        assert!(t.exceeded(10, 14)); // delta=4, allowed=3
    }

    #[test]
    fn percentage_tolerance_allows_within_range() {
        let t = Tolerance::Percentage(10.0);
        assert!(!t.exceeded(100, 109)); // delta=9, allowed=floor(10)=10
        assert!(!t.exceeded(100, 110)); // delta=10, allowed=10
        assert!(t.exceeded(100, 111)); // delta=11, allowed=10
    }

    #[test]
    fn percentage_tolerance_from_zero_baseline() {
        let t = Tolerance::Percentage(10.0);
        assert!(t.exceeded(0, 1)); // any increase from zero
        assert!(!t.exceeded(0, 0)); // no increase
    }

    #[test]
    fn decrease_never_exceeds() {
        let t = Tolerance::Absolute(0);
        assert!(!t.exceeded(10, 5));
        let t = Tolerance::Percentage(0.0);
        assert!(!t.exceeded(10, 5));
    }

    // ── CheckCounts::from_results ──────────────────────────────────

    #[test]
    fn check_counts_from_results() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        results.unused_exports.push(UnusedExport {
            path: PathBuf::from("b.ts"),
            export_name: "foo".into(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });
        let counts = CheckCounts::from_results(&results);
        assert_eq!(counts.total_issues, 2);
        assert_eq!(counts.unused_files, 1);
        assert_eq!(counts.unused_exports, 1);
        assert_eq!(counts.unused_types, 0);
    }

    // ── CheckCounts::deltas ────────────────────────────────────────

    #[test]
    fn deltas_reports_changes_only() {
        let baseline = CheckCounts {
            total_issues: 10,
            unused_files: 5,
            unused_exports: 3,
            unused_types: 2,
            unused_dependencies: 0,
            unused_dev_dependencies: 0,
            unused_optional_dependencies: 0,
            unused_enum_members: 0,
            unused_class_members: 0,
            unresolved_imports: 0,
            unlisted_dependencies: 0,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let current = CheckCounts {
            unused_files: 7,   // +2
            unused_exports: 1, // -2
            unused_types: 2,   // 0 (no change)
            ..baseline
        };
        let deltas = baseline.deltas(&current);
        assert_eq!(deltas.len(), 2);
        assert!(deltas.contains(&("unused_files", 2)));
        assert!(deltas.contains(&("unused_exports", -2)));
    }

    // ── RegressionOutcome::to_json ──────────────────────────────────

    #[test]
    fn pass_outcome_json() {
        let outcome = RegressionOutcome::Pass {
            baseline_total: 10,
            current_total: 10,
        };
        let json = outcome.to_json();
        assert_eq!(json["status"], "pass");
        assert_eq!(json["exceeded"], false);
        assert_eq!(json["delta"], 0);
    }

    #[test]
    fn exceeded_outcome_json() {
        let outcome = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Percentage(2.0),
            type_deltas: vec![("unused_files", 5)],
        };
        let json = outcome.to_json();
        assert_eq!(json["status"], "exceeded");
        assert_eq!(json["exceeded"], true);
        assert_eq!(json["delta"], 5);
        assert_eq!(json["tolerance_kind"], "percentage");
    }

    #[test]
    fn skipped_outcome_json() {
        let outcome = RegressionOutcome::Skipped {
            reason: "test reason",
        };
        let json = outcome.to_json();
        assert_eq!(json["status"], "skipped");
        assert_eq!(json["exceeded"], false);
    }

    // ── Regression baseline serialization roundtrip ────────────────

    #[test]
    fn regression_baseline_roundtrip() {
        let baseline = RegressionBaseline {
            schema_version: 1,
            fallow_version: "2.4.0".into(),
            timestamp: "2026-03-27T10:00:00Z".into(),
            git_sha: Some("abc123".into()),
            check: Some(CheckCounts {
                total_issues: 42,
                unused_files: 5,
                unused_exports: 20,
                unused_types: 8,
                unused_dependencies: 3,
                unused_dev_dependencies: 2,
                unused_optional_dependencies: 0,
                unused_enum_members: 1,
                unused_class_members: 1,
                unresolved_imports: 0,
                unlisted_dependencies: 1,
                duplicate_exports: 0,
                circular_dependencies: 1,
                type_only_dependencies: 0,
                test_only_dependencies: 0,
            }),
            dupes: Some(DupesCounts {
                clone_groups: 12,
                duplication_percentage: 4.2,
            }),
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        let loaded: RegressionBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.check.as_ref().unwrap().total_issues, 42);
        assert_eq!(loaded.dupes.as_ref().unwrap().clone_groups, 12);
    }

    // ── Tolerance display in regression messages ────────────────────

    #[test]
    fn regression_outcome_is_failure() {
        let pass = RegressionOutcome::Pass {
            baseline_total: 10,
            current_total: 10,
        };
        assert!(!pass.is_failure());

        let exceeded = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Absolute(2),
            type_deltas: vec![],
        };
        assert!(exceeded.is_failure());

        let skipped = RegressionOutcome::Skipped { reason: "test" };
        assert!(!skipped.is_failure());
    }

    // ── update_json_regression ──────────────────────────────────────

    fn sample_baseline() -> fallow_config::RegressionBaseline {
        fallow_config::RegressionBaseline {
            total_issues: 5,
            unused_files: 2,
            ..Default::default()
        }
    }

    #[test]
    fn json_insert_into_empty_object() {
        let result = update_json_regression("{}", &sample_baseline()).unwrap();
        assert!(result.contains("\"regression\""));
        assert!(result.contains("\"totalIssues\": 5"));
        // Should be valid JSON
        serde_json::from_str::<serde_json::Value>(&result).unwrap();
    }

    #[test]
    fn json_insert_into_existing_config() {
        let config = r#"{
  "entry": ["src/main.ts"],
  "production": true
}"#;
        let result = update_json_regression(config, &sample_baseline()).unwrap();
        assert!(result.contains("\"regression\""));
        assert!(result.contains("\"entry\""));
        serde_json::from_str::<serde_json::Value>(&result).unwrap();
    }

    #[test]
    fn json_replace_existing_regression() {
        let config = r#"{
  "entry": ["src/main.ts"],
  "regression": {
    "baseline": {
      "totalIssues": 99
    }
  }
}"#;
        let result = update_json_regression(config, &sample_baseline()).unwrap();
        // Old value replaced
        assert!(!result.contains("99"));
        assert!(result.contains("\"totalIssues\": 5"));
        serde_json::from_str::<serde_json::Value>(&result).unwrap();
    }

    #[test]
    fn json_skips_regression_in_comment() {
        let config = "{\n  // See \"regression\" docs\n  \"entry\": []\n}";
        let result = update_json_regression(config, &sample_baseline()).unwrap();
        // Should insert new regression, not try to replace the comment
        assert!(result.contains("\"regression\":"));
        assert!(result.contains("\"entry\""));
    }

    #[test]
    fn json_malformed_brace_returns_error() {
        // regression key exists but no matching closing brace
        let config = r#"{ "regression": { "baseline": { "totalIssues": 1 }"#;
        let result = update_json_regression(config, &sample_baseline());
        assert!(result.is_err());
    }

    // ── update_toml_regression ──────────────────────────────────────

    #[test]
    fn toml_insert_into_empty() {
        let result = update_toml_regression("", &sample_baseline());
        assert!(result.contains("[regression.baseline]"));
        assert!(result.contains("totalIssues = 5"));
    }

    #[test]
    fn toml_insert_after_existing_content() {
        let config = "[rules]\nunused-files = \"warn\"\n";
        let result = update_toml_regression(config, &sample_baseline());
        assert!(result.contains("[rules]"));
        assert!(result.contains("[regression.baseline]"));
        assert!(result.contains("totalIssues = 5"));
    }

    #[test]
    fn toml_replace_existing_section() {
        let config =
            "[regression.baseline]\ntotalIssues = 99\n\n[rules]\nunused-files = \"warn\"\n";
        let result = update_toml_regression(config, &sample_baseline());
        assert!(!result.contains("99"));
        assert!(result.contains("totalIssues = 5"));
        assert!(result.contains("[rules]"));
    }

    // ── find_json_key ───────────────────────────────────────────────

    #[test]
    fn find_json_key_basic() {
        assert_eq!(find_json_key(r#"{"foo": 1}"#, "foo"), Some(1));
    }

    #[test]
    fn find_json_key_skips_comment() {
        let content = "{\n  // \"foo\" is important\n  \"bar\": 1\n}";
        assert_eq!(find_json_key(content, "foo"), None);
        assert!(find_json_key(content, "bar").is_some());
    }

    #[test]
    fn find_json_key_not_found() {
        assert_eq!(find_json_key("{}", "missing"), None);
    }

    #[test]
    fn find_json_key_skips_block_comment() {
        let content = "{\n  /* \"foo\": old value */\n  \"foo\": 1\n}";
        // Should find the real key, not the one inside /* */
        let pos = find_json_key(content, "foo").unwrap();
        assert!(content[pos..].starts_with("\"foo\": 1"));
    }

    // ── Additional tolerance parsing ────────────────────────────────

    #[test]
    fn parse_whitespace_padded_tolerance() {
        let t = Tolerance::parse("  5  ").unwrap();
        assert!(matches!(t, Tolerance::Absolute(5)));
    }

    #[test]
    fn parse_whitespace_only_defaults_to_zero() {
        let t = Tolerance::parse("   ").unwrap();
        assert!(matches!(t, Tolerance::Absolute(0)));
    }

    #[test]
    fn parse_zero_percent_tolerance() {
        let t = Tolerance::parse("0%").unwrap();
        assert!(matches!(t, Tolerance::Percentage(p) if p == 0.0));
    }

    #[test]
    fn parse_decimal_percentage_tolerance() {
        let t = Tolerance::parse("1.5%").unwrap();
        assert!(matches!(t, Tolerance::Percentage(p) if (p - 1.5).abs() < f64::EPSILON));
    }

    #[test]
    fn parse_large_absolute_tolerance() {
        let t = Tolerance::parse("1000").unwrap();
        assert!(matches!(t, Tolerance::Absolute(1000)));
    }

    #[test]
    fn parse_negative_absolute_is_err() {
        // usize can't be negative, so parsing "-1" as usize fails
        assert!(Tolerance::parse("-1").is_err());
    }

    #[test]
    fn parse_whitespace_padded_percentage() {
        let t = Tolerance::parse("  3.5%  ").unwrap();
        assert!(matches!(t, Tolerance::Percentage(p) if (p - 3.5).abs() < f64::EPSILON));
    }

    // ── Additional Tolerance::exceeded ──────────────────────────────

    #[test]
    fn zero_pct_tolerance_detects_any_increase() {
        let t = Tolerance::Percentage(0.0);
        assert!(t.exceeded(100, 101));
        assert!(!t.exceeded(100, 100));
        assert!(!t.exceeded(100, 99));
    }

    #[test]
    fn percentage_tolerance_with_small_baseline() {
        // baseline=3, 10% of 3 = 0.3, floor = 0 => delta > 0 triggers
        let t = Tolerance::Percentage(10.0);
        assert!(t.exceeded(3, 4)); // delta=1 > allowed=0
        assert!(!t.exceeded(3, 3)); // no increase
    }

    #[test]
    fn percentage_tolerance_large_percentage() {
        let t = Tolerance::Percentage(100.0);
        // baseline=10, 100% of 10 = 10, floor=10 => delta > 10 triggers
        assert!(!t.exceeded(10, 20)); // delta=10, allowed=10
        assert!(t.exceeded(10, 21)); // delta=11, allowed=10
    }

    #[test]
    fn absolute_tolerance_at_exact_boundary() {
        let t = Tolerance::Absolute(5);
        assert!(!t.exceeded(10, 15)); // delta=5, allowed=5
        assert!(t.exceeded(10, 16)); // delta=6, allowed=5
    }

    #[test]
    fn decrease_never_exceeds_for_all_variants() {
        let t = Tolerance::Absolute(0);
        assert!(!t.exceeded(10, 0));
        let t = Tolerance::Percentage(0.0);
        assert!(!t.exceeded(10, 0));
    }

    #[test]
    fn equal_values_never_exceed() {
        assert!(!Tolerance::Absolute(0).exceeded(0, 0));
        assert!(!Tolerance::Percentage(0.0).exceeded(0, 0));
        assert!(!Tolerance::Absolute(0).exceeded(100, 100));
        assert!(!Tolerance::Percentage(0.0).exceeded(100, 100));
    }

    // ── CheckCounts config baseline roundtrip ────────────────────────

    #[test]
    fn check_counts_config_roundtrip() {
        let counts = CheckCounts {
            total_issues: 42,
            unused_files: 5,
            unused_exports: 20,
            unused_types: 8,
            unused_dependencies: 3,
            unused_dev_dependencies: 2,
            unused_optional_dependencies: 1,
            unused_enum_members: 1,
            unused_class_members: 1,
            unresolved_imports: 0,
            unlisted_dependencies: 1,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let config_baseline = counts.to_config_baseline();
        let roundtripped = CheckCounts::from_config_baseline(&config_baseline);
        assert_eq!(roundtripped.total_issues, 42);
        assert_eq!(roundtripped.unused_files, 5);
        assert_eq!(roundtripped.unused_exports, 20);
        assert_eq!(roundtripped.unused_types, 8);
        assert_eq!(roundtripped.unused_dependencies, 3);
        assert_eq!(roundtripped.unused_dev_dependencies, 2);
        assert_eq!(roundtripped.unused_optional_dependencies, 1);
        assert_eq!(roundtripped.unused_enum_members, 1);
        assert_eq!(roundtripped.unused_class_members, 1);
        assert_eq!(roundtripped.unresolved_imports, 0);
        assert_eq!(roundtripped.unlisted_dependencies, 1);
        assert_eq!(roundtripped.duplicate_exports, 0);
        assert_eq!(roundtripped.circular_dependencies, 0);
        assert_eq!(roundtripped.type_only_dependencies, 0);
        assert_eq!(roundtripped.test_only_dependencies, 0);
    }

    #[test]
    fn check_counts_zero_config_roundtrip() {
        let counts = CheckCounts {
            total_issues: 0,
            unused_files: 0,
            unused_exports: 0,
            unused_types: 0,
            unused_dependencies: 0,
            unused_dev_dependencies: 0,
            unused_optional_dependencies: 0,
            unused_enum_members: 0,
            unused_class_members: 0,
            unresolved_imports: 0,
            unlisted_dependencies: 0,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let config_baseline = counts.to_config_baseline();
        let roundtripped = CheckCounts::from_config_baseline(&config_baseline);
        assert_eq!(roundtripped.total_issues, 0);
        assert_eq!(roundtripped.unused_files, 0);
    }

    // ── deltas edge cases ──────────────────────────────────────────

    #[test]
    fn deltas_empty_when_identical() {
        let counts = CheckCounts {
            total_issues: 10,
            unused_files: 5,
            unused_exports: 3,
            unused_types: 2,
            unused_dependencies: 0,
            unused_dev_dependencies: 0,
            unused_optional_dependencies: 0,
            unused_enum_members: 0,
            unused_class_members: 0,
            unresolved_imports: 0,
            unlisted_dependencies: 0,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let deltas = counts.deltas(&counts);
        assert!(deltas.is_empty());
    }

    #[test]
    fn deltas_all_categories_changed() {
        let baseline = CheckCounts {
            total_issues: 0,
            unused_files: 0,
            unused_exports: 0,
            unused_types: 0,
            unused_dependencies: 0,
            unused_dev_dependencies: 0,
            unused_optional_dependencies: 0,
            unused_enum_members: 0,
            unused_class_members: 0,
            unresolved_imports: 0,
            unlisted_dependencies: 0,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let current = CheckCounts {
            total_issues: 14,
            unused_files: 1,
            unused_exports: 1,
            unused_types: 1,
            unused_dependencies: 1,
            unused_dev_dependencies: 1,
            unused_optional_dependencies: 1,
            unused_enum_members: 1,
            unused_class_members: 1,
            unresolved_imports: 1,
            unlisted_dependencies: 1,
            duplicate_exports: 1,
            circular_dependencies: 1,
            type_only_dependencies: 1,
            test_only_dependencies: 1,
        };
        let deltas = baseline.deltas(&current);
        // total_issues is not in deltas — only per-type fields
        assert_eq!(deltas.len(), 14);
        for (_, d) in &deltas {
            assert_eq!(*d, 1);
        }
    }

    #[test]
    fn deltas_mixed_increase_decrease() {
        let baseline = CheckCounts {
            total_issues: 10,
            unused_files: 5,
            unused_exports: 3,
            unused_types: 2,
            unused_dependencies: 0,
            unused_dev_dependencies: 0,
            unused_optional_dependencies: 0,
            unused_enum_members: 0,
            unused_class_members: 0,
            unresolved_imports: 0,
            unlisted_dependencies: 0,
            duplicate_exports: 0,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let current = CheckCounts {
            unused_files: 3,       // -2
            unused_exports: 5,     // +2
            unused_types: 0,       // -2
            unresolved_imports: 1, // +1
            ..baseline
        };
        let deltas = baseline.deltas(&current);
        assert_eq!(deltas.len(), 4);
        assert!(deltas.contains(&("unused_files", -2)));
        assert!(deltas.contains(&("unused_exports", 2)));
        assert!(deltas.contains(&("unused_types", -2)));
        assert!(deltas.contains(&("unresolved_imports", 1)));
    }

    // ── RegressionOutcome JSON with absolute tolerance ──────────────

    #[test]
    fn exceeded_outcome_json_absolute() {
        let outcome = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Absolute(2),
            type_deltas: vec![("unused_files", 5)],
        };
        let json = outcome.to_json();
        assert_eq!(json["status"], "exceeded");
        assert_eq!(json["tolerance_kind"], "absolute");
        assert_eq!(json["tolerance"], 2.0);
        assert_eq!(json["delta"], 5);
    }

    #[test]
    fn pass_outcome_json_with_improvement() {
        let outcome = RegressionOutcome::Pass {
            baseline_total: 10,
            current_total: 5,
        };
        let json = outcome.to_json();
        assert_eq!(json["status"], "pass");
        assert_eq!(json["delta"], -5);
        assert_eq!(json["exceeded"], false);
    }

    // ── DupesCounts serialization ──────────────────────────────────

    #[test]
    fn dupes_counts_roundtrip() {
        let dupes = DupesCounts {
            clone_groups: 8,
            duplication_percentage: 3.17,
        };
        let json = serde_json::to_string(&dupes).unwrap();
        let loaded: DupesCounts = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.clone_groups, 8);
        assert!((loaded.duplication_percentage - 3.17).abs() < f64::EPSILON);
    }

    #[test]
    fn dupes_counts_default_fields() {
        // Deserializing with missing fields should default to zero
        let json = "{}";
        let loaded: DupesCounts = serde_json::from_str(json).unwrap();
        assert_eq!(loaded.clone_groups, 0);
        assert!((loaded.duplication_percentage).abs() < f64::EPSILON);
    }

    // ── RegressionBaseline with missing optional sections ──────────

    #[test]
    fn baseline_without_check_section() {
        let baseline = RegressionBaseline {
            schema_version: 1,
            fallow_version: "2.4.0".into(),
            timestamp: "2026-03-27T10:00:00Z".into(),
            git_sha: None,
            check: None,
            dupes: Some(DupesCounts {
                clone_groups: 3,
                duplication_percentage: 1.0,
            }),
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        let loaded: RegressionBaseline = serde_json::from_str(&json).unwrap();
        assert!(loaded.check.is_none());
        assert!(loaded.dupes.is_some());
    }

    #[test]
    fn baseline_without_dupes_section() {
        let baseline = RegressionBaseline {
            schema_version: 1,
            fallow_version: "2.4.0".into(),
            timestamp: "2026-03-27T10:00:00Z".into(),
            git_sha: Some("deadbeef".into()),
            check: Some(CheckCounts {
                total_issues: 1,
                unused_files: 1,
                ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
            }),
            dupes: None,
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        let loaded: RegressionBaseline = serde_json::from_str(&json).unwrap();
        assert!(loaded.check.is_some());
        assert!(loaded.dupes.is_none());
        assert_eq!(loaded.git_sha.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn baseline_without_git_sha() {
        let baseline = RegressionBaseline {
            schema_version: 1,
            fallow_version: "2.4.0".into(),
            timestamp: "2026-03-27T10:00:00Z".into(),
            git_sha: None,
            check: None,
            dupes: None,
        };
        let json = serde_json::to_string_pretty(&baseline).unwrap();
        // git_sha should be skipped in serialization
        assert!(!json.contains("git_sha"));
        let loaded: RegressionBaseline = serde_json::from_str(&json).unwrap();
        assert!(loaded.git_sha.is_none());
    }

    // ── Forward compatibility: extra fields are ignored ──────────────

    #[test]
    fn baseline_json_with_unknown_check_fields_deserializes() {
        let json = r#"{
            "schema_version": 1,
            "fallow_version": "3.0.0",
            "timestamp": "2026-03-27T10:00:00Z",
            "check": {
                "total_issues": 10,
                "unused_files": 2,
                "some_future_field": 99
            }
        }"#;
        // Should not fail — extra fields are ignored by serde default
        let loaded: Result<RegressionBaseline, _> = serde_json::from_str(json);
        // Note: serde doesn't deny unknown fields by default, so this should work
        assert!(loaded.is_ok());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.check.as_ref().unwrap().total_issues, 10);
    }

    // ── save/load roundtrip ────────────────────────────────────────

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("regression-baseline.json");
        let counts = CheckCounts {
            total_issues: 15,
            unused_files: 3,
            unused_exports: 5,
            unused_types: 2,
            unused_dependencies: 1,
            unused_dev_dependencies: 1,
            unused_optional_dependencies: 0,
            unused_enum_members: 1,
            unused_class_members: 0,
            unresolved_imports: 1,
            unlisted_dependencies: 0,
            duplicate_exports: 1,
            circular_dependencies: 0,
            type_only_dependencies: 0,
            test_only_dependencies: 0,
        };
        let dupes = DupesCounts {
            clone_groups: 4,
            duplication_percentage: 2.5,
        };

        save_regression_baseline(&path, dir.path(), Some(&counts), Some(&dupes)).unwrap();
        let loaded = load_regression_baseline(&path).unwrap();

        assert_eq!(loaded.schema_version, REGRESSION_SCHEMA_VERSION);
        let check = loaded.check.unwrap();
        assert_eq!(check.total_issues, 15);
        assert_eq!(check.unused_files, 3);
        assert_eq!(check.unused_exports, 5);
        assert_eq!(check.unused_types, 2);
        assert_eq!(check.unused_dependencies, 1);
        assert_eq!(check.unresolved_imports, 1);
        assert_eq!(check.duplicate_exports, 1);
        let dupes = loaded.dupes.unwrap();
        assert_eq!(dupes.clone_groups, 4);
        assert!((dupes.duplication_percentage - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn save_load_roundtrip_check_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("regression-baseline.json");
        let counts = CheckCounts {
            total_issues: 5,
            unused_files: 5,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };

        save_regression_baseline(&path, dir.path(), Some(&counts), None).unwrap();
        let loaded = load_regression_baseline(&path).unwrap();

        assert!(loaded.check.is_some());
        assert!(loaded.dupes.is_none());
        assert_eq!(loaded.check.unwrap().unused_files, 5);
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("dir").join("baseline.json");
        let counts = CheckCounts {
            total_issues: 1,
            unused_files: 1,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };

        save_regression_baseline(&path, dir.path(), Some(&counts), None).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = load_regression_baseline(Path::new("/tmp/nonexistent-baseline-12345.json"));
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json {{{").unwrap();
        let result = load_regression_baseline(&path);
        assert!(result.is_err());
    }

    // ── save_baseline_to_config ────────────────────────────────────

    #[test]
    fn save_baseline_to_json_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".fallowrc.json");
        std::fs::write(&config_path, r#"{"entry": ["src/main.ts"]}"#).unwrap();

        let counts = CheckCounts {
            total_issues: 7,
            unused_files: 3,
            unused_exports: 4,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };
        save_baseline_to_config(&config_path, &counts).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"regression\""));
        assert!(content.contains("\"totalIssues\": 7"));
        // Should still be valid JSON
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn save_baseline_to_toml_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("fallow.toml");
        std::fs::write(&config_path, "[rules]\nunused-files = \"warn\"\n").unwrap();

        let counts = CheckCounts {
            total_issues: 7,
            unused_files: 3,
            unused_exports: 4,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };
        save_baseline_to_config(&config_path, &counts).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[regression.baseline]"));
        assert!(content.contains("totalIssues = 7"));
        assert!(content.contains("[rules]"));
    }

    #[test]
    fn save_baseline_to_nonexistent_json_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".fallowrc.json");
        // File doesn't exist — should create it from scratch

        let counts = CheckCounts {
            total_issues: 1,
            unused_files: 1,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };
        save_baseline_to_config(&config_path, &counts).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("\"regression\""));
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }

    #[test]
    fn save_baseline_to_nonexistent_toml_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("fallow.toml");

        let counts = CheckCounts {
            total_issues: 0,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };
        save_baseline_to_config(&config_path, &counts).unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[regression.baseline]"));
        assert!(content.contains("totalIssues = 0"));
    }

    // ── update_json_regression edge cases ──────────────────────────

    #[test]
    fn json_insert_with_trailing_comma() {
        let config = r#"{
  "entry": ["src/main.ts"],
}"#;
        // Trailing comma — our insertion should still produce reasonable output
        let result = update_json_regression(config, &sample_baseline()).unwrap();
        assert!(result.contains("\"regression\""));
    }

    #[test]
    fn json_no_closing_brace_returns_error() {
        let result = update_json_regression("", &sample_baseline());
        assert!(result.is_err());
    }

    #[test]
    fn json_nested_regression_object_replaced_correctly() {
        let config = r#"{
  "regression": {
    "baseline": {
      "totalIssues": 99,
      "unusedFiles": 10
    },
    "tolerance": "5%"
  },
  "entry": ["src/main.ts"]
}"#;
        let result = update_json_regression(config, &sample_baseline()).unwrap();
        assert!(!result.contains("99"));
        assert!(result.contains("\"totalIssues\": 5"));
        assert!(result.contains("\"entry\""));
    }

    // ── update_toml_regression edge cases ──────────────────────────

    #[test]
    fn toml_content_without_trailing_newline() {
        let config = "[rules]\nunused-files = \"warn\"";
        let result = update_toml_regression(config, &sample_baseline());
        assert!(result.contains("[regression.baseline]"));
        assert!(result.contains("[rules]"));
    }

    #[test]
    fn toml_replace_section_not_at_end() {
        let config = "[regression.baseline]\ntotalIssues = 99\nunusedFiles = 10\n\n[rules]\nunused-files = \"warn\"\n";
        let result = update_toml_regression(config, &sample_baseline());
        assert!(!result.contains("99"));
        assert!(result.contains("totalIssues = 5"));
        assert!(result.contains("[rules]"));
        assert!(result.contains("unused-files = \"warn\""));
    }

    #[test]
    fn toml_replace_section_at_end() {
        let config =
            "[rules]\nunused-files = \"warn\"\n\n[regression.baseline]\ntotalIssues = 99\n";
        let result = update_toml_regression(config, &sample_baseline());
        assert!(!result.contains("99"));
        assert!(result.contains("totalIssues = 5"));
        assert!(result.contains("[rules]"));
    }

    // ── find_json_key edge cases ────────────────────────────────────

    #[test]
    fn find_json_key_multiple_same_keys() {
        // Returns the first occurrence
        let content = r#"{"foo": 1, "bar": {"foo": 2}}"#;
        let pos = find_json_key(content, "foo").unwrap();
        assert_eq!(pos, 1);
    }

    #[test]
    fn find_json_key_in_nested_comment_then_real() {
        let content = "{\n  // \"entry\": old\n  /* \"entry\": also old */\n  \"entry\": []\n}";
        let pos = find_json_key(content, "entry").unwrap();
        assert!(content[pos..].starts_with("\"entry\": []"));
    }

    // ── chrono_now ─────────────────────────────────────────────────

    #[test]
    fn chrono_now_format() {
        let ts = chrono_now();
        // Should be ISO 8601 format: YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20);
        assert!(ts.ends_with('Z'));
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
    }

    // ── print_regression_outcome ────────────────────────────────────

    #[test]
    fn print_pass_outcome_does_not_panic() {
        let outcome = RegressionOutcome::Pass {
            baseline_total: 10,
            current_total: 8,
        };
        // Just verify it doesn't panic — output goes to stderr
        print_regression_outcome(&outcome);
    }

    #[test]
    fn print_exceeded_outcome_does_not_panic() {
        let outcome = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Percentage(2.0),
            type_deltas: vec![("unused_files", 5), ("unused_exports", -2)],
        };
        print_regression_outcome(&outcome);
    }

    #[test]
    fn print_exceeded_outcome_absolute_does_not_panic() {
        let outcome = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Absolute(2),
            type_deltas: vec![("unused_files", 3), ("unresolved_imports", 2)],
        };
        print_regression_outcome(&outcome);
    }

    #[test]
    fn print_skipped_outcome_does_not_panic() {
        let outcome = RegressionOutcome::Skipped {
            reason: "test reason",
        };
        print_regression_outcome(&outcome);
    }

    #[test]
    fn print_exceeded_with_empty_deltas_does_not_panic() {
        let outcome = RegressionOutcome::Exceeded {
            baseline_total: 10,
            current_total: 15,
            tolerance: Tolerance::Absolute(0),
            type_deltas: vec![],
        };
        print_regression_outcome(&outcome);
    }

    // ── compare_check_regression ────────────────────────────────────

    fn make_opts(
        fail: bool,
        tolerance: Tolerance,
        scoped: bool,
        baseline_file: Option<&Path>,
    ) -> RegressionOpts<'_> {
        RegressionOpts {
            fail_on_regression: fail,
            tolerance,
            regression_baseline_file: baseline_file,
            save_target: SaveRegressionTarget::None,
            scoped,
            quiet: true,
        }
    }

    #[test]
    fn compare_returns_none_when_disabled() {
        let results = AnalysisResults::default();
        let opts = make_opts(false, Tolerance::Absolute(0), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 5,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        assert!(outcome.is_none());
    }

    #[test]
    fn compare_returns_skipped_when_scoped() {
        let results = AnalysisResults::default();
        let opts = make_opts(true, Tolerance::Absolute(0), true, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 5,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        assert!(matches!(outcome, Some(RegressionOutcome::Skipped { .. })));
    }

    #[test]
    fn compare_pass_with_config_baseline() {
        let results = AnalysisResults::default(); // 0 issues
        let opts = make_opts(true, Tolerance::Absolute(0), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 0,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        match outcome {
            Some(RegressionOutcome::Pass {
                baseline_total,
                current_total,
            }) => {
                assert_eq!(baseline_total, 0);
                assert_eq!(current_total, 0);
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn compare_exceeded_with_config_baseline() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("b.ts"),
        });
        let opts = make_opts(true, Tolerance::Absolute(0), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 0,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        match outcome {
            Some(RegressionOutcome::Exceeded {
                baseline_total,
                current_total,
                ..
            }) => {
                assert_eq!(baseline_total, 0);
                assert_eq!(current_total, 2);
            }
            other => panic!("expected Exceeded, got {other:?}"),
        }
    }

    #[test]
    fn compare_pass_within_tolerance() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        let opts = make_opts(true, Tolerance::Absolute(5), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 0,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        assert!(matches!(outcome, Some(RegressionOutcome::Pass { .. })));
    }

    #[test]
    fn compare_improvement_is_pass() {
        // Current has fewer issues than baseline
        let results = AnalysisResults::default(); // 0 issues
        let opts = make_opts(true, Tolerance::Absolute(0), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 10,
            unused_files: 5,
            unused_exports: 5,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        match outcome {
            Some(RegressionOutcome::Pass {
                baseline_total,
                current_total,
            }) => {
                assert_eq!(baseline_total, 10);
                assert_eq!(current_total, 0);
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    #[test]
    fn compare_with_file_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");

        // Save a baseline to file
        let counts = CheckCounts {
            total_issues: 5,
            unused_files: 5,
            ..CheckCounts::from_config_baseline(&fallow_config::RegressionBaseline::default())
        };
        save_regression_baseline(&baseline_path, dir.path(), Some(&counts), None).unwrap();

        // Compare with empty results -> pass (improvement)
        let results = AnalysisResults::default();
        let opts = make_opts(true, Tolerance::Absolute(0), false, Some(&baseline_path));
        let outcome = compare_check_regression(&results, &opts, None).unwrap();
        assert!(matches!(outcome, Some(RegressionOutcome::Pass { .. })));
    }

    #[test]
    fn compare_file_baseline_missing_check_data_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");

        // Save a baseline with no check data (dupes only)
        save_regression_baseline(
            &baseline_path,
            dir.path(),
            None,
            Some(&DupesCounts {
                clone_groups: 1,
                duplication_percentage: 1.0,
            }),
        )
        .unwrap();

        let results = AnalysisResults::default();
        let opts = make_opts(true, Tolerance::Absolute(0), false, Some(&baseline_path));
        let outcome = compare_check_regression(&results, &opts, None);
        assert!(outcome.is_err());
    }

    #[test]
    fn compare_no_baseline_source_returns_error() {
        let results = AnalysisResults::default();
        let opts = make_opts(true, Tolerance::Absolute(0), false, None);
        let outcome = compare_check_regression(&results, &opts, None);
        assert!(outcome.is_err());
    }

    #[test]
    fn compare_exceeded_includes_type_deltas() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("b.ts"),
        });
        results.unused_exports.push(UnusedExport {
            path: PathBuf::from("c.ts"),
            export_name: "foo".into(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });

        let opts = make_opts(true, Tolerance::Absolute(0), false, None);
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 0,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();

        match outcome {
            Some(RegressionOutcome::Exceeded { type_deltas, .. }) => {
                assert!(type_deltas.contains(&("unused_files", 2)));
                assert!(type_deltas.contains(&("unused_exports", 1)));
            }
            other => panic!("expected Exceeded, got {other:?}"),
        }
    }

    #[test]
    fn compare_with_percentage_tolerance() {
        let mut results = AnalysisResults::default();
        // Add 1 issue
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });

        let opts = make_opts(true, Tolerance::Percentage(50.0), false, None);
        // baseline=10, 50% of 10 = 5, delta=1-10=-9 (improvement, should pass)
        // Wait, total_issues in config is the baseline for comparison.
        // results has 1 issue, baseline has 10 -> improvement -> pass
        let config_baseline = fallow_config::RegressionBaseline {
            total_issues: 10,
            unused_files: 10,
            ..Default::default()
        };
        let outcome = compare_check_regression(&results, &opts, Some(&config_baseline)).unwrap();
        assert!(matches!(outcome, Some(RegressionOutcome::Pass { .. })));
    }
}
