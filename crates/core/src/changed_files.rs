//! Git-aware "changed files" filtering shared between fallow-cli and fallow-lsp.
//!
//! Provides:
//! - [`validate_git_ref`] for input validation at trust boundaries.
//! - [`ChangedFilesError`] / [`try_get_changed_files`] / [`get_changed_files`]
//!   for resolving a git ref into the set of changed files.
//! - [`filter_results_by_changed_files`] for narrowing an [`AnalysisResults`]
//!   to issues in those files.
//! - [`filter_duplication_by_changed_files`] for narrowing a
//!   [`DuplicationReport`] to clone groups touching at least one changed file.
//!
//! Both filters intentionally exclude dependency-level issues (unused deps,
//! type-only deps, test-only deps) since "unused dependency" is a function of
//! the entire import graph and can't be attributed to individual changed files.

use std::path::{Path, PathBuf};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::duplicates::{DuplicationReport, DuplicationStats, families};
use crate::results::AnalysisResults;

/// Validate a user-supplied git ref before passing it to `git diff`.
///
/// Rejects empty strings, refs starting with `-` (which `git` would interpret
/// as an option flag), and characters outside the safe allowlist for branch
/// names, tags, SHAs, and reflog expressions (`HEAD~N`, `HEAD@{...}`).
///
/// Inside `@{...}` braces, colons and spaces are allowed so reflog timestamps
/// like `HEAD@{2025-01-01}` and `HEAD@{1 week ago}` round-trip.
///
/// Used by both the CLI (clap value parser) and the LSP (initializationOptions
/// trust boundary) to fail fast with a readable error rather than handing a
/// malformed ref to git.
pub fn validate_git_ref(s: &str) -> Result<&str, String> {
    if s.is_empty() {
        return Err("git ref cannot be empty".to_string());
    }
    if s.starts_with('-') {
        return Err("git ref cannot start with '-'".to_string());
    }
    let mut in_braces = false;
    for c in s.chars() {
        match c {
            '{' => in_braces = true,
            '}' => in_braces = false,
            ':' | ' ' if in_braces => {}
            c if c.is_ascii_alphanumeric()
                || matches!(c, '.' | '_' | '-' | '/' | '~' | '^' | '@' | '{' | '}') => {}
            _ => return Err(format!("git ref contains disallowed character: '{c}'")),
        }
    }
    if in_braces {
        return Err("git ref has unclosed '{'".to_string());
    }
    Ok(s)
}

/// Classification of a `git diff` failure, so callers can pick their own
/// wording (soft warning vs hard error) without re-parsing stderr.
#[derive(Debug)]
pub enum ChangedFilesError {
    /// Git ref failed validation before invoking `git`.
    InvalidRef(String),
    /// `git` binary not found / not executable.
    GitMissing(String),
    /// Command ran but the directory isn't a git repository.
    NotARepository,
    /// Command ran but the ref is invalid / another git error.
    GitFailed(String),
}

impl ChangedFilesError {
    /// Human-readable clause suitable for embedding in an error message.
    /// Does not include the flag name (e.g. "--changed-since") so callers can
    /// prepend their own context.
    pub fn describe(&self) -> String {
        match self {
            Self::InvalidRef(e) => format!("invalid git ref: {e}"),
            Self::GitMissing(e) => format!("failed to run git: {e}"),
            Self::NotARepository => "not a git repository".to_owned(),
            Self::GitFailed(stderr) => augment_git_failed(stderr),
        }
    }
}

/// Enrich a raw `git diff` stderr with actionable hints when the failure mode
/// is recognizable. Today: shallow-clone misses (`actions/checkout@v4` defaults
/// to `fetch-depth: 1`, GitLab CI to `GIT_DEPTH: 50`), where the baseline ref
/// predates the fetch boundary. Bare git stderr is famously cryptic; a hint
/// here is much more useful than a docs link the reader has to chase.
fn augment_git_failed(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("not a valid object name")
        || lower.contains("unknown revision")
        || lower.contains("ambiguous argument")
    {
        format!(
            "{stderr} (shallow clone? try `git fetch --unshallow`, or set `fetch-depth: 0` on actions/checkout / `GIT_DEPTH: 0` in GitLab CI)"
        )
    } else {
        stderr.to_owned()
    }
}

