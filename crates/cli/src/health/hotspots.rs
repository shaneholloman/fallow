use crate::error::emit_error;
use crate::health_types::{FileHealthScore, HotspotEntry, HotspotSummary};

use super::HealthOptions;

/// Validate git prerequisites and return churn data for hotspot analysis.
///
/// Returns `None` (with an error printed) if the repo is invalid, `--since` is
/// malformed, or git analysis fails.
fn fetch_churn_data(
    opts: &HealthOptions<'_>,
) -> Option<(
    fallow_core::churn::ChurnResult,
    fallow_core::churn::SinceDuration,
)> {
    use fallow_core::churn;

    if !churn::is_git_repo(opts.root) {
        let _ = emit_error("hotspot analysis requires a git repository", 2, opts.output);
        return None;
    }

    let since_input = opts.since.unwrap_or("6m");
    if let Err(e) = crate::validate::validate_no_control_chars(since_input, "--since") {
        let _ = emit_error(&e, 2, opts.output);
        return None;
    }
    let since = match churn::parse_since(since_input) {
        Ok(s) => s,
        Err(e) => {
            let _ = emit_error(&format!("invalid --since: {e}"), 2, opts.output);
            return None;
        }
    };

    let churn_result = churn::analyze_churn(opts.root, &since)?;
    Some((churn_result, since))
}

/// Find the maximum weighted-commits and complexity-density across eligible files.
///
/// Used to normalize hotspot scores into the 0-100 range.
pub(super) fn compute_normalization_maxima(
    file_scores: &[FileHealthScore],
    churn_files: &rustc_hash::FxHashMap<std::path::PathBuf, fallow_core::churn::FileChurn>,
    min_commits: u32,
) -> (f64, f64) {
    let mut max_weighted: f64 = 0.0;
    let mut max_density: f64 = 0.0;
    for score in file_scores {
        if let Some(churn) = churn_files.get(&score.path)
            && churn.commits >= min_commits
        {
            max_weighted = max_weighted.max(churn.weighted_commits);
            max_density = max_density.max(score.complexity_density);
        }
    }
    (max_weighted, max_density)
}

/// Check whether a file should be excluded from hotspot results
/// based on workspace filter and ignore patterns.
pub(super) fn is_excluded_from_hotspots(
    path: &std::path::Path,
    root: &std::path::Path,
    ignore_set: &globset::GlobSet,
    ws_root: Option<&std::path::Path>,
) -> bool {
    if let Some(ws) = ws_root
        && !path.starts_with(ws)
    {
        return true;
    }
    if !ignore_set.is_empty() {
        let relative = path.strip_prefix(root).unwrap_or(path);
        if ignore_set.is_match(relative) {
            return true;
        }
    }
    false
}

/// Compute a normalized hotspot score from churn and complexity data.
///
/// Both inputs are normalized against their respective maxima so the result
/// falls in the 0-100 range (rounded to one decimal).
pub(super) fn compute_hotspot_score(
    weighted_commits: f64,
    max_weighted: f64,
    complexity_density: f64,
    max_density: f64,
) -> f64 {
    let norm_churn = if max_weighted > 0.0 {
        weighted_commits / max_weighted
    } else {
        0.0
    };
    let norm_complexity = if max_density > 0.0 {
        complexity_density / max_density
    } else {
        0.0
    };
    (norm_churn * norm_complexity * 100.0 * 10.0).round() / 10.0
}

