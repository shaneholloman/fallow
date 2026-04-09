//! Score types, grade boundaries, file health metrics, and findings.

/// Hotspot score threshold for counting a file as a hotspot in vital signs.
pub const HOTSPOT_SCORE_THRESHOLD: f64 = 50.0;

/// Cognitive complexity threshold above which a function is flagged for extraction.
pub const COGNITIVE_EXTRACTION_THRESHOLD: u16 = 30;

/// Project-level health score: a single 0–100 number with letter grade.
///
/// ## Score Formula
///
/// ```text
/// score = 100
///   - min(dead_file_pct × 0.2, 15)
///   - min(dead_export_pct × 0.2, 15)
///   - min(max(0, avg_cyclomatic − 1.5) × 5, 20)
///   - min(max(0, p90_cyclomatic − 10), 10)
///   - min(max(0, 70 − maintainability_avg) × 0.5, 15)
///   - min(hotspot_count / total_files × 200, 10)
///   - min(unused_dep_count, 10)
///   - min(circular_dep_count, 10)
///   - min(max(0, very_high_risk_pct − 5) × 0.5, 10)    [unit size]
///   - min(max(0, p95_fan_in − 30) × 0.25, 5)            [coupling]
/// ```
///
/// Missing metrics (from pipelines that didn't run) don't penalize — run
/// `--score` (which forces full pipeline) for the most accurate result.
///
/// ## Letter Grades
///
/// A: score ≥ 85, B: 70–84, C: 55–69, D: 40–54, F: below 40.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthScore {
    /// Overall score (0–100, higher is better).
    pub score: f64,
    /// Letter grade: A, B, C, D, or F.
    pub grade: &'static str,
    /// Per-component penalty breakdown. Shows what drove the score down.
    pub penalties: HealthScorePenalties,
}

/// Per-component penalty breakdown for the health score.
///
/// Each field shows how many points were subtracted for that component.
/// `None` means the metric was not available (pipeline didn't run).
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthScorePenalties {
    /// Points lost from dead files (max 15).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dead_files: Option<f64>,
    /// Points lost from dead exports (max 15).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dead_exports: Option<f64>,
    /// Points lost from average cyclomatic complexity (max 20).
    pub complexity: f64,
    /// Points lost from p90 cyclomatic complexity (max 10).
    pub p90_complexity: f64,
    /// Points lost from low maintainability index (max 15).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintainability: Option<f64>,
    /// Points lost from hotspot files (max 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspots: Option<f64>,
    /// Points lost from unused dependencies (max 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unused_deps: Option<f64>,
    /// Points lost from circular dependencies (max 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circular_deps: Option<f64>,
    /// Points lost from oversized functions (max 10).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_size: Option<f64>,
    /// Points lost from coupling concentration (max 5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coupling: Option<f64>,
}

/// Map a numeric score (0–100) to a letter grade.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "score is 0-100, fits in u32"
)]
pub const fn letter_grade(score: f64) -> &'static str {
    // Truncate to u32 so that 84.9 maps to B and 85.0 maps to A —
    // fractional digits don't affect the grade bucket.
    let s = score as u32;
    if s >= 85 {
        "A"
    } else if s >= 70 {
        "B"
    } else if s >= 55 {
        "C"
    } else if s >= 40 {
        "D"
    } else {
        "F"
    }
}

/// A single function that exceeds a complexity threshold.
#[derive(Debug, serde::Serialize)]
pub struct HealthFinding {
    /// Absolute file path.
    pub path: std::path::PathBuf,
    /// Function name.
    pub name: String,
    /// 1-based line number.
    pub line: u32,
    /// 0-based column.
    pub col: u32,
    /// Cyclomatic complexity.
    pub cyclomatic: u16,
    /// Cognitive complexity.
    pub cognitive: u16,
    /// Number of lines in the function.
    pub line_count: u32,
    /// Number of parameters.
    pub param_count: u8,
    /// Which threshold was exceeded.
    pub exceeded: ExceededThreshold,
}

/// Which complexity threshold was exceeded.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceededThreshold {
    /// Only cyclomatic exceeded.
    Cyclomatic,
    /// Only cognitive exceeded.
    Cognitive,
    /// Both thresholds exceeded.
    Both,
}