fn collect_git_paths(root: &Path, args: &[&str]) -> Result<FxHashSet<PathBuf>, ChangedFilesError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| ChangedFilesError::GitMissing(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(if stderr.contains("not a git repository") {
            ChangedFilesError::NotARepository
        } else {
            ChangedFilesError::GitFailed(stderr.trim().to_owned())
        });
    }

    let files: FxHashSet<PathBuf> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| root.join(line))
        .collect();

    Ok(files)
}

/// Get files changed since a git ref. Returns `Err` (with details) when the
/// git invocation itself failed, so callers can choose between warn-and-ignore
/// and hard-error behavior.
///
/// Includes both:
/// - committed changes from the merge-base range `git_ref...HEAD`
/// - tracked staged/unstaged changes from `HEAD` to the current worktree
/// - untracked files not ignored by Git
///
/// This keeps `--changed-since` useful for local validation instead of only
/// reflecting the last committed `HEAD`.
pub fn try_get_changed_files(
    root: &Path,
    git_ref: &str,
) -> Result<FxHashSet<PathBuf>, ChangedFilesError> {
    validate_git_ref(git_ref).map_err(ChangedFilesError::InvalidRef)?;

    let mut files = collect_git_paths(
        root,
        &[
            "diff",
            "--name-only",
            "--end-of-options",
            &format!("{git_ref}...HEAD"),
        ],
    )?;
    files.extend(collect_git_paths(root, &["diff", "--name-only", "HEAD"])?);
    files.extend(collect_git_paths(
        root,
        &["ls-files", "--others", "--exclude-standard"],
    )?);
    Ok(files)
}

/// Get files changed since a git ref. Returns `None` on git failure after
/// printing a warning to stderr. Used by `--changed-since` and `--file`, where
/// a failure falls back to full-scope analysis.
#[expect(
    clippy::print_stderr,
    reason = "intentional user-facing warning for the CLI's --changed-since fallback path; LSP callers use try_get_changed_files instead"
)]
pub fn get_changed_files(root: &Path, git_ref: &str) -> Option<FxHashSet<PathBuf>> {
    match try_get_changed_files(root, git_ref) {
        Ok(files) => Some(files),
        Err(ChangedFilesError::InvalidRef(e)) => {
            eprintln!("Warning: --changed-since ignored: invalid git ref: {e}");
            None
        }
        Err(ChangedFilesError::GitMissing(e)) => {
            eprintln!("Warning: --changed-since ignored: failed to run git: {e}");
            None
        }
        Err(ChangedFilesError::NotARepository) => {
            eprintln!("Warning: --changed-since ignored: not a git repository");
            None
        }
        Err(ChangedFilesError::GitFailed(stderr)) => {
            eprintln!("Warning: --changed-since failed for ref '{git_ref}': {stderr}");
            None
        }
    }
}

/// Filter `results` to only include issues whose source file is in
/// `changed_files`.
///
/// Dependency-level issues (unused deps, dev deps, optional deps, type-only
/// deps, test-only deps) are intentionally NOT filtered here. Unlike
/// file-level issues, a dependency being "unused" is a function of the entire
/// import graph and can't be attributed to individual changed source files.
#[expect(
    clippy::implicit_hasher,
    reason = "fallow standardizes on FxHashSet across the workspace"
)]
pub fn filter_results_by_changed_files(
    results: &mut AnalysisResults,
    changed_files: &FxHashSet<PathBuf>,
) {
    results
        .unused_files
        .retain(|f| changed_files.contains(&f.path));
    results
        .unused_exports
        .retain(|e| changed_files.contains(&e.path));
    results
        .unused_types
        .retain(|e| changed_files.contains(&e.path));
    results
        .unused_enum_members
        .retain(|m| changed_files.contains(&m.path));
    results
        .unused_class_members
        .retain(|m| changed_files.contains(&m.path));
    results
        .unresolved_imports
        .retain(|i| changed_files.contains(&i.path));

    // Unlisted deps: keep only if any importing file is changed
    results.unlisted_dependencies.retain(|d| {
        d.imported_from
            .iter()
            .any(|s| changed_files.contains(&s.path))
    });

    // Duplicate exports: filter locations to changed files, drop groups with < 2
    for dup in &mut results.duplicate_exports {
        dup.locations
            .retain(|loc| changed_files.contains(&loc.path));
    }
    results.duplicate_exports.retain(|d| d.locations.len() >= 2);

    // Circular deps: keep cycles where at least one file is changed
    results
        .circular_dependencies
        .retain(|c| c.files.iter().any(|f| changed_files.contains(f)));

    // Boundary violations: keep if the importing file changed
    results
        .boundary_violations
        .retain(|v| changed_files.contains(&v.from_path));

    // Stale suppressions: keep if the file changed
    results
        .stale_suppressions
        .retain(|s| changed_files.contains(&s.path));
}

