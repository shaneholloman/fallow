//! Architecture boundary zone and rule definitions.

use globset::Glob;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Built-in architecture presets.
///
/// Each preset expands into a set of zones and import rules for a common
/// architecture pattern. User-defined zones and rules merge on top of the
/// preset defaults (zones with the same name replace the preset zone;
/// rules with the same `from` replace the preset rule).
///
/// # Examples
///
/// ```
/// use fallow_config::BoundaryPreset;
///
/// let preset: BoundaryPreset = serde_json::from_str(r#""layered""#).unwrap();
/// assert!(matches!(preset, BoundaryPreset::Layered));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum BoundaryPreset {
    /// Classic layered architecture: presentation → application → domain ← infrastructure.
    /// Infrastructure may also import from application (common in DI frameworks).
    Layered,
    /// Hexagonal / ports-and-adapters: adapters → ports → domain.
    Hexagonal,
    /// Feature-Sliced Design: app > pages > widgets > features > entities > shared.
    /// Each layer may only import from layers below it.
    FeatureSliced,
    /// Bulletproof React: app → features → shared + server.
    /// Feature modules are isolated from each other; shared utilities and server
    /// infrastructure form the base layers.
    Bulletproof,
}

impl BoundaryPreset {
    /// Expand the preset into default zones and rules.
    ///
    /// `source_root` is the directory prefix for zone patterns (e.g., `"src"`, `"lib"`).
    /// Patterns are generated as `{source_root}/{zone_name}/**`.
    #[must_use]
    pub fn default_config(&self, source_root: &str) -> (Vec<BoundaryZone>, Vec<BoundaryRule>) {
        match self {
            Self::Layered => Self::layered_config(source_root),
            Self::Hexagonal => Self::hexagonal_config(source_root),
            Self::FeatureSliced => Self::feature_sliced_config(source_root),
            Self::Bulletproof => Self::bulletproof_config(source_root),
        }
    }

    fn zone(name: &str, source_root: &str) -> BoundaryZone {
        BoundaryZone {
            name: name.to_owned(),
            patterns: vec![format!("{source_root}/{name}/**")],
            root: None,
        }
    }

    fn rule(from: &str, allow: &[&str]) -> BoundaryRule {
        BoundaryRule {
            from: from.to_owned(),
            allow: allow.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn layered_config(source_root: &str) -> (Vec<BoundaryZone>, Vec<BoundaryRule>) {
        let zones = vec![
            Self::zone("presentation", source_root),
            Self::zone("application", source_root),
            Self::zone("domain", source_root),
            Self::zone("infrastructure", source_root),
        ];
        let rules = vec![
            Self::rule("presentation", &["application"]),
            Self::rule("application", &["domain"]),
            Self::rule("domain", &[]),
            Self::rule("infrastructure", &["domain", "application"]),
        ];
        (zones, rules)
    }

    fn hexagonal_config(source_root: &str) -> (Vec<BoundaryZone>, Vec<BoundaryRule>) {
        let zones = vec![
            Self::zone("adapters", source_root),
            Self::zone("ports", source_root),
            Self::zone("domain", source_root),
        ];
        let rules = vec![
            Self::rule("adapters", &["ports"]),
            Self::rule("ports", &["domain"]),
            Self::rule("domain", &[]),
        ];
        (zones, rules)
    }

    fn feature_sliced_config(source_root: &str) -> (Vec<BoundaryZone>, Vec<BoundaryRule>) {
        let layer_names = ["app", "pages", "widgets", "features", "entities", "shared"];
        let zones = layer_names
            .iter()
            .map(|name| Self::zone(name, source_root))
            .collect();
        let rules = layer_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let below: Vec<&str> = layer_names[i + 1..].to_vec();
                Self::rule(name, &below)
            })
            .collect();
        (zones, rules)
    }

    fn bulletproof_config(source_root: &str) -> (Vec<BoundaryZone>, Vec<BoundaryRule>) {
        let zones = vec![
            Self::zone("app", source_root),
            Self::zone("features", source_root),
            BoundaryZone {
                name: "shared".to_owned(),
                patterns: [
                    "components",
                    "hooks",
                    "lib",
                    "utils",
                    "utilities",
                    "providers",
                    "shared",
                    "types",
                    "styles",
                    "i18n",
                ]
                .iter()
                .map(|dir| format!("{source_root}/{dir}/**"))
                .collect(),
                root: None,
            },
            Self::zone("server", source_root),
        ];
        let rules = vec![
            Self::rule("app", &["features", "shared", "server"]),
            Self::rule("features", &["shared", "server"]),
            Self::rule("server", &["shared"]),
            Self::rule("shared", &[]),
        ];
        (zones, rules)
    }
}

