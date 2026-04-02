mod boundaries;
mod duplicates_config;
mod format;
mod health;
mod parsing;
mod resolution;
mod rules;

pub use boundaries::{
    BoundaryConfig, BoundaryPreset, BoundaryRule, BoundaryZone, ResolvedBoundaryConfig,
    ResolvedBoundaryRule, ResolvedZone,
};
pub use duplicates_config::{
    DetectionMode, DuplicatesConfig, NormalizationConfig, ResolvedNormalization,
};
pub use format::OutputFormat;
pub use health::HealthConfig;
pub use resolution::{ConfigOverride, IgnoreExportRule, ResolvedConfig, ResolvedOverride};
pub use rules::{PartialRulesConfig, RulesConfig, Severity};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::external_plugin::ExternalPluginDef;
use crate::workspace::WorkspaceConfig;

/// User-facing configuration loaded from `.fallowrc.json` or `fallow.toml`.
///
/// # Examples
///
/// ```
/// use fallow_config::FallowConfig;
///
/// // Default config has sensible defaults
/// let config = FallowConfig::default();
/// assert!(config.entry.is_empty());
/// assert!(!config.production);
///
/// // Deserialize from JSON
/// let config: FallowConfig = serde_json::from_str(r#"{
///     "entry": ["src/main.ts"],
///     "production": true
/// }"#).unwrap();
/// assert_eq!(config.entry, vec!["src/main.ts"]);
/// assert!(config.production);
/// ```
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FallowConfig {
    /// JSON Schema reference (ignored during deserialization).
    #[serde(rename = "$schema", default, skip_serializing)]
    #[schemars(skip)]
    pub schema: Option<String>,

    /// Base config files to extend from.
    ///
    /// Supports two resolution strategies:
    /// - **Relative paths**: resolved relative to the config file containing the `extends`.
    /// - **npm packages**: prefixed with `npm:`, resolved by walking up `node_modules/`.
    ///   Package resolution checks `package.json` `exports`/`main` first, then falls back
    ///   to standard config file names. Subpaths are supported (e.g., `npm:@co/config/strict.json`).
    ///
    /// Base configs are loaded first, then this config's values override them.
    /// Later entries in the array override earlier ones.
    ///
    /// **Note:** `npm:` resolution uses `node_modules/` directory walk-up and is
    /// incompatible with Yarn Plug'n'Play (PnP), which has no `node_modules/`.
    #[serde(default, skip_serializing)]
    pub extends: Vec<String>,

    /// Additional entry point glob patterns.
    #[serde(default)]
    pub entry: Vec<String>,

    /// Glob patterns to ignore from analysis.
    #[serde(default)]
    pub ignore_patterns: Vec<String>,

    /// Custom framework definitions (inline plugin definitions).
    #[serde(default)]
    pub framework: Vec<ExternalPluginDef>,

    /// Workspace overrides.
    #[serde(default)]
    pub workspaces: Option<WorkspaceConfig>,

    /// Dependencies to ignore (always considered used).
    #[serde(default)]
    pub ignore_dependencies: Vec<String>,

    /// Export ignore rules.
    #[serde(default)]
    pub ignore_exports: Vec<IgnoreExportRule>,

    /// Duplication detection settings.
    #[serde(default)]
    pub duplicates: DuplicatesConfig,

    /// Complexity health metrics settings.
    #[serde(default)]
    pub health: HealthConfig,

    /// Per-issue-type severity rules.
    #[serde(default)]
    pub rules: RulesConfig,

    /// Architecture boundary enforcement configuration.
    #[serde(default)]
    pub boundaries: BoundaryConfig,

    /// Production mode: exclude test/dev files, only start/build scripts.
    #[serde(default)]
    pub production: bool,

    /// Paths to external plugin files or directories containing plugin files.
    ///
    /// Supports TOML, JSON, and JSONC formats.
    ///
    /// In addition to these explicit paths, fallow automatically discovers:
    /// - `*.toml`, `*.json`, `*.jsonc` files in `.fallow/plugins/`
    /// - `fallow-plugin-*.{toml,json,jsonc}` files in the project root
    #[serde(default)]
    pub plugins: Vec<String>,

    /// Per-file rule overrides matching oxlint's overrides pattern.
    #[serde(default)]
    pub overrides: Vec<ConfigOverride>,

    /// Regression detection baseline embedded in config.
    /// Stores issue counts from a known-good state for CI regression checks.
    /// Populated by `--save-regression-baseline` (no path), read by `--fail-on-regression`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regression: Option<RegressionConfig>,
}