/// Recompute duplication statistics after filtering.
///
/// Uses per-file line deduplication (matching `compute_stats` in
/// `duplicates/detect.rs`) so overlapping clone instances don't inflate the
/// duplicated line count.
fn recompute_duplication_stats(report: &DuplicationReport) -> DuplicationStats {
    let mut files_with_clones: FxHashSet<&Path> = FxHashSet::default();
    let mut file_dup_lines: FxHashMap<&Path, FxHashSet<usize>> = FxHashMap::default();
    let mut duplicated_tokens = 0_usize;
    let mut clone_instances = 0_usize;

    for group in &report.clone_groups {
        for instance in &group.instances {
            files_with_clones.insert(&instance.file);
            clone_instances += 1;
            let lines = file_dup_lines.entry(&instance.file).or_default();
            for line in instance.start_line..=instance.end_line {
                lines.insert(line);
            }
        }
        duplicated_tokens += group.token_count * group.instances.len();
    }

    let duplicated_lines: usize = file_dup_lines.values().map(FxHashSet::len).sum();

    DuplicationStats {
        total_files: report.stats.total_files,
        files_with_clones: files_with_clones.len(),
        total_lines: report.stats.total_lines,
        duplicated_lines,
        total_tokens: report.stats.total_tokens,
        duplicated_tokens,
        clone_groups: report.clone_groups.len(),
        clone_instances,
        #[expect(
            clippy::cast_precision_loss,
            reason = "stat percentages are display-only; precision loss at usize::MAX line counts is acceptable"
        )]
        duplication_percentage: if report.stats.total_lines > 0 {
            (duplicated_lines as f64 / report.stats.total_lines as f64) * 100.0
        } else {
            0.0
        },
    }
}