/// Architecture boundary configuration.
///
/// Defines zones (directory groupings) and rules (which zones may import from which).
/// Optionally uses a built-in preset as a starting point.
///
/// # Examples
///
/// ```
/// use fallow_config::BoundaryConfig;
///
/// let json = r#"{
///     "zones": [
///         { "name": "ui", "patterns": ["src/components/**"] },
///         { "name": "db", "patterns": ["src/db/**"] }
///     ],
///     "rules": [
///         { "from": "ui", "allow": ["db"] }
///     ]
/// }"#;
/// let config: BoundaryConfig = serde_json::from_str(json).unwrap();
/// assert_eq!(config.zones.len(), 2);
/// assert_eq!(config.rules.len(), 1);
/// ```
///
/// Using a preset:
///
/// ```
/// use fallow_config::BoundaryConfig;
///
/// let json = r#"{ "preset": "layered" }"#;
/// let mut config: BoundaryConfig = serde_json::from_str(json).unwrap();
/// config.expand("src");
/// assert_eq!(config.zones.len(), 4);
/// assert_eq!(config.rules.len(), 4);
/// ```
#[derive(Debug, Default, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryConfig {
    /// Built-in architecture preset. When set, expands into default zones and rules.
    /// User-defined zones and rules merge on top: zones with the same name replace
    /// the preset zone; rules with the same `from` replace the preset rule.
    /// Preset patterns use `{rootDir}/{zone}/**` where rootDir is auto-detected
    /// from tsconfig.json (falls back to `src`).
    /// Note: preset patterns are flat (`src/<zone>/**`). For monorepos with
    /// per-package source directories, define zones explicitly instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<BoundaryPreset>,
    /// Named zones mapping directory patterns to architectural layers.
    #[serde(default)]
    pub zones: Vec<BoundaryZone>,
    /// Import rules between zones. A zone with a rule entry can only import
    /// from the listed zones (plus itself). A zone without a rule entry is unrestricted.
    #[serde(default)]
    pub rules: Vec<BoundaryRule>,
}

/// A named zone grouping files by directory pattern.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryZone {
    /// Zone identifier referenced in rules (e.g., `"ui"`, `"database"`, `"shared"`).
    pub name: String,
    /// Glob patterns (relative to project root) that define zone membership.
    /// A file belongs to the first zone whose pattern matches.
    pub patterns: Vec<String>,
    /// Optional subtree scope. Reserved for future subtree-relative pattern
    /// support: when set, patterns would resolve relative to this directory
    /// instead of the project root (useful for monorepos with per-package
    /// boundaries). Currently inert: the detector ignores the field, and
    /// `BoundaryConfig::resolve` emits a warning tagged
    /// `FALLOW-BOUNDARY-ROOT-RESERVED` so users do not silently rely on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

/// An import rule between zones.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryRule {
    /// The zone this rule applies to (the importing side).
    pub from: String,
    /// Zones that `from` is allowed to import from. Self-imports are always allowed.
    /// An empty list means the zone may not import from any other zone.
    #[serde(default)]
    pub allow: Vec<String>,
}

/// Resolved boundary config with pre-compiled glob matchers.
#[derive(Debug, Default)]
pub struct ResolvedBoundaryConfig {
    /// Zones with compiled glob matchers for fast file classification.
    pub zones: Vec<ResolvedZone>,
    /// Rules indexed by source zone name.
    pub rules: Vec<ResolvedBoundaryRule>,
}

/// A zone with pre-compiled glob matchers.
#[derive(Debug)]
pub struct ResolvedZone {
    /// Zone identifier.
    pub name: String,
    /// Pre-compiled glob matchers for zone membership.
    pub matchers: Vec<globset::GlobMatcher>,
}