/// Regression baseline counts, embedded in the config file.
///
/// When `--fail-on-regression` is used without `--regression-baseline <PATH>`,
/// fallow reads the baseline from this config section.
/// When `--save-regression-baseline` is used without a path argument,
/// fallow writes the baseline into the config file.
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegressionConfig {
    /// Dead code issue counts baseline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<RegressionBaseline>,
}

/// Per-type issue counts for regression comparison.
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegressionBaseline {
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
    #[serde(default)]
    pub boundary_violations: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Default trait ───────────────────────────────────────────────

    #[test]
    fn default_config_has_empty_collections() {
        let config = FallowConfig::default();
        assert!(config.schema.is_none());
        assert!(config.extends.is_empty());
        assert!(config.entry.is_empty());
        assert!(config.ignore_patterns.is_empty());
        assert!(config.framework.is_empty());
        assert!(config.workspaces.is_none());
        assert!(config.ignore_dependencies.is_empty());
        assert!(config.ignore_exports.is_empty());
        assert!(config.plugins.is_empty());
        assert!(config.overrides.is_empty());
        assert!(!config.production);
    }

    #[test]
    fn default_config_rules_are_error() {
        let config = FallowConfig::default();
        assert_eq!(config.rules.unused_files, Severity::Error);
        assert_eq!(config.rules.unused_exports, Severity::Error);
        assert_eq!(config.rules.unused_dependencies, Severity::Error);
    }

    #[test]
    fn default_config_duplicates_enabled() {
        let config = FallowConfig::default();
        assert!(config.duplicates.enabled);
        assert_eq!(config.duplicates.min_tokens, 50);
        assert_eq!(config.duplicates.min_lines, 5);
    }

    #[test]
    fn default_config_health_thresholds() {
        let config = FallowConfig::default();
        assert_eq!(config.health.max_cyclomatic, 20);
        assert_eq!(config.health.max_cognitive, 15);
    }

    // ── JSON deserialization ────────────────────────────────────────

    #[test]
    fn deserialize_empty_json_object() {
        let config: FallowConfig = serde_json::from_str("{}").unwrap();
        assert!(config.entry.is_empty());
        assert!(!config.production);
    }

    #[test]
    fn deserialize_json_with_all_top_level_fields() {
        let json = r#"{
            "$schema": "https://fallow.dev/schema.json",
            "entry": ["src/main.ts"],
            "ignorePatterns": ["generated/**"],
            "ignoreDependencies": ["postcss"],
            "production": true,
            "plugins": ["custom-plugin.toml"],
            "rules": {"unused-files": "warn"},
            "duplicates": {"enabled": false},
            "health": {"maxCyclomatic": 30}
        }"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.schema.as_deref(),
            Some("https://fallow.dev/schema.json")
        );
        assert_eq!(config.entry, vec!["src/main.ts"]);
        assert_eq!(config.ignore_patterns, vec!["generated/**"]);
        assert_eq!(config.ignore_dependencies, vec!["postcss"]);
        assert!(config.production);
        assert_eq!(config.plugins, vec!["custom-plugin.toml"]);
        assert_eq!(config.rules.unused_files, Severity::Warn);
        assert!(!config.duplicates.enabled);
        assert_eq!(config.health.max_cyclomatic, 30);
    }

    #[test]
    fn deserialize_json_deny_unknown_fields() {
        let json = r#"{"unknownField": true}"#;
        let result: Result<FallowConfig, _> = serde_json::from_str(json);
        assert!(result.is_err(), "unknown fields should be rejected");
    }

    #[test]
    fn deserialize_json_production_mode_default_false() {
        let config: FallowConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.production);
    }

    #[test]
    fn deserialize_json_production_mode_true() {
        let config: FallowConfig = serde_json::from_str(r#"{"production": true}"#).unwrap();
        assert!(config.production);
    }

    // ── TOML deserialization ────────────────────────────────────────

    #[test]
    fn deserialize_toml_minimal() {
        let toml_str = r#"
entry = ["src/index.ts"]
production = true
"#;
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.entry, vec!["src/index.ts"]);
        assert!(config.production);
    }

    #[test]
    fn deserialize_toml_with_inline_framework() {
        let toml_str = r#"
[[framework]]
name = "my-framework"
enablers = ["my-framework-pkg"]
entryPoints = ["src/routes/**/*.tsx"]
"#;
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.framework.len(), 1);
        assert_eq!(config.framework[0].name, "my-framework");
        assert_eq!(config.framework[0].enablers, vec!["my-framework-pkg"]);
        assert_eq!(
            config.framework[0].entry_points,
            vec!["src/routes/**/*.tsx"]
        );
    }

    #[test]
    fn deserialize_toml_with_workspace_config() {
        let toml_str = r#"
[workspaces]
patterns = ["packages/*", "apps/*"]
"#;
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        assert!(config.workspaces.is_some());
        let ws = config.workspaces.unwrap();
        assert_eq!(ws.patterns, vec!["packages/*", "apps/*"]);
    }

    #[test]
    fn deserialize_toml_with_ignore_exports() {
        let toml_str = r#"
[[ignoreExports]]
file = "src/types/**/*.ts"
exports = ["*"]
"#;
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.ignore_exports.len(), 1);
        assert_eq!(config.ignore_exports[0].file, "src/types/**/*.ts");
        assert_eq!(config.ignore_exports[0].exports, vec!["*"]);
    }

    #[test]
    fn deserialize_toml_deny_unknown_fields() {
        let toml_str = r"bogus_field = true";
        let result: Result<FallowConfig, _> = toml::from_str(toml_str);
        assert!(result.is_err(), "unknown fields should be rejected");
    }

    // ── Serialization roundtrip ─────────────────────────────────────

    #[test]
    fn json_serialize_roundtrip() {
        let config = FallowConfig {
            entry: vec!["src/main.ts".to_string()],
            production: true,
            ..FallowConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: FallowConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.entry, vec!["src/main.ts"]);
        assert!(restored.production);
    }

    #[test]
    fn schema_field_not_serialized() {
        let config = FallowConfig {
            schema: Some("https://example.com/schema.json".to_string()),
            ..FallowConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        // $schema has skip_serializing, should not appear in output
        assert!(
            !json.contains("$schema"),
            "schema field should be skipped in serialization"
        );
    }

    #[test]
    fn extends_field_not_serialized() {
        let config = FallowConfig {
            extends: vec!["base.json".to_string()],
            ..FallowConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("extends"),
            "extends field should be skipped in serialization"
        );
    }

    // ── RegressionConfig / RegressionBaseline ──────────────────────

    #[test]
    fn regression_config_deserialize_json() {
        let json = r#"{
            "regression": {
                "baseline": {
                    "totalIssues": 42,
                    "unusedFiles": 10,
                    "unusedExports": 5,
                    "circularDependencies": 2
                }
            }
        }"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        let regression = config.regression.unwrap();
        let baseline = regression.baseline.unwrap();
        assert_eq!(baseline.total_issues, 42);
        assert_eq!(baseline.unused_files, 10);
        assert_eq!(baseline.unused_exports, 5);
        assert_eq!(baseline.circular_dependencies, 2);
        // Unset fields default to 0
        assert_eq!(baseline.unused_types, 0);
        assert_eq!(baseline.boundary_violations, 0);
    }

    #[test]
    fn regression_config_defaults_to_none() {
        let config: FallowConfig = serde_json::from_str("{}").unwrap();
        assert!(config.regression.is_none());
    }

    #[test]
    fn regression_baseline_all_zeros_by_default() {
        let baseline = RegressionBaseline::default();
        assert_eq!(baseline.total_issues, 0);
        assert_eq!(baseline.unused_files, 0);
        assert_eq!(baseline.unused_exports, 0);
        assert_eq!(baseline.unused_types, 0);
        assert_eq!(baseline.unused_dependencies, 0);
        assert_eq!(baseline.unused_dev_dependencies, 0);
        assert_eq!(baseline.unused_optional_dependencies, 0);
        assert_eq!(baseline.unused_enum_members, 0);
        assert_eq!(baseline.unused_class_members, 0);
        assert_eq!(baseline.unresolved_imports, 0);
        assert_eq!(baseline.unlisted_dependencies, 0);
        assert_eq!(baseline.duplicate_exports, 0);
        assert_eq!(baseline.circular_dependencies, 0);
        assert_eq!(baseline.type_only_dependencies, 0);
        assert_eq!(baseline.test_only_dependencies, 0);
        assert_eq!(baseline.boundary_violations, 0);
    }

    #[test]
    fn regression_config_serialize_roundtrip() {
        let baseline = RegressionBaseline {
            total_issues: 100,
            unused_files: 20,
            unused_exports: 30,
            ..RegressionBaseline::default()
        };
        let regression = RegressionConfig {
            baseline: Some(baseline),
        };
        let config = FallowConfig {
            regression: Some(regression),
            ..FallowConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: FallowConfig = serde_json::from_str(&json).unwrap();
        let restored_baseline = restored.regression.unwrap().baseline.unwrap();
        assert_eq!(restored_baseline.total_issues, 100);
        assert_eq!(restored_baseline.unused_files, 20);
        assert_eq!(restored_baseline.unused_exports, 30);
        assert_eq!(restored_baseline.unused_types, 0);
    }

    #[test]
    fn regression_config_empty_baseline_deserialize() {
        let json = r#"{"regression": {}}"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        let regression = config.regression.unwrap();
        assert!(regression.baseline.is_none());
    }

    #[test]
    fn regression_baseline_not_serialized_when_none() {
        let config = FallowConfig {
            regression: None,
            ..FallowConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("regression"),
            "regression should be skipped when None"
        );
    }

    // ── JSON config with overrides and boundaries ──────────────────

    #[test]
    fn deserialize_json_with_overrides() {
        let json = r#"{
            "overrides": [
                {
                    "files": ["*.test.ts", "*.spec.ts"],
                    "rules": {
                        "unused-exports": "off",
                        "unused-files": "warn"
                    }
                }
            ]
        }"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.overrides.len(), 1);
        assert_eq!(config.overrides[0].files.len(), 2);
        assert_eq!(
            config.overrides[0].rules.unused_exports,
            Some(Severity::Off)
        );
        assert_eq!(config.overrides[0].rules.unused_files, Some(Severity::Warn));
    }

    #[test]
    fn deserialize_json_with_boundaries() {
        let json = r#"{
            "boundaries": {
                "preset": "layered"
            }
        }"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.boundaries.preset, Some(BoundaryPreset::Layered));
    }

    // ── TOML with regression config ────────────────────────────────

    #[test]
    fn deserialize_toml_with_regression_baseline() {
        let toml_str = r"
[regression.baseline]
totalIssues = 50
unusedFiles = 10
unusedExports = 15
";
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        let baseline = config.regression.unwrap().baseline.unwrap();
        assert_eq!(baseline.total_issues, 50);
        assert_eq!(baseline.unused_files, 10);
        assert_eq!(baseline.unused_exports, 15);
    }

    // ── TOML with multiple overrides ───────────────────────────────

    #[test]
    fn deserialize_toml_with_overrides() {
        let toml_str = r#"
[[overrides]]
files = ["*.test.ts"]

[overrides.rules]
unused-exports = "off"

[[overrides]]
files = ["*.stories.tsx"]

[overrides.rules]
unused-files = "off"
"#;
        let config: FallowConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.overrides.len(), 2);
        assert_eq!(
            config.overrides[0].rules.unused_exports,
            Some(Severity::Off)
        );
        assert_eq!(config.overrides[1].rules.unused_files, Some(Severity::Off));
    }

    // ── Default regression config ──────────────────────────────────

    #[test]
    fn regression_config_default_is_none_baseline() {
        let config = RegressionConfig::default();
        assert!(config.baseline.is_none());
    }

    // ── Config with multiple ignore export rules ───────────────────

    #[test]
    fn deserialize_json_multiple_ignore_export_rules() {
        let json = r#"{
            "ignoreExports": [
                {"file": "src/types/**/*.ts", "exports": ["*"]},
                {"file": "src/constants.ts", "exports": ["FOO", "BAR"]},
                {"file": "src/index.ts", "exports": ["default"]}
            ]
        }"#;
        let config: FallowConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.ignore_exports.len(), 3);
        assert_eq!(config.ignore_exports[2].exports, vec!["default"]);
    }
}