/// Compute hotspot entries by combining git churn data with file health scores.
pub(super) fn compute_hotspots(
    opts: &HealthOptions<'_>,
    config: &fallow_config::ResolvedConfig,
    file_scores: &[FileHealthScore],
    ignore_set: &globset::GlobSet,
    ws_root: Option<&std::path::Path>,
) -> (Vec<HotspotEntry>, Option<HotspotSummary>) {
    let Some((churn_result, since)) = fetch_churn_data(opts) else {
        return (Vec::new(), None);
    };

    // Warn about shallow clones (read from churn result to avoid redundant git call)
    let shallow_clone = churn_result.shallow_clone;
    if shallow_clone && !opts.quiet {
        eprintln!(
            "Warning: shallow clone detected. Hotspot analysis may be incomplete. \
             Use `git fetch --unshallow` for full history."
        );
    }

    let min_commits = opts.min_commits.unwrap_or(3);
    let (max_weighted, max_density) =
        compute_normalization_maxima(file_scores, &churn_result.files, min_commits);

    // Build hotspot entries
    let mut hotspot_entries = Vec::new();
    let mut files_excluded: usize = 0;

    for score in file_scores {
        if is_excluded_from_hotspots(&score.path, &config.root, ignore_set, ws_root) {
            continue;
        }

        let Some(churn) = churn_result.files.get(&score.path) else {
            continue;
        };
        if churn.commits < min_commits {
            files_excluded += 1;
            continue;
        }

        hotspot_entries.push(HotspotEntry {
            path: score.path.clone(),
            score: compute_hotspot_score(
                churn.weighted_commits,
                max_weighted,
                score.complexity_density,
                max_density,
            ),
            commits: churn.commits,
            weighted_commits: churn.weighted_commits,
            lines_added: churn.lines_added,
            lines_deleted: churn.lines_deleted,
            complexity_density: score.complexity_density,
            fan_in: score.fan_in,
            trend: churn.trend,
        });
    }

    // Sort by score descending (highest risk first)
    hotspot_entries.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute summary BEFORE --top truncation
    let files_analyzed = hotspot_entries.len();
    let summary = HotspotSummary {
        since: since.display,
        min_commits,
        files_analyzed,
        files_excluded,
        shallow_clone,
    };

    // Apply --top to hotspots
    if let Some(top) = opts.top {
        hotspot_entries.truncate(top);
    }

    (hotspot_entries, Some(summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- compute_hotspot_score ---

    #[test]
    fn hotspot_score_both_maxima_zero() {
        // When both maxima are zero, avoid division by zero -> score 0
        assert!((compute_hotspot_score(0.0, 0.0, 0.0, 0.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_max_weighted_zero() {
        // Churn dimension zero, complexity present -> score 0
        assert!((compute_hotspot_score(5.0, 0.0, 0.5, 1.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_max_density_zero() {
        // Complexity dimension zero, churn present -> score 0
        assert!((compute_hotspot_score(5.0, 10.0, 0.0, 0.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_equal_normalization() {
        // File equals both maxima -> normalized values both 1.0 -> score 100
        let score = compute_hotspot_score(10.0, 10.0, 2.0, 2.0);
        assert!((score - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_half_values() {
        // Half of each maximum -> 0.5 * 0.5 * 100 = 25.0
        let score = compute_hotspot_score(5.0, 10.0, 1.0, 2.0);
        assert!((score - 25.0).abs() < f64::EPSILON);
    }

    // --- is_excluded_from_hotspots ---

    #[test]
    fn excluded_no_filters() {
        let path = std::path::Path::new("/project/src/foo.ts");
        let root = std::path::Path::new("/project");
        let ignore_set = globset::GlobSet::empty();

        assert!(!is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_workspace_filter_mismatch() {
        let path = std::path::Path::new("/project/packages/b/src/foo.ts");
        let root = std::path::Path::new("/project");
        let ws_root = std::path::Path::new("/project/packages/a");
        let ignore_set = globset::GlobSet::empty();

        assert!(is_excluded_from_hotspots(
            path,
            root,
            &ignore_set,
            Some(ws_root)
        ));
    }

    #[test]
    fn excluded_workspace_filter_match() {
        let path = std::path::Path::new("/project/packages/a/src/foo.ts");
        let root = std::path::Path::new("/project");
        let ws_root = std::path::Path::new("/project/packages/a");
        let ignore_set = globset::GlobSet::empty();

        assert!(!is_excluded_from_hotspots(
            path,
            root,
            &ignore_set,
            Some(ws_root)
        ));
    }

    #[test]
    fn excluded_matching_glob() {
        let path = std::path::Path::new("/project/src/generated/types.ts");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("src/generated/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_non_matching_glob() {
        let path = std::path::Path::new("/project/src/components/Button.tsx");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("src/generated/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(!is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    // --- compute_normalization_maxima ---

    #[test]
    fn normalization_maxima_empty_input() {
        let scores: Vec<FileHealthScore> = vec![];
        let churn_files = rustc_hash::FxHashMap::default();

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w).abs() < f64::EPSILON);
        assert!((max_d).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_single_file() {
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/foo.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 0.75,
            maintainability_index: 80.0,
            total_cyclomatic: 15,
            total_cognitive: 10,
            function_count: 3,
            lines: 20,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/foo.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/foo.ts"),
                commits: 5,
                weighted_commits: 4.2,
                lines_added: 100,
                lines_deleted: 20,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w - 4.2).abs() < f64::EPSILON);
        assert!((max_d - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_below_min_commits() {
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/foo.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 0.75,
            maintainability_index: 80.0,
            total_cyclomatic: 15,
            total_cognitive: 10,
            function_count: 3,
            lines: 20,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/foo.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/foo.ts"),
                commits: 2, // below min_commits=3
                weighted_commits: 4.2,
                lines_added: 100,
                lines_deleted: 20,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );

        // File has only 2 commits, below min_commits=3 -> excluded
        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w).abs() < f64::EPSILON);
        assert!((max_d).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_all_zeros() {
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/foo.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 0.0,
            maintainability_index: 100.0,
            total_cyclomatic: 0,
            total_cognitive: 0,
            function_count: 1,
            lines: 10,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/foo.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/foo.ts"),
                commits: 5,
                weighted_commits: 0.0,
                lines_added: 0,
                lines_deleted: 0,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w).abs() < f64::EPSILON);
        assert!((max_d).abs() < f64::EPSILON);
    }

    // --- compute_hotspot_score: additional edge cases ---

    #[test]
    fn hotspot_score_high_churn_low_complexity() {
        // File at maximum churn but only 10% complexity -> 10.0
        let score = compute_hotspot_score(10.0, 10.0, 0.1, 1.0);
        assert!((score - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_low_churn_high_complexity() {
        // File at 10% churn but maximum complexity -> 10.0
        let score = compute_hotspot_score(1.0, 10.0, 2.0, 2.0);
        assert!((score - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_rounding() {
        // 0.33 * 0.33 * 100 = 10.89 -> should round to one decimal
        let score = compute_hotspot_score(1.0, 3.0, 1.0, 3.0);
        // 1/3 * 1/3 * 100 = 11.111... -> rounded to 11.1
        assert!((score - 11.1).abs() < f64::EPSILON);
    }

    #[test]
    fn hotspot_score_very_small_values() {
        // Both values are tiny fractions of their maxima
        let score = compute_hotspot_score(0.01, 100.0, 0.001, 10.0);
        // 0.0001 * 0.0001 * 100 = 0.001 -> rounds to 0.0
        assert!((score).abs() < 0.1);
    }

    #[test]
    fn hotspot_score_weighted_exceeds_max() {
        // Edge case: weighted_commits > max_weighted (shouldn't happen but be robust)
        let score = compute_hotspot_score(15.0, 10.0, 1.0, 2.0);
        // 1.5 * 0.5 * 100 = 75.0
        assert!((score - 75.0).abs() < f64::EPSILON);
    }

    // --- compute_normalization_maxima: additional edge cases ---

    #[test]
    fn normalization_maxima_multiple_files_picks_max() {
        let scores = vec![
            FileHealthScore {
                path: std::path::PathBuf::from("/src/a.ts"),
                fan_in: 0,
                fan_out: 0,
                dead_code_ratio: 0.0,
                complexity_density: 0.5,
                maintainability_index: 80.0,
                total_cyclomatic: 10,
                total_cognitive: 5,
                function_count: 2,
                lines: 50,
                crap_max: 0.0,
                crap_above_threshold: 0,
            },
            FileHealthScore {
                path: std::path::PathBuf::from("/src/b.ts"),
                fan_in: 0,
                fan_out: 0,
                dead_code_ratio: 0.0,
                complexity_density: 1.2, // highest density
                maintainability_index: 60.0,
                total_cyclomatic: 30,
                total_cognitive: 20,
                function_count: 5,
                lines: 100,
                crap_max: 0.0,
                crap_above_threshold: 0,
            },
            FileHealthScore {
                path: std::path::PathBuf::from("/src/c.ts"),
                fan_in: 0,
                fan_out: 0,
                dead_code_ratio: 0.0,
                complexity_density: 0.8,
                maintainability_index: 70.0,
                total_cyclomatic: 20,
                total_cognitive: 15,
                function_count: 4,
                lines: 80,
                crap_max: 0.0,
                crap_above_threshold: 0,
            },
        ];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/a.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/a.ts"),
                commits: 5,
                weighted_commits: 3.0,
                lines_added: 50,
                lines_deleted: 10,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );
        churn_files.insert(
            std::path::PathBuf::from("/src/b.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/b.ts"),
                commits: 10,
                weighted_commits: 8.5, // highest weighted
                lines_added: 200,
                lines_deleted: 50,
                trend: fallow_core::churn::ChurnTrend::Accelerating,
            },
        );
        churn_files.insert(
            std::path::PathBuf::from("/src/c.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/c.ts"),
                commits: 7,
                weighted_commits: 5.0,
                lines_added: 100,
                lines_deleted: 30,
                trend: fallow_core::churn::ChurnTrend::Cooling,
            },
        );

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w - 8.5).abs() < f64::EPSILON);
        assert!((max_d - 1.2).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_mixed_above_and_below_threshold() {
        // Two files: one above min_commits, one below.
        // Only the above-threshold file should contribute.
        let scores = vec![
            FileHealthScore {
                path: std::path::PathBuf::from("/src/frequent.ts"),
                fan_in: 0,
                fan_out: 0,
                dead_code_ratio: 0.0,
                complexity_density: 0.4,
                maintainability_index: 85.0,
                total_cyclomatic: 8,
                total_cognitive: 4,
                function_count: 2,
                lines: 40,
                crap_max: 0.0,
                crap_above_threshold: 0,
            },
            FileHealthScore {
                path: std::path::PathBuf::from("/src/rare.ts"),
                fan_in: 0,
                fan_out: 0,
                dead_code_ratio: 0.0,
                complexity_density: 2.0, // higher but excluded
                maintainability_index: 50.0,
                total_cyclomatic: 40,
                total_cognitive: 30,
                function_count: 8,
                lines: 200,
                crap_max: 0.0,
                crap_above_threshold: 0,
            },
        ];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/frequent.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/frequent.ts"),
                commits: 10,
                weighted_commits: 7.0,
                lines_added: 150,
                lines_deleted: 40,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );
        churn_files.insert(
            std::path::PathBuf::from("/src/rare.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/rare.ts"),
                commits: 1, // below min_commits=5
                weighted_commits: 0.9,
                lines_added: 10,
                lines_deleted: 2,
                trend: fallow_core::churn::ChurnTrend::Cooling,
            },
        );

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 5);
        // Only frequent.ts qualifies
        assert!((max_w - 7.0).abs() < f64::EPSILON);
        assert!((max_d - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_file_score_without_churn() {
        // File exists in scores but has no churn data -> ignored
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/no_churn.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 5.0,
            maintainability_index: 30.0,
            total_cyclomatic: 100,
            total_cognitive: 80,
            function_count: 20,
            lines: 500,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let churn_files: rustc_hash::FxHashMap<std::path::PathBuf, fallow_core::churn::FileChurn> =
            rustc_hash::FxHashMap::default();

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 1);
        assert!((max_w).abs() < f64::EPSILON);
        assert!((max_d).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_min_commits_zero() {
        // min_commits=0 means every file qualifies
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/foo.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 0.3,
            maintainability_index: 90.0,
            total_cyclomatic: 3,
            total_cognitive: 2,
            function_count: 1,
            lines: 10,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/foo.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/foo.ts"),
                commits: 0,
                weighted_commits: 0.0,
                lines_added: 0,
                lines_deleted: 0,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );

        // commits=0 >= min_commits=0, so file is included
        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 0);
        assert!((max_w).abs() < f64::EPSILON);
        assert!((max_d - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn normalization_maxima_exactly_at_threshold() {
        // File has exactly min_commits -> should be included
        let scores = vec![FileHealthScore {
            path: std::path::PathBuf::from("/src/foo.ts"),
            fan_in: 0,
            fan_out: 0,
            dead_code_ratio: 0.0,
            complexity_density: 1.5,
            maintainability_index: 65.0,
            total_cyclomatic: 25,
            total_cognitive: 18,
            function_count: 5,
            lines: 120,
            crap_max: 0.0,
            crap_above_threshold: 0,
        }];
        let mut churn_files: rustc_hash::FxHashMap<
            std::path::PathBuf,
            fallow_core::churn::FileChurn,
        > = rustc_hash::FxHashMap::default();
        churn_files.insert(
            std::path::PathBuf::from("/src/foo.ts"),
            fallow_core::churn::FileChurn {
                path: std::path::PathBuf::from("/src/foo.ts"),
                commits: 3, // exactly at min_commits=3
                weighted_commits: 2.8,
                lines_added: 60,
                lines_deleted: 15,
                trend: fallow_core::churn::ChurnTrend::Stable,
            },
        );

        let (max_w, max_d) = compute_normalization_maxima(&scores, &churn_files, 3);
        assert!((max_w - 2.8).abs() < f64::EPSILON);
        assert!((max_d - 1.5).abs() < f64::EPSILON);
    }

    // --- is_excluded_from_hotspots: additional edge cases ---

    #[test]
    fn excluded_workspace_and_glob_combined() {
        // File matches workspace but also matches ignore glob -> excluded
        let path = std::path::Path::new("/project/packages/a/src/generated/types.ts");
        let root = std::path::Path::new("/project");
        let ws_root = std::path::Path::new("/project/packages/a");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("**/generated/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(is_excluded_from_hotspots(
            path,
            root,
            &ignore_set,
            Some(ws_root)
        ));
    }

    #[test]
    fn excluded_workspace_match_but_glob_no_match() {
        // File is in workspace and doesn't match ignore -> not excluded
        let path = std::path::Path::new("/project/packages/a/src/index.ts");
        let root = std::path::Path::new("/project");
        let ws_root = std::path::Path::new("/project/packages/a");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("**/generated/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(!is_excluded_from_hotspots(
            path,
            root,
            &ignore_set,
            Some(ws_root)
        ));
    }

    #[test]
    fn excluded_path_equals_root() {
        // Path is the root itself (edge case for strip_prefix)
        let path = std::path::Path::new("/project");
        let root = std::path::Path::new("/project");
        let ignore_set = globset::GlobSet::empty();

        assert!(!is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_path_outside_root() {
        // Path not under root -> strip_prefix falls back to full path
        let path = std::path::Path::new("/other/src/foo.ts");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        // Glob matches relative path, but strip_prefix fails so full path is used
        builder.add(globset::Glob::new("src/foo.ts").unwrap());
        let ignore_set = builder.build().unwrap();

        // strip_prefix fails -> uses full path "/other/src/foo.ts"
        // which doesn't match "src/foo.ts"
        assert!(!is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_multiple_globs_first_matches() {
        let path = std::path::Path::new("/project/dist/bundle.js");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("dist/**").unwrap());
        builder.add(globset::Glob::new("node_modules/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_multiple_globs_second_matches() {
        let path = std::path::Path::new("/project/node_modules/lodash/index.js");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("dist/**").unwrap());
        builder.add(globset::Glob::new("node_modules/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(is_excluded_from_hotspots(path, root, &ignore_set, None));
    }

    #[test]
    fn excluded_multiple_globs_none_matches() {
        let path = std::path::Path::new("/project/src/app.ts");
        let root = std::path::Path::new("/project");
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("dist/**").unwrap());
        builder.add(globset::Glob::new("node_modules/**").unwrap());
        let ignore_set = builder.build().unwrap();

        assert!(!is_excluded_from_hotspots(path, root, &ignore_set, None));
    }
}
