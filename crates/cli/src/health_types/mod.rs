//! Health / complexity analysis report types.
//!
//! Separated from the `health` command module so that report formatters
//! (which are compiled as part of both the lib and bin targets) can
//! reference these types without pulling in binary-only dependencies.

mod coverage;
mod scores;
mod targets;
mod trends;
mod vital_signs;

pub use coverage::*;
pub use scores::*;
pub use targets::*;
pub use trends::*;
pub use vital_signs::*;

/// Result of complexity analysis for reporting.
#[derive(Debug, serde::Serialize)]
pub struct HealthReport {
    /// Functions exceeding thresholds.
    pub findings: Vec<HealthFinding>,
    /// Summary statistics.
    pub summary: HealthSummary,
    /// Project-wide vital signs (always computed from available data).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vital_signs: Option<VitalSigns>,
    /// Project-wide health score (only populated with `--score`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_score: Option<HealthScore>,
    /// Per-file health scores (only populated with `--file-scores` or `--hotspots`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_scores: Vec<FileHealthScore>,
    /// Static coverage gaps (only populated with `--coverage-gaps`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage_gaps: Option<CoverageGaps>,
    /// Hotspot entries (only populated with `--hotspots`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hotspots: Vec<HotspotEntry>,
    /// Hotspot analysis summary (only set with `--hotspots`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hotspot_summary: Option<HotspotSummary>,
    /// Ranked refactoring recommendations (only populated with `--targets`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<RefactoringTarget>,
    /// Adaptive thresholds used for target scoring (only set with `--targets`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_thresholds: Option<TargetThresholds>,
    /// Health trend comparison against a previous snapshot (only set with `--trend`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health_trend: Option<HealthTrend>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_report_skips_empty_collections() {
        let report = HealthReport {
            findings: vec![],
            summary: HealthSummary {
                files_analyzed: 0,
                functions_analyzed: 0,
                functions_above_threshold: 0,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
                coverage_model: None,
                istanbul_matched: None,
                istanbul_total: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        // Empty vecs should be omitted due to skip_serializing_if
        assert!(!json.contains("file_scores"));
        assert!(!json.contains("hotspots"));
        assert!(!json.contains("hotspot_summary"));
        assert!(!json.contains("targets"));
        assert!(!json.contains("vital_signs"));
        assert!(!json.contains("health_score"));
    }

    #[test]
    fn health_score_none_skipped_in_report() {
        let report = HealthReport {
            findings: vec![],
            summary: HealthSummary {
                files_analyzed: 0,
                functions_analyzed: 0,
                functions_above_threshold: 0,
                max_cyclomatic_threshold: 20,
                max_cognitive_threshold: 15,
                files_scored: None,
                average_maintainability: None,
                coverage_model: None,
                istanbul_matched: None,
                istanbul_total: None,
            },
            vital_signs: None,
            health_score: None,
            file_scores: vec![],
            coverage_gaps: None,
            hotspots: vec![],
            hotspot_summary: None,
            targets: vec![],
            target_thresholds: None,
            health_trend: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("health_score"));
    }
}