/// A resolved boundary rule.
#[derive(Debug)]
pub struct ResolvedBoundaryRule {
    /// The zone this rule restricts.
    pub from_zone: String,
    /// Zones that `from_zone` is allowed to import from.
    pub allowed_zones: Vec<String>,
}

impl BoundaryConfig {
    /// Whether any boundaries are configured (including via preset).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.preset.is_none() && self.zones.is_empty()
    }

    /// Expand the preset (if set) into zones and rules, merging user overrides on top.
    ///
    /// `source_root` is the directory prefix for preset zone patterns (e.g., `"src"`).
    /// After expansion, `self.preset` is cleared and all zones/rules are explicit.
    ///
    /// Merge semantics:
    /// - User zones with the same name as a preset zone **replace** the preset zone entirely.
    /// - User rules with the same `from` as a preset rule **replace** the preset rule.
    /// - User zones/rules with new names **add** to the preset set.
    pub fn expand(&mut self, source_root: &str) {
        let Some(preset) = self.preset.take() else {
            return;
        };

        let (preset_zones, preset_rules) = preset.default_config(source_root);

        // Build set of user-defined zone names for override detection.
        let user_zone_names: rustc_hash::FxHashSet<&str> =
            self.zones.iter().map(|z| z.name.as_str()).collect();

        // Start with preset zones, replacing any that the user overrides.
        let mut merged_zones: Vec<BoundaryZone> = preset_zones
            .into_iter()
            .filter(|pz| {
                if user_zone_names.contains(pz.name.as_str()) {
                    tracing::info!(
                        "boundary preset: user zone '{}' replaces preset zone",
                        pz.name
                    );
                    false
                } else {
                    true
                }
            })
            .collect();
        // Append all user zones (both overrides and additions).
        merged_zones.append(&mut self.zones);
        self.zones = merged_zones;

        // Build set of user-defined rule `from` names for override detection.
        let user_rule_sources: rustc_hash::FxHashSet<&str> =
            self.rules.iter().map(|r| r.from.as_str()).collect();

        let mut merged_rules: Vec<BoundaryRule> = preset_rules
            .into_iter()
            .filter(|pr| {
                if user_rule_sources.contains(pr.from.as_str()) {
                    tracing::info!(
                        "boundary preset: user rule for '{}' replaces preset rule",
                        pr.from
                    );
                    false
                } else {
                    true
                }
            })
            .collect();
        merged_rules.append(&mut self.rules);
        self.rules = merged_rules;
    }

    /// Return the preset name if one is configured but not yet expanded.
    #[must_use]
    pub fn preset_name(&self) -> Option<&str> {
        self.preset.as_ref().map(|p| match p {
            BoundaryPreset::Layered => "layered",
            BoundaryPreset::Hexagonal => "hexagonal",
            BoundaryPreset::FeatureSliced => "feature-sliced",
            BoundaryPreset::Bulletproof => "bulletproof",
        })
    }

    /// Return the names of zones that set the reserved `root` field. The
    /// detector currently ignores `root`; consumers (the resolver, CLI
    /// validate command) call this to warn users so the silent no-op cannot
    /// be mistaken for an enforced subtree scope.
    #[must_use]
    pub fn reserved_root_zones(&self) -> Vec<&str> {
        self.zones
            .iter()
            .filter(|z| z.root.is_some())
            .map(|z| z.name.as_str())
            .collect()
    }

    /// Validate that all zone names referenced in rules are defined in `zones`.
    /// Returns a list of (rule_index, undefined_zone_name) pairs.
    #[must_use]
    pub fn validate_zone_references(&self) -> Vec<(usize, &str)> {
        let zone_names: rustc_hash::FxHashSet<&str> =
            self.zones.iter().map(|z| z.name.as_str()).collect();

        let mut errors = Vec::new();
        for (i, rule) in self.rules.iter().enumerate() {
            if !zone_names.contains(rule.from.as_str()) {
                errors.push((i, rule.from.as_str()));
            }
            for allowed in &rule.allow {
                if !zone_names.contains(allowed.as_str()) {
                    errors.push((i, allowed.as_str()));
                }
            }
        }
        errors
    }

    /// Resolve into compiled form with pre-built glob matchers.
    /// Invalid glob patterns are logged and skipped.
    #[must_use]
    pub fn resolve(&self) -> ResolvedBoundaryConfig {
        let zones = self
            .zones
            .iter()
            .map(|zone| {
                let matchers = zone
                    .patterns
                    .iter()
                    .filter_map(|pattern| match Glob::new(pattern) {
                        Ok(glob) => Some(glob.compile_matcher()),
                        Err(e) => {
                            tracing::warn!(
                                "invalid boundary zone glob pattern '{}' in zone '{}': {e}",
                                pattern,
                                zone.name
                            );
                            None
                        }
                    })
                    .collect();
                ResolvedZone {
                    name: zone.name.clone(),
                    matchers,
                }
            })
            .collect();

        let rules = self
            .rules
            .iter()
            .map(|rule| ResolvedBoundaryRule {
                from_zone: rule.from.clone(),
                allowed_zones: rule.allow.clone(),
            })
            .collect();

        ResolvedBoundaryConfig { zones, rules }
    }
}

