use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageSummary {
    pub functions_total: usize,
    pub functions_called: usize,
    pub functions_never_called: usize,
    pub functions_coverage_unavailable: usize,
    pub percent_dead_in_production: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageVerdict {
    Clean,
    HotPathChangesNeeded,
    ColdCodeDetected,
    LicenseExpiredGrace,
    #[default]
    Unknown,
}

impl ProductionCoverageVerdict {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::HotPathChangesNeeded => "hot-path-changes-needed",
            Self::ColdCodeDetected => "cold-code-detected",
            Self::LicenseExpiredGrace => "license-expired-grace",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageState {
    Called,
    NeverCalled,
    CoverageUnavailable,
    Unknown,
}

impl ProductionCoverageState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Called => "called",
            Self::NeverCalled => "never-called",
            Self::CoverageUnavailable => "coverage-unavailable",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

impl ProductionCoverageConfidence {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductionCoverageWatermark {
    TrialExpired,
    LicenseExpiredGrace,
    Unknown,
}

impl ProductionCoverageWatermark {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TrialExpired => "trial-expired",
            Self::LicenseExpiredGrace => "license-expired-grace",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ProductionCoverageWatermark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageAction {
    /// Stable action identifier. Serialized as `type` in JSON to match the
    /// `actions[].type` contract shared with every other `fallow health` finding.
    #[serde(rename = "type")]
    pub kind: String,
    pub description: String,
    pub auto_fixable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageMessage {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageFinding {
    pub path: PathBuf,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub state: ProductionCoverageState,
    pub invocations: u64,
    pub confidence: ProductionCoverageConfidence,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ProductionCoverageAction>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductionCoverageHotPath {
    pub path: PathBuf,
    pub function: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub invocations: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ProductionCoverageAction>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ProductionCoverageReport {
    pub verdict: ProductionCoverageVerdict,
    pub summary: ProductionCoverageSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<ProductionCoverageFinding>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hot_paths: Vec<ProductionCoverageHotPath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watermark: Option<ProductionCoverageWatermark>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ProductionCoverageMessage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_display_matches_kebab_case_serde() {
        assert_eq!(ProductionCoverageVerdict::Clean.to_string(), "clean");
        assert_eq!(
            ProductionCoverageVerdict::HotPathChangesNeeded.to_string(),
            "hot-path-changes-needed",
        );
        assert_eq!(
            ProductionCoverageVerdict::ColdCodeDetected.to_string(),
            "cold-code-detected",
        );
        assert_eq!(
            ProductionCoverageVerdict::LicenseExpiredGrace.to_string(),
            "license-expired-grace",
        );
        assert_eq!(ProductionCoverageVerdict::Unknown.to_string(), "unknown");
    }

    #[test]
    fn state_display_matches_kebab_case_serde() {
        assert_eq!(ProductionCoverageState::Called.to_string(), "called");
        assert_eq!(
            ProductionCoverageState::NeverCalled.to_string(),
            "never-called",
        );
        assert_eq!(
            ProductionCoverageState::CoverageUnavailable.to_string(),
            "coverage-unavailable",
        );
    }

    #[test]
    fn confidence_display_matches_kebab_case_serde() {
        assert_eq!(ProductionCoverageConfidence::High.to_string(), "high");
        assert_eq!(ProductionCoverageConfidence::Medium.to_string(), "medium");
        assert_eq!(ProductionCoverageConfidence::Low.to_string(), "low");
        assert_eq!(ProductionCoverageConfidence::Unknown.to_string(), "unknown");
    }

    #[test]
    fn watermark_display_matches_kebab_case_serde() {
        assert_eq!(
            ProductionCoverageWatermark::TrialExpired.to_string(),
            "trial-expired",
        );
        assert_eq!(
            ProductionCoverageWatermark::LicenseExpiredGrace.to_string(),
            "license-expired-grace",
        );
    }

    #[test]
    fn action_serializes_kind_as_type() {
        let action = ProductionCoverageAction {
            kind: "review-deletion".to_owned(),
            description: "Remove the function.".to_owned(),
            auto_fixable: false,
        };
        let value = serde_json::to_value(&action).expect("action should serialize");
        assert_eq!(value["type"], "review-deletion");
        assert!(
            value.get("kind").is_none(),
            "kind should be renamed to type"
        );
    }
}