/// Filter a duplication report to only retain clone groups where at least one
/// instance belongs to a changed file. Families, mirrored directories, and
/// stats are rebuilt from the surviving groups so consumers see consistent,
/// correctly-scoped numbers.
#[expect(
    clippy::implicit_hasher,
    reason = "fallow standardizes on FxHashSet across the workspace"
)]
pub fn filter_duplication_by_changed_files(
    report: &mut DuplicationReport,
    changed_files: &FxHashSet<PathBuf>,
    root: &Path,
) {
    report
        .clone_groups
        .retain(|g| g.instances.iter().any(|i| changed_files.contains(&i.file)));
    report.clone_families = families::group_into_families(&report.clone_groups, root);
    report.mirrored_directories =
        families::detect_mirrored_directories(&report.clone_families, root);
    report.stats = recompute_duplication_stats(report);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicates::{CloneGroup, CloneInstance};
    use crate::results::{BoundaryViolation, CircularDependency, UnusedExport, UnusedFile};

    #[test]
    fn changed_files_error_describe_variants() {
        assert!(
            ChangedFilesError::InvalidRef("bad".to_owned())
                .describe()
                .contains("invalid git ref")
        );
        assert!(
            ChangedFilesError::GitMissing("oops".to_owned())
                .describe()
                .contains("oops")
        );
        assert_eq!(
            ChangedFilesError::NotARepository.describe(),
            "not a git repository"
        );
        assert!(
            ChangedFilesError::GitFailed("bad ref".to_owned())
                .describe()
                .contains("bad ref")
        );
    }

    #[test]
    fn augment_git_failed_appends_shallow_clone_hint_for_unknown_revision() {
        let stderr = "fatal: ambiguous argument 'fallow-baseline...HEAD': unknown revision or path not in the working tree.";
        let described = ChangedFilesError::GitFailed(stderr.to_owned()).describe();
        assert!(described.contains(stderr), "original stderr preserved");
        assert!(
            described.contains("shallow clone"),
            "hint surfaced: {described}"
        );
        assert!(
            described.contains("fetch-depth: 0") || described.contains("git fetch --unshallow"),
            "hint actionable: {described}"
        );
    }

    #[test]
    fn augment_git_failed_passthrough_for_other_errors() {
        // Errors that aren't shallow-clone-related stay verbatim
        let stderr = "fatal: refusing to merge unrelated histories";
        let described = ChangedFilesError::GitFailed(stderr.to_owned()).describe();
        assert_eq!(described, stderr);
    }

    #[test]
    fn validate_git_ref_rejects_leading_dash() {
        assert!(validate_git_ref("--upload-pack=evil").is_err());
        assert!(validate_git_ref("-flag").is_err());
    }

    #[test]
    fn validate_git_ref_accepts_baseline_tag() {
        assert_eq!(
            validate_git_ref("fallow-baseline").unwrap(),
            "fallow-baseline"
        );
    }

    #[test]
    fn try_get_changed_files_rejects_invalid_ref() {
        // Validation runs before git invocation, so any path will do
        let err = try_get_changed_files(Path::new("/"), "--evil")
            .expect_err("leading-dash ref must be rejected");
        assert!(matches!(err, ChangedFilesError::InvalidRef(_)));
        assert!(err.describe().contains("cannot start with"));
    }

    #[test]
    fn validate_git_ref_rejects_option_like_ref() {
        assert!(validate_git_ref("--output=/tmp/fallow-proof").is_err());
    }

    #[test]
    fn validate_git_ref_allows_reflog_relative_date() {
        assert!(validate_git_ref("HEAD@{1 week ago}").is_ok());
    }

    #[test]
    fn try_get_changed_files_rejects_option_like_ref_before_git() {
        let root = tempfile::tempdir().expect("create temp dir");
        let proof_path = root.path().join("proof");

        let result = try_get_changed_files(
            root.path(),
            &format!("--output={}", proof_path.to_string_lossy()),
        );

        assert!(matches!(result, Err(ChangedFilesError::InvalidRef(_))));
        assert!(
            !proof_path.exists(),
            "invalid changedSince ref must not be passed through to git as an option"
        );
    }

    #[test]
    fn filter_results_keeps_only_changed_files() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: "/a.ts".into(),
        });
        results.unused_files.push(UnusedFile {
            path: "/b.ts".into(),
        });
        results.unused_exports.push(UnusedExport {
            path: "/a.ts".into(),
            export_name: "foo".into(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });

        let mut changed: FxHashSet<PathBuf> = FxHashSet::default();
        changed.insert("/a.ts".into());

        filter_results_by_changed_files(&mut results, &changed);

        assert_eq!(results.unused_files.len(), 1);
        assert_eq!(results.unused_files[0].path, PathBuf::from("/a.ts"));
        assert_eq!(results.unused_exports.len(), 1);
    }

    #[test]
    fn filter_results_preserves_dependency_level_issues() {
        let mut results = AnalysisResults::default();
        results
            .unused_dependencies
            .push(crate::results::UnusedDependency {
                package_name: "lodash".into(),
                location: crate::results::DependencyLocation::Dependencies,
                path: "/pkg.json".into(),
                line: 3,
            });

        let changed: FxHashSet<PathBuf> = FxHashSet::default();
        filter_results_by_changed_files(&mut results, &changed);

        // Dependency-level issues survive even when no source files changed
        assert_eq!(results.unused_dependencies.len(), 1);
    }

    #[test]
    fn filter_results_keeps_circular_dep_when_any_file_changed() {
        let mut results = AnalysisResults::default();
        results.circular_dependencies.push(CircularDependency {
            files: vec!["/a.ts".into(), "/b.ts".into()],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        });

        let mut changed: FxHashSet<PathBuf> = FxHashSet::default();
        changed.insert("/b.ts".into());

        filter_results_by_changed_files(&mut results, &changed);
        assert_eq!(results.circular_dependencies.len(), 1);
    }

    #[test]
    fn filter_results_drops_circular_dep_when_no_file_changed() {
        let mut results = AnalysisResults::default();
        results.circular_dependencies.push(CircularDependency {
            files: vec!["/a.ts".into(), "/b.ts".into()],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        });

        let changed: FxHashSet<PathBuf> = FxHashSet::default();
        filter_results_by_changed_files(&mut results, &changed);
        assert!(results.circular_dependencies.is_empty());
    }

    #[test]
    fn filter_results_drops_boundary_violation_when_importer_unchanged() {
        let mut results = AnalysisResults::default();
        results.boundary_violations.push(BoundaryViolation {
            from_path: "/a.ts".into(),
            to_path: "/b.ts".into(),
            from_zone: "ui".into(),
            to_zone: "data".into(),
            import_specifier: "../data/db".into(),
            line: 1,
            col: 0,
        });

        let mut changed: FxHashSet<PathBuf> = FxHashSet::default();
        // only the imported file changed, not the importer
        changed.insert("/b.ts".into());

        filter_results_by_changed_files(&mut results, &changed);
        assert!(results.boundary_violations.is_empty());
    }

    #[test]
    fn filter_duplication_keeps_groups_with_at_least_one_changed_instance() {
        let mut report = DuplicationReport {
            clone_groups: vec![CloneGroup {
                instances: vec![
                    CloneInstance {
                        file: "/a.ts".into(),
                        start_line: 1,
                        end_line: 5,
                        start_col: 0,
                        end_col: 10,
                        fragment: "code".into(),
                    },
                    CloneInstance {
                        file: "/b.ts".into(),
                        start_line: 1,
                        end_line: 5,
                        start_col: 0,
                        end_col: 10,
                        fragment: "code".into(),
                    },
                ],
                token_count: 20,
                line_count: 5,
            }],
            clone_families: vec![],
            mirrored_directories: vec![],
            stats: DuplicationStats {
                total_files: 2,
                files_with_clones: 2,
                total_lines: 100,
                duplicated_lines: 10,
                total_tokens: 200,
                duplicated_tokens: 40,
                clone_groups: 1,
                clone_instances: 2,
                duplication_percentage: 10.0,
            },
        };

        let mut changed: FxHashSet<PathBuf> = FxHashSet::default();
        changed.insert("/a.ts".into());

        filter_duplication_by_changed_files(&mut report, &changed, Path::new(""));
        assert_eq!(report.clone_groups.len(), 1);
        // stats recomputed from surviving groups
        assert_eq!(report.stats.clone_groups, 1);
        assert_eq!(report.stats.clone_instances, 2);
    }

    #[test]
    fn filter_duplication_drops_groups_with_no_changed_instance() {
        let mut report = DuplicationReport {
            clone_groups: vec![CloneGroup {
                instances: vec![CloneInstance {
                    file: "/a.ts".into(),
                    start_line: 1,
                    end_line: 5,
                    start_col: 0,
                    end_col: 10,
                    fragment: "code".into(),
                }],
                token_count: 20,
                line_count: 5,
            }],
            clone_families: vec![],
            mirrored_directories: vec![],
            stats: DuplicationStats {
                total_files: 1,
                files_with_clones: 1,
                total_lines: 100,
                duplicated_lines: 5,
                total_tokens: 100,
                duplicated_tokens: 20,
                clone_groups: 1,
                clone_instances: 1,
                duplication_percentage: 5.0,
            },
        };

        let changed: FxHashSet<PathBuf> = FxHashSet::default();
        filter_duplication_by_changed_files(&mut report, &changed, Path::new(""));
        assert!(report.clone_groups.is_empty());
        assert_eq!(report.stats.clone_groups, 0);
        assert_eq!(report.stats.clone_instances, 0);
        assert!((report.stats.duplication_percentage - 0.0).abs() < f64::EPSILON);
    }
}