impl ResolvedBoundaryConfig {
    /// Whether any boundaries are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Classify a file path into a zone. Returns the first matching zone name.
    /// Path should be relative to the project root with forward slashes.
    #[must_use]
    pub fn classify_zone(&self, relative_path: &str) -> Option<&str> {
        for zone in &self.zones {
            if zone.matchers.iter().any(|m| m.is_match(relative_path)) {
                return Some(&zone.name);
            }
        }
        None
    }

    /// Check if an import from `from_zone` to `to_zone` is allowed.
    /// Returns `true` if the import is permitted.
    #[must_use]
    pub fn is_import_allowed(&self, from_zone: &str, to_zone: &str) -> bool {
        // Self-imports are always allowed.
        if from_zone == to_zone {
            return true;
        }

        // Find the rule for the source zone.
        let rule = self.rules.iter().find(|r| r.from_zone == from_zone);

        match rule {
            // Zone has no rule entry — unrestricted.
            None => true,
            // Zone has a rule — check the allowlist.
            Some(r) => r.allowed_zones.iter().any(|z| z == to_zone),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config() {
        let config = BoundaryConfig::default();
        assert!(config.is_empty());
        assert!(config.validate_zone_references().is_empty());
    }

    #[test]
    fn deserialize_json() {
        let json = r#"{
            "zones": [
                { "name": "ui", "patterns": ["src/components/**", "src/pages/**"] },
                { "name": "db", "patterns": ["src/db/**"] },
                { "name": "shared", "patterns": ["src/shared/**"] }
            ],
            "rules": [
                { "from": "ui", "allow": ["shared"] },
                { "from": "db", "allow": ["shared"] }
            ]
        }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.zones.len(), 3);
        assert_eq!(config.rules.len(), 2);
        assert_eq!(config.zones[0].name, "ui");
        assert_eq!(
            config.zones[0].patterns,
            vec!["src/components/**", "src/pages/**"]
        );
        assert_eq!(config.rules[0].from, "ui");
        assert_eq!(config.rules[0].allow, vec!["shared"]);
    }