/// Summary statistics for the health report.
#[derive(Debug, serde::Serialize)]
pub struct HealthSummary {
    /// Number of files analyzed.
    pub files_analyzed: usize,
    /// Total number of functions found.
    pub functions_analyzed: usize,
    /// Number of functions above threshold.
    pub functions_above_threshold: usize,
    /// Configured cyclomatic threshold.
    pub max_cyclomatic_threshold: u16,
    /// Configured cognitive threshold.
    pub max_cognitive_threshold: u16,
    /// Number of files scored (only set with `--file-scores`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_scored: Option<usize>,
    /// Average maintainability index across all scored files (only set with `--file-scores`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_maintainability: Option<f64>,
    /// Coverage model used for CRAP computation (None when file scores not computed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_model: Option<CoverageModel>,
    /// Number of functions matched against Istanbul coverage data.
    /// Only present when `coverage_model` is `istanbul`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub istanbul_matched: Option<usize>,
    /// Total functions that could potentially be matched.
    /// Only present when `coverage_model` is `istanbul`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub istanbul_total: Option<usize>,
}

/// Per-file health score combining complexity, coupling, and dead code metrics.
///
/// Files with zero functions (barrel files, re-export files) are excluded by default.
///
/// ## Maintainability Index Formula
///
/// ```text
/// fan_out_penalty = min(ln(fan_out + 1) × 4, 15)
/// maintainability = 100
///     - (complexity_density × 30)
///     - (dead_code_ratio × 20)
///     - fan_out_penalty
/// ```
///
/// Clamped to \[0, 100\]. Higher is better.
///
/// - **complexity_density**: total cyclomatic complexity / lines of code
/// - **dead_code_ratio**: fraction of value exports (excluding type-only exports) with zero references (0.0–1.0)
/// - **fan_out_penalty**: logarithmic scaling with cap at 15 points; reflects diminishing marginal risk of additional imports
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileHealthScore {
    /// File path (absolute; stripped to relative in output).
    pub path: std::path::PathBuf,
    /// Number of files that import this file.
    pub fan_in: usize,
    /// Number of files this file imports.
    pub fan_out: usize,
    /// Fraction of value exports with zero references (0.0–1.0). Files with no value exports get 0.0.
    /// Type-only exports (interfaces, type aliases) are excluded from both numerator and denominator
    /// to avoid inflating the ratio for well-typed codebases that export props types alongside components.
    pub dead_code_ratio: f64,
    /// Total cyclomatic complexity / lines of code.
    pub complexity_density: f64,
    /// Weighted composite score (0–100, higher is better).
    pub maintainability_index: f64,
    /// Sum of cyclomatic complexity across all functions.
    pub total_cyclomatic: u32,
    /// Sum of cognitive complexity across all functions.
    pub total_cognitive: u32,
    /// Number of functions in this file.
    pub function_count: usize,
    /// Total lines of code (from line_offsets).
    pub lines: u32,
    /// Maximum CRAP score among functions in this file.
    /// Static binary model: test-reachable file = CC, untested = CC^2 + CC.
    pub crap_max: f64,
    /// Count of functions with CRAP >= 30 (CC >= 5 without test path).
    pub crap_above_threshold: usize,
}

/// Coverage model used for CRAP score computation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageModel {
    /// Binary model: test-reachable = CC, untested = CC^2 + CC.
    /// Superseded by `StaticEstimated`; retained for serialization compatibility.
    #[allow(
        dead_code,
        reason = "retained for backwards-compatible JSON deserialization"
    )]
    StaticBinary,
    /// Graph-based estimation: per-function coverage derived from export
    /// reference analysis. Directly test-referenced = 85%, indirectly
    /// test-reachable = 40%, untested = 0%. Default model.
    StaticEstimated,
    /// Istanbul-format coverage data: real per-function statement coverage
    /// from Jest, Vitest, c8, nyc, or any Istanbul-compatible tool.
    /// CRAP = CC^2 * (1 - cov/100)^3 + CC.
    Istanbul,
}