    #[test]
    fn deserialize_toml() {
        let toml_str = r#"
[[zones]]
name = "ui"
patterns = ["src/components/**"]

[[zones]]
name = "db"
patterns = ["src/db/**"]

[[rules]]
from = "ui"
allow = ["db"]
"#;
        let config: BoundaryConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.zones.len(), 2);
        assert_eq!(config.rules.len(), 1);
    }

    #[test]
    fn validate_zone_references_valid() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["db".to_string()],
            }],
        };
        assert!(config.validate_zone_references().is_empty());
    }

    #[test]
    fn validate_zone_references_invalid_from() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "nonexistent".to_string(),
                allow: vec!["ui".to_string()],
            }],
        };
        let errors = config.validate_zone_references();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, "nonexistent");
    }

    #[test]
    fn validate_zone_references_invalid_allow() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["nonexistent".to_string()],
            }],
        };
        let errors = config.validate_zone_references();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1, "nonexistent");
    }

    #[test]
    fn resolve_and_classify() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec!["src/components/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec!["src/db/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/components/Button.tsx"),
            Some("ui")
        );
        assert_eq!(resolved.classify_zone("src/db/queries.ts"), Some("db"));
        assert_eq!(resolved.classify_zone("src/utils/helpers.ts"), None);
    }

    #[test]
    fn first_match_wins() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "specific".to_string(),
                    patterns: vec!["src/shared/db-utils/**".to_string()],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec!["src/shared/**".to_string()],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/shared/db-utils/pool.ts"),
            Some("specific")
        );
        assert_eq!(
            resolved.classify_zone("src/shared/helpers.ts"),
            Some("shared")
        );
    }

    #[test]
    fn self_import_always_allowed() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec![],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("ui", "ui"));
    }

    #[test]
    fn unrestricted_zone_allows_all() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("shared", "db"));
    }

    #[test]
    fn restricted_zone_blocks_unlisted() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "ui".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "db".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "shared".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["shared".to_string()],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("ui", "shared"));
        assert!(!resolved.is_import_allowed("ui", "db"));
    }

    #[test]
    fn empty_allow_blocks_all_except_self() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "isolated".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "other".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "isolated".to_string(),
                allow: vec![],
            }],
        };
        let resolved = config.resolve();
        assert!(resolved.is_import_allowed("isolated", "isolated"));
        assert!(!resolved.is_import_allowed("isolated", "other"));
    }

    #[test]
    fn root_field_reserved() {
        let json = r#"{
            "zones": [
                { "name": "ui", "patterns": ["src/**"], "root": "packages/app/" },
                { "name": "api", "patterns": ["api/**"] }
            ],
            "rules": []
        }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.zones[0].root.as_deref(), Some("packages/app/"));
        assert_eq!(config.reserved_root_zones(), vec!["ui"]);
    }

    #[test]
    fn reserved_root_zones_empty_when_no_zone_sets_root() {
        let json = r#"{
            "zones": [{ "name": "ui", "patterns": ["src/**"] }],
            "rules": []
        }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert!(config.reserved_root_zones().is_empty());
    }

    // ── Preset deserialization ─────────────────────────────────

    #[test]
    fn deserialize_preset_json() {
        let json = r#"{ "preset": "layered" }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.preset, Some(BoundaryPreset::Layered));
        assert!(config.zones.is_empty());
    }

    #[test]
    fn deserialize_preset_hexagonal_json() {
        let json = r#"{ "preset": "hexagonal" }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.preset, Some(BoundaryPreset::Hexagonal));
    }

    #[test]
    fn deserialize_preset_feature_sliced_json() {
        let json = r#"{ "preset": "feature-sliced" }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.preset, Some(BoundaryPreset::FeatureSliced));
    }

    #[test]
    fn deserialize_preset_toml() {
        let toml_str = r#"preset = "layered""#;
        let config: BoundaryConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.preset, Some(BoundaryPreset::Layered));
    }

    #[test]
    fn deserialize_invalid_preset_rejected() {
        let json = r#"{ "preset": "invalid_preset" }"#;
        let result: Result<BoundaryConfig, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn preset_absent_by_default() {
        let config = BoundaryConfig::default();
        assert!(config.preset.is_none());
        assert!(config.is_empty());
    }

    #[test]
    fn preset_makes_config_non_empty() {
        let config = BoundaryConfig {
            preset: Some(BoundaryPreset::Layered),
            zones: vec![],
            rules: vec![],
        };
        assert!(!config.is_empty());
    }

    // ── Preset expansion ───────────────────────────────────────

    #[test]
    fn expand_layered_produces_four_zones() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Layered),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 4);
        assert_eq!(config.rules.len(), 4);
        assert!(config.preset.is_none(), "preset cleared after expand");
        assert_eq!(config.zones[0].name, "presentation");
        assert_eq!(config.zones[0].patterns, vec!["src/presentation/**"]);
    }

    #[test]
    fn expand_layered_rules_correct() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Layered),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        // presentation → application only
        let pres_rule = config
            .rules
            .iter()
            .find(|r| r.from == "presentation")
            .unwrap();
        assert_eq!(pres_rule.allow, vec!["application"]);
        // application → domain only
        let app_rule = config
            .rules
            .iter()
            .find(|r| r.from == "application")
            .unwrap();
        assert_eq!(app_rule.allow, vec!["domain"]);
        // domain → nothing
        let dom_rule = config.rules.iter().find(|r| r.from == "domain").unwrap();
        assert!(dom_rule.allow.is_empty());
        // infrastructure → domain + application (DI-friendly)
        let infra_rule = config
            .rules
            .iter()
            .find(|r| r.from == "infrastructure")
            .unwrap();
        assert_eq!(infra_rule.allow, vec!["domain", "application"]);
    }

    #[test]
    fn expand_hexagonal_produces_three_zones() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 3);
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.zones[0].name, "adapters");
        assert_eq!(config.zones[1].name, "ports");
        assert_eq!(config.zones[2].name, "domain");
    }

    #[test]
    fn expand_feature_sliced_produces_six_zones() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::FeatureSliced),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 6);
        assert_eq!(config.rules.len(), 6);
        // app can import everything below
        let app_rule = config.rules.iter().find(|r| r.from == "app").unwrap();
        assert_eq!(
            app_rule.allow,
            vec!["pages", "widgets", "features", "entities", "shared"]
        );
        // shared imports nothing
        let shared_rule = config.rules.iter().find(|r| r.from == "shared").unwrap();
        assert!(shared_rule.allow.is_empty());
        // entities → shared only
        let ent_rule = config.rules.iter().find(|r| r.from == "entities").unwrap();
        assert_eq!(ent_rule.allow, vec!["shared"]);
    }

    #[test]
    fn expand_bulletproof_produces_four_zones() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Bulletproof),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 4);
        assert_eq!(config.rules.len(), 4);
        assert_eq!(config.zones[0].name, "app");
        assert_eq!(config.zones[1].name, "features");
        assert_eq!(config.zones[2].name, "shared");
        assert_eq!(config.zones[3].name, "server");
        // shared zone has multiple patterns
        assert!(config.zones[2].patterns.len() > 1);
        assert!(
            config.zones[2]
                .patterns
                .contains(&"src/components/**".to_string())
        );
        assert!(
            config.zones[2]
                .patterns
                .contains(&"src/hooks/**".to_string())
        );
        assert!(config.zones[2].patterns.contains(&"src/lib/**".to_string()));
        assert!(
            config.zones[2]
                .patterns
                .contains(&"src/providers/**".to_string())
        );
    }

    #[test]
    fn expand_bulletproof_rules_correct() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Bulletproof),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        // app → features, shared, server
        let app_rule = config.rules.iter().find(|r| r.from == "app").unwrap();
        assert_eq!(app_rule.allow, vec!["features", "shared", "server"]);
        // features → shared, server
        let feat_rule = config.rules.iter().find(|r| r.from == "features").unwrap();
        assert_eq!(feat_rule.allow, vec!["shared", "server"]);
        // server → shared
        let srv_rule = config.rules.iter().find(|r| r.from == "server").unwrap();
        assert_eq!(srv_rule.allow, vec!["shared"]);
        // shared → nothing (isolated)
        let shared_rule = config.rules.iter().find(|r| r.from == "shared").unwrap();
        assert!(shared_rule.allow.is_empty());
    }

    #[test]
    fn expand_bulletproof_then_resolve_classifies() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Bulletproof),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/app/dashboard/page.tsx"),
            Some("app")
        );
        assert_eq!(
            resolved.classify_zone("src/features/auth/hooks/useAuth.ts"),
            Some("features")
        );
        assert_eq!(
            resolved.classify_zone("src/components/Button/Button.tsx"),
            Some("shared")
        );
        assert_eq!(
            resolved.classify_zone("src/hooks/useFormatters.ts"),
            Some("shared")
        );
        assert_eq!(
            resolved.classify_zone("src/server/db/schema/users.ts"),
            Some("server")
        );
        // features cannot import shared directly — only via allowed rules
        assert!(resolved.is_import_allowed("features", "shared"));
        assert!(resolved.is_import_allowed("features", "server"));
        assert!(!resolved.is_import_allowed("features", "app"));
        assert!(!resolved.is_import_allowed("shared", "features"));
        assert!(!resolved.is_import_allowed("server", "features"));
    }

    #[test]
    fn expand_uses_custom_source_root() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![],
            rules: vec![],
        };
        config.expand("lib");
        assert_eq!(config.zones[0].patterns, vec!["lib/adapters/**"]);
        assert_eq!(config.zones[2].patterns, vec!["lib/domain/**"]);
    }

    // ── Preset merge behavior ──────────────────────────────────

    #[test]
    fn user_zone_replaces_preset_zone() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![BoundaryZone {
                name: "domain".to_string(),
                patterns: vec!["src/core/**".to_string()],
                root: None,
            }],
            rules: vec![],
        };
        config.expand("src");
        // 3 zones total: adapters + ports from preset, domain from user
        assert_eq!(config.zones.len(), 3);
        let domain = config.zones.iter().find(|z| z.name == "domain").unwrap();
        assert_eq!(domain.patterns, vec!["src/core/**"]);
    }

    #[test]
    fn user_zone_adds_to_preset() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![BoundaryZone {
                name: "shared".to_string(),
                patterns: vec!["src/shared/**".to_string()],
                root: None,
            }],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 4); // 3 preset + 1 user
        assert!(config.zones.iter().any(|z| z.name == "shared"));
    }

    #[test]
    fn user_rule_replaces_preset_rule() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![],
            rules: vec![BoundaryRule {
                from: "adapters".to_string(),
                allow: vec!["ports".to_string(), "domain".to_string()],
            }],
        };
        config.expand("src");
        let adapter_rule = config.rules.iter().find(|r| r.from == "adapters").unwrap();
        // User rule allows both ports and domain (preset only allowed ports)
        assert_eq!(adapter_rule.allow, vec!["ports", "domain"]);
        // Other preset rules untouched
        assert_eq!(
            config.rules.iter().filter(|r| r.from == "adapters").count(),
            1
        );
    }

    #[test]
    fn expand_without_preset_is_noop() {
        let mut config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec!["src/ui/**".to_string()],
                root: None,
            }],
            rules: vec![],
        };
        config.expand("src");
        assert_eq!(config.zones.len(), 1);
        assert_eq!(config.zones[0].name, "ui");
    }

    #[test]
    fn expand_then_validate_succeeds() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Layered),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        assert!(config.validate_zone_references().is_empty());
    }

    #[test]
    fn expand_then_resolve_classifies() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::Hexagonal),
            zones: vec![],
            rules: vec![],
        };
        config.expand("src");
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/adapters/http/handler.ts"),
            Some("adapters")
        );
        assert_eq!(resolved.classify_zone("src/domain/user.ts"), Some("domain"));
        assert!(!resolved.is_import_allowed("adapters", "domain"));
        assert!(resolved.is_import_allowed("adapters", "ports"));
    }

    #[test]
    fn preset_name_returns_correct_string() {
        let config = BoundaryConfig {
            preset: Some(BoundaryPreset::FeatureSliced),
            zones: vec![],
            rules: vec![],
        };
        assert_eq!(config.preset_name(), Some("feature-sliced"));

        let empty = BoundaryConfig::default();
        assert_eq!(empty.preset_name(), None);
    }

    #[test]
    fn preset_name_all_variants() {
        let cases = [
            (BoundaryPreset::Layered, "layered"),
            (BoundaryPreset::Hexagonal, "hexagonal"),
            (BoundaryPreset::FeatureSliced, "feature-sliced"),
            (BoundaryPreset::Bulletproof, "bulletproof"),
        ];
        for (preset, expected_name) in cases {
            let config = BoundaryConfig {
                preset: Some(preset),
                zones: vec![],
                rules: vec![],
            };
            assert_eq!(
                config.preset_name(),
                Some(expected_name),
                "preset_name() mismatch for variant"
            );
        }
    }

    // ── ResolvedBoundaryConfig::is_empty ────────────────────────────

    #[test]
    fn resolved_boundary_config_empty() {
        let resolved = ResolvedBoundaryConfig::default();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolved_boundary_config_with_zones_not_empty() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec!["src/ui/**".to_string()],
                root: None,
            }],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert!(!resolved.is_empty());
    }

    // ── BoundaryConfig::is_empty edge cases ─────────────────────────

    #[test]
    fn boundary_config_with_only_rules_is_empty() {
        // Having rules but no zones/preset is still "empty" since rules without zones
        // cannot produce boundary violations.
        let config = BoundaryConfig {
            preset: None,
            zones: vec![],
            rules: vec![BoundaryRule {
                from: "ui".to_string(),
                allow: vec!["db".to_string()],
            }],
        };
        assert!(config.is_empty());
    }

    #[test]
    fn boundary_config_with_zones_not_empty() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![],
        };
        assert!(!config.is_empty());
    }

    // ── Multiple zone patterns ──────────────────────────────────────

    #[test]
    fn zone_with_multiple_patterns_matches_any() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![
                    "src/components/**".to_string(),
                    "src/pages/**".to_string(),
                    "src/views/**".to_string(),
                ],
                root: None,
            }],
            rules: vec![],
        };
        let resolved = config.resolve();
        assert_eq!(
            resolved.classify_zone("src/components/Button.tsx"),
            Some("ui")
        );
        assert_eq!(resolved.classify_zone("src/pages/Home.tsx"), Some("ui"));
        assert_eq!(
            resolved.classify_zone("src/views/Dashboard.tsx"),
            Some("ui")
        );
        assert_eq!(resolved.classify_zone("src/utils/helpers.ts"), None);
    }

    // ── validate_zone_references with multiple errors ───────────────

    #[test]
    fn validate_zone_references_multiple_errors() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "ui".to_string(),
                patterns: vec![],
                root: None,
            }],
            rules: vec![
                BoundaryRule {
                    from: "nonexistent_from".to_string(),
                    allow: vec!["nonexistent_allow".to_string()],
                },
                BoundaryRule {
                    from: "ui".to_string(),
                    allow: vec!["also_nonexistent".to_string()],
                },
            ],
        };
        let errors = config.validate_zone_references();
        // Rule 0: invalid "from" + invalid "allow" = 2 errors
        // Rule 1: valid "from", invalid "allow" = 1 error
        assert_eq!(errors.len(), 3);
    }

    // ── Preset expansion with custom source root ────────────────────

    #[test]
    fn expand_feature_sliced_with_custom_root() {
        let mut config = BoundaryConfig {
            preset: Some(BoundaryPreset::FeatureSliced),
            zones: vec![],
            rules: vec![],
        };
        config.expand("lib");
        assert_eq!(config.zones[0].patterns, vec!["lib/app/**"]);
        assert_eq!(config.zones[5].patterns, vec!["lib/shared/**"]);
    }

    // ── is_import_allowed for zone not in rules (unrestricted) ──────

    #[test]
    fn zone_not_in_rules_is_unrestricted() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![
                BoundaryZone {
                    name: "a".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "b".to_string(),
                    patterns: vec![],
                    root: None,
                },
                BoundaryZone {
                    name: "c".to_string(),
                    patterns: vec![],
                    root: None,
                },
            ],
            rules: vec![BoundaryRule {
                from: "a".to_string(),
                allow: vec!["b".to_string()],
            }],
        };
        let resolved = config.resolve();
        // "a" is restricted: can import from "b" but not "c"
        assert!(resolved.is_import_allowed("a", "b"));
        assert!(!resolved.is_import_allowed("a", "c"));
        // "b" has no rule entry: unrestricted
        assert!(resolved.is_import_allowed("b", "a"));
        assert!(resolved.is_import_allowed("b", "c"));
        // "c" has no rule entry: unrestricted
        assert!(resolved.is_import_allowed("c", "a"));
    }

    // ── Preset serialization/deserialization roundtrip ───────────────

    #[test]
    fn boundary_preset_json_roundtrip() {
        let presets = [
            BoundaryPreset::Layered,
            BoundaryPreset::Hexagonal,
            BoundaryPreset::FeatureSliced,
            BoundaryPreset::Bulletproof,
        ];
        for preset in presets {
            let json = serde_json::to_string(&preset).unwrap();
            let restored: BoundaryPreset = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, preset);
        }
    }

    #[test]
    fn deserialize_preset_bulletproof_json() {
        let json = r#"{ "preset": "bulletproof" }"#;
        let config: BoundaryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.preset, Some(BoundaryPreset::Bulletproof));
    }

    // ── Zone with invalid glob ──────────────────────────────────────

    #[test]
    fn resolve_skips_invalid_zone_glob() {
        let config = BoundaryConfig {
            preset: None,
            zones: vec![BoundaryZone {
                name: "broken".to_string(),
                patterns: vec!["[invalid".to_string()],
                root: None,
            }],
            rules: vec![],
        };
        let resolved = config.resolve();
        // Zone exists but has no valid matchers, so no file can be classified into it
        assert!(!resolved.is_empty());
        assert_eq!(resolved.classify_zone("anything.ts"), None);
    }
}