/// A hotspot: a file that is both complex and frequently changing.
///
/// ## Score Formula
///
/// ```text
/// normalized_churn = weighted_commits / max_weighted_commits   (0..1)
/// normalized_complexity = complexity_density / max_density      (0..1)
/// score = normalized_churn × normalized_complexity × 100       (0..100)
/// ```
///
/// Score uses within-project max normalization. Higher score = higher risk.
/// Fan-in is shown separately as "blast radius" — not baked into the score.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HotspotEntry {
    /// File path (absolute; stripped to relative in output).
    pub path: std::path::PathBuf,
    /// Hotspot score (0–100). Higher means more risk.
    pub score: f64,
    /// Number of commits in the analysis window.
    pub commits: u32,
    /// Recency-weighted commit count (exponential decay, half-life 90 days).
    pub weighted_commits: f64,
    /// Total lines added across all commits.
    pub lines_added: u32,
    /// Total lines deleted across all commits.
    pub lines_deleted: u32,
    /// Cyclomatic complexity / lines of code.
    pub complexity_density: f64,
    /// Number of files that import this file (blast radius).
    pub fan_in: usize,
    /// Churn trend: accelerating, stable, or cooling.
    pub trend: fallow_core::churn::ChurnTrend,
}

/// Summary statistics for hotspot analysis.
#[derive(Debug, serde::Serialize)]
pub struct HotspotSummary {
    /// Analysis window display string (e.g., "6 months").
    pub since: String,
    /// Minimum commits threshold.
    pub min_commits: u32,
    /// Number of files with churn data meeting the threshold.
    pub files_analyzed: usize,
    /// Number of files excluded (below min_commits).
    pub files_excluded: usize,
    /// Whether the repository is a shallow clone.
    pub shallow_clone: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exceeded_threshold_serializes_as_snake_case() {
        let json = serde_json::to_string(&ExceededThreshold::Both).unwrap();
        assert_eq!(json, r#""both""#);

        let json = serde_json::to_string(&ExceededThreshold::Cyclomatic).unwrap();
        assert_eq!(json, r#""cyclomatic""#);
    }

    #[test]
    fn exceeded_threshold_all_variants_serialize() {
        for variant in [
            ExceededThreshold::Cyclomatic,
            ExceededThreshold::Cognitive,
            ExceededThreshold::Both,
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert!(!json.is_empty());
        }
    }

    #[test]
    fn letter_grade_boundaries() {
        assert_eq!(letter_grade(100.0), "A");
        assert_eq!(letter_grade(85.0), "A");
        assert_eq!(letter_grade(84.9), "B");
        assert_eq!(letter_grade(70.0), "B");
        assert_eq!(letter_grade(69.9), "C");
        assert_eq!(letter_grade(55.0), "C");
        assert_eq!(letter_grade(54.9), "D");
        assert_eq!(letter_grade(40.0), "D");
        assert_eq!(letter_grade(39.9), "F");
        assert_eq!(letter_grade(0.0), "F");
    }

    #[test]
    fn hotspot_score_threshold_is_50() {
        assert!((HOTSPOT_SCORE_THRESHOLD - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn health_score_serializes_correctly() {
        let score = HealthScore {
            score: 78.5,
            grade: "B",
            penalties: HealthScorePenalties {
                dead_files: Some(3.1),
                dead_exports: Some(6.0),
                complexity: 0.0,
                p90_complexity: 0.0,
                maintainability: None,
                hotspots: None,
                unused_deps: Some(5.0),
                circular_deps: Some(4.0),
                unit_size: None,
                coupling: None,
            },
        };
        let json = serde_json::to_string(&score).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["score"], 78.5);
        assert_eq!(parsed["grade"], "B");
        assert_eq!(parsed["penalties"]["dead_files"], 3.1);
        // None fields should be absent
        assert!(!json.contains("maintainability"));
        assert!(!json.contains("hotspots"));
    }

    #[test]
    fn coverage_model_serializes_as_snake_case() {
        let json = serde_json::to_string(&CoverageModel::StaticBinary).unwrap();
        assert_eq!(json, r#""static_binary""#);

        let json = serde_json::to_string(&CoverageModel::StaticEstimated).unwrap();
        assert_eq!(json, r#""static_estimated""#);

        let json = serde_json::to_string(&CoverageModel::Istanbul).unwrap();
        assert_eq!(json, r#""istanbul""#);
    }
}
