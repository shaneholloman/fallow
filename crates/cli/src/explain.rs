//! Metric and rule definitions for explainable CLI output.
//!
//! Provides structured metadata that describes what each metric, threshold,
//! and rule means — consumed by the `_meta` object in JSON output and by
//! SARIF `fullDescription` / `helpUri` fields.

use serde_json::{Value, json};

// ── Docs base URL ────────────────────────────────────────────────

const DOCS_BASE: &str = "https://docs.fallow.tools";

/// Docs URL for the dead-code (check) command.
pub const CHECK_DOCS: &str = "https://docs.fallow.tools/cli/dead-code";

/// Docs URL for the health command.
pub const HEALTH_DOCS: &str = "https://docs.fallow.tools/cli/health";

/// Docs URL for the dupes command.
pub const DUPES_DOCS: &str = "https://docs.fallow.tools/cli/dupes";

// ── Check rules ─────────────────────────────────────────────────

/// Rule definition for SARIF `fullDescription` and JSON `_meta`.
pub struct RuleDef {
    pub id: &'static str,
    pub name: &'static str,
    pub short: &'static str,
    pub full: &'static str,
    pub docs_path: &'static str,
}

pub const CHECK_RULES: &[RuleDef] = &[
    RuleDef {
        id: "fallow/unused-file",
        name: "Unused Files",
        short: "File is not reachable from any entry point",
        full: "Source files that are not imported by any other module and are not entry points (scripts, tests, configs). These files can safely be deleted. Detection uses graph reachability from configured entry points.",
        docs_path: "explanations/dead-code#unused-files",
    },
    RuleDef {
        id: "fallow/unused-export",
        name: "Unused Exports",
        short: "Export is never imported",
        full: "Named exports that are never imported by any other module in the project. Includes both direct exports and re-exports through barrel files. The export may still be used locally within the same file.",
        docs_path: "explanations/dead-code#unused-exports",
    },
    RuleDef {
        id: "fallow/unused-type",
        name: "Unused Type Exports",
        short: "Type export is never imported",
        full: "Type-only exports (interfaces, type aliases, enums used only as types) that are never imported. These do not generate runtime code but add maintenance burden.",
        docs_path: "explanations/dead-code#unused-types",
    },
    RuleDef {
        id: "fallow/private-type-leak",
        name: "Private Type Leaks",
        short: "Exported signature references a private type",
        full: "Exported values or types whose public TypeScript signature references a same-file type declaration that is not exported. Consumers cannot name that private type directly, so the backing type should be exported or removed from the public signature.",
        docs_path: "explanations/dead-code#private-type-leaks",
    },
    RuleDef {
        id: "fallow/unused-dependency",
        name: "Unused Dependencies",
        short: "Dependency listed but never imported",
        full: "Packages listed in dependencies that are never imported or required by any source file. Framework plugins and CLI tools may be false positives — use the ignore_dependencies config to suppress.",
        docs_path: "explanations/dead-code#unused-dependencies",
    },
    RuleDef {
        id: "fallow/unused-dev-dependency",
        name: "Unused Dev Dependencies",
        short: "Dev dependency listed but never imported",
        full: "Packages listed in devDependencies that are never imported by test files, config files, or scripts. Build tools and jest presets that are referenced only in config may appear as false positives.",
        docs_path: "explanations/dead-code#unused-devdependencies",
    },
    RuleDef {
        id: "fallow/unused-optional-dependency",
        name: "Unused Optional Dependencies",
        short: "Optional dependency listed but never imported",
        full: "Packages listed in optionalDependencies that are never imported. Optional dependencies are typically platform-specific — verify they are not needed on any supported platform before removing.",
        docs_path: "explanations/dead-code#unused-optionaldependencies",
    },
    RuleDef {
        id: "fallow/type-only-dependency",
        name: "Type-only Dependencies",
        short: "Production dependency only used via type-only imports",
        full: "Production dependencies that are only imported via `import type` statements. These can be moved to devDependencies since they generate no runtime code and are stripped during compilation.",
        docs_path: "explanations/dead-code#type-only-dependencies",
    },
    RuleDef {
        id: "fallow/unused-enum-member",
        name: "Unused Enum Members",
        short: "Enum member is never referenced",
        full: "Enum members that are never referenced in the codebase. Uses scope-aware binding analysis to track all references including computed access patterns.",
        docs_path: "explanations/dead-code#unused-enum-members",
    },
    RuleDef {
        id: "fallow/unused-class-member",
        name: "Unused Class Members",
        short: "Class member is never referenced",
        full: "Class methods and properties that are never referenced outside the class. Private members are checked within the class scope; public members are checked project-wide.",
        docs_path: "explanations/dead-code#unused-class-members",
    },
    RuleDef {
        id: "fallow/unresolved-import",
        name: "Unresolved Imports",
        short: "Import could not be resolved",
        full: "Import specifiers that could not be resolved to a file on disk. Common causes: deleted files, typos in paths, missing path aliases in tsconfig, or uninstalled packages.",
        docs_path: "explanations/dead-code#unresolved-imports",
    },
    RuleDef {
        id: "fallow/unlisted-dependency",
        name: "Unlisted Dependencies",
        short: "Dependency used but not in package.json",
        full: "Packages that are imported in source code but not listed in package.json. These work by accident (hoisted from another workspace package or transitive dep) and will break in strict package managers.",
        docs_path: "explanations/dead-code#unlisted-dependencies",
    },
    RuleDef {
        id: "fallow/duplicate-export",
        name: "Duplicate Exports",
        short: "Export name appears in multiple modules",
        full: "The same export name is defined in multiple modules. Consumers may import from the wrong module, leading to subtle bugs. Consider renaming or consolidating.",
        docs_path: "explanations/dead-code#duplicate-exports",
    },
    RuleDef {
        id: "fallow/circular-dependency",
        name: "Circular Dependencies",
        short: "Circular dependency chain detected",
        full: "A cycle in the module import graph. Circular dependencies cause undefined behavior with CommonJS (partial modules) and initialization ordering issues with ESM. Break cycles by extracting shared code.",
        docs_path: "explanations/dead-code#circular-dependencies",
    },
    RuleDef {
        id: "fallow/stale-suppression",
        name: "Stale Suppressions",
        short: "Suppression comment or tag no longer matches any issue",
        full: "A fallow-ignore-next-line, fallow-ignore-file, or @expected-unused suppression that no longer matches any active issue. The underlying problem was fixed but the suppression was left behind. Remove it to keep the codebase clean.",
        docs_path: "explanations/dead-code#stale-suppressions",
    },
];

/// Look up a rule definition by its SARIF rule ID across all rule sets.
#[must_use]
pub fn rule_by_id(id: &str) -> Option<&'static RuleDef> {
    CHECK_RULES
        .iter()
        .chain(HEALTH_RULES.iter())
        .chain(DUPES_RULES.iter())
        .find(|r| r.id == id)
}

/// Build the docs URL for a rule.
#[must_use]
pub fn rule_docs_url(rule: &RuleDef) -> String {
    format!("{DOCS_BASE}/{}", rule.docs_path)
}

// ── Health SARIF rules ──────────────────────────────────────────

pub const HEALTH_RULES: &[RuleDef] = &[
    RuleDef {
        id: "fallow/high-cyclomatic-complexity",
        name: "High Cyclomatic Complexity",
        short: "Function has high cyclomatic complexity",
        full: "McCabe cyclomatic complexity exceeds the configured threshold. Cyclomatic complexity counts the number of independent paths through a function (1 + decision points: if/else, switch cases, loops, ternary, logical operators). High values indicate functions that are hard to test exhaustively.",
        docs_path: "explanations/health#cyclomatic-complexity",
    },
    RuleDef {
        id: "fallow/high-cognitive-complexity",
        name: "High Cognitive Complexity",
        short: "Function has high cognitive complexity",
        full: "SonarSource cognitive complexity exceeds the configured threshold. Unlike cyclomatic complexity, cognitive complexity penalizes nesting depth and non-linear control flow (breaks, continues, early returns). It measures how hard a function is to understand when reading sequentially.",
        docs_path: "explanations/health#cognitive-complexity",
    },
    RuleDef {
        id: "fallow/high-complexity",
        name: "High Complexity (Both)",
        short: "Function exceeds both complexity thresholds",
        full: "Function exceeds both cyclomatic and cognitive complexity thresholds. This is the strongest signal that a function needs refactoring, it has many paths AND is hard to understand.",
        docs_path: "explanations/health#complexity-metrics",
    },
    RuleDef {
        id: "fallow/high-crap-score",
        name: "High CRAP Score",
        short: "Function has a high CRAP score (complexity combined with low coverage)",
        full: "The function's CRAP (Change Risk Anti-Patterns) score meets or exceeds the configured threshold. CRAP combines cyclomatic complexity with test coverage using the Savoia and Evans (2007) formula: `CC^2 * (1 - coverage/100)^3 + CC`. High CRAP indicates changes to this function carry high risk because it is complex AND poorly tested. Pair with `--coverage` for accurate per-function scoring; without it fallow estimates coverage from the module graph.",
        docs_path: "explanations/health#crap-score",
    },
    RuleDef {
        id: "fallow/refactoring-target",
        name: "Refactoring Target",
        short: "File identified as a high-priority refactoring candidate",
        full: "File identified as a refactoring candidate based on a weighted combination of complexity density, churn velocity, dead code ratio, fan-in (blast radius), and fan-out (coupling). Categories: urgent churn+complexity, break circular dependency, split high-impact file, remove dead code, extract complex functions, reduce coupling.",
        docs_path: "explanations/health#refactoring-targets",
    },
    RuleDef {
        id: "fallow/untested-file",
        name: "Untested File",
        short: "Runtime-reachable file has no test dependency path",
        full: "A file is reachable from runtime entry points but not from any discovered test entry point. This indicates production code that no test imports, directly or transitively, according to the static module graph.",
        docs_path: "explanations/health#coverage-gaps",
    },
    RuleDef {
        id: "fallow/untested-export",
        name: "Untested Export",
        short: "Runtime-reachable export has no test dependency path",
        full: "A value export is reachable from runtime entry points but no test-reachable module references it. This is a static test dependency gap rather than line coverage, and highlights exports exercised only through production entry paths.",
        docs_path: "explanations/health#coverage-gaps",
    },
    RuleDef {
        id: "fallow/runtime-safe-to-delete",
        name: "Production Safe To Delete",
        short: "Statically unused AND never invoked in production with V8 tracking",
        full: "The function is both statically unreachable in the module graph and was never invoked during the observed runtime coverage window. This is the highest-confidence delete signal fallow emits.",
        docs_path: "explanations/health#runtime-coverage",
    },
    RuleDef {
        id: "fallow/runtime-review-required",
        name: "Production Review Required",
        short: "Statically used but never invoked in production",
        full: "The function is reachable in the module graph (or exercised by tests / untracked call sites) but was not invoked during the observed runtime coverage window. Needs a human look — may be seasonal, error-path only, or legitimately unused.",
        docs_path: "explanations/health#runtime-coverage",
    },
    RuleDef {
        id: "fallow/runtime-low-traffic",
        name: "Production Low Traffic",
        short: "Function was invoked below the low-traffic threshold",
        full: "The function was invoked in production but below the configured `--low-traffic-threshold` fraction of total trace count (spec default 0.1%). Effectively dead for the current period.",
        docs_path: "explanations/health#runtime-coverage",
    },
    RuleDef {
        id: "fallow/runtime-coverage-unavailable",
        name: "Runtime Coverage Unavailable",
        short: "Runtime coverage could not be resolved for this function",
        full: "The function could not be matched to a V8-tracked coverage entry. Common causes: the function lives in a worker thread (separate V8 isolate), it is lazy-parsed and never reached the JIT tier, or its source map did not resolve to the expected source path. This is advisory, not a dead-code signal.",
        docs_path: "explanations/health#runtime-coverage",
    },
    RuleDef {
        id: "fallow/runtime-coverage",
        name: "Runtime Coverage",
        short: "Runtime coverage finding",
        full: "Generic runtime-coverage finding for verdicts not covered by a more specific rule. Covers the forward-compat `unknown` sentinel; the CLI filters `active` entries out of `runtime_coverage.findings` so the surfaced list stays actionable.",
        docs_path: "explanations/health#runtime-coverage",
    },
];

pub const DUPES_RULES: &[RuleDef] = &[RuleDef {
    id: "fallow/code-duplication",
    name: "Code Duplication",
    short: "Duplicated code block",
    full: "A block of code that appears in multiple locations with identical or near-identical token sequences. Clone detection uses normalized token comparison — identifier names and literals are abstracted away in non-strict modes.",
    docs_path: "explanations/duplication#clone-groups",
}];

// ── JSON _meta builders ─────────────────────────────────────────

/// Build the `_meta` object for `fallow dead-code --format json --explain`.
#[must_use]
pub fn check_meta() -> Value {
    let rules: Value = CHECK_RULES
        .iter()
        .map(|r| {
            (
                r.id.replace("fallow/", ""),
                json!({
                    "name": r.name,
                    "description": r.full,
                    "docs": rule_docs_url(r)
                }),
            )
        })
        .collect::<serde_json::Map<String, Value>>()
        .into();

    json!({
        "docs": CHECK_DOCS,
        "rules": rules
    })
}

/// Build the `_meta` object for `fallow health --format json --explain`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "flat metric table: every entry is 3-4 short lines of metadata and keeping them in one map is clearer than splitting into per-metric helpers"
)]
pub fn health_meta() -> Value {
    json!({
        "docs": HEALTH_DOCS,
        "metrics": {
            "cyclomatic": {
                "name": "Cyclomatic Complexity",
                "description": "McCabe cyclomatic complexity: 1 + number of decision points (if/else, switch cases, loops, ternary, logical operators). Measures the number of independent paths through a function.",
                "range": "[1, \u{221e})",
                "interpretation": "lower is better; default threshold: 20"
            },
            "cognitive": {
                "name": "Cognitive Complexity",
                "description": "SonarSource cognitive complexity: penalizes nesting depth and non-linear control flow (breaks, continues, early returns). Measures how hard a function is to understand when reading top-to-bottom.",
                "range": "[0, \u{221e})",
                "interpretation": "lower is better; default threshold: 15"
            },
            "line_count": {
                "name": "Function Line Count",
                "description": "Number of lines in the function body.",
                "range": "[1, \u{221e})",
                "interpretation": "context-dependent; long functions may need splitting"
            },
            "lines": {
                "name": "File Line Count",
                "description": "Total lines of code in the file (from line offsets). Provides scale context for other metrics: a file with 0.4 complexity density at 80 LOC is different from 0.4 density at 800 LOC.",
                "range": "[1, \u{221e})",
                "interpretation": "context-dependent; large files may benefit from splitting even if individual functions are small"
            },
            "maintainability_index": {
                "name": "Maintainability Index",
                "description": "Composite score: 100 - (complexity_density \u{00d7} 30 \u{00d7} dampening) - (dead_code_ratio \u{00d7} 20) - min(ln(fan_out+1) \u{00d7} 4, 15), where dampening = min(lines/50, 1.0). Clamped to [0, 100]. Higher is better.",
                "range": "[0, 100]",
                "interpretation": "higher is better; <40 poor, 40\u{2013}70 moderate, >70 good"
            },
            "complexity_density": {
                "name": "Complexity Density",
                "description": "Total cyclomatic complexity divided by lines of code. Measures how densely complex the code is per line.",
                "range": "[0, \u{221e})",
                "interpretation": "lower is better; >1.0 indicates very dense complexity"
            },
            "dead_code_ratio": {
                "name": "Dead Code Ratio",
                "description": "Fraction of value exports (excluding type-only exports like interfaces and type aliases) with zero references across the project.",
                "range": "[0, 1]",
                "interpretation": "lower is better; 0 = all exports are used"
            },
            "fan_in": {
                "name": "Fan-in (Importers)",
                "description": "Number of files that import this file. High fan-in means high blast radius \u{2014} changes to this file affect many dependents.",
                "range": "[0, \u{221e})",
                "interpretation": "context-dependent; high fan-in files need careful review before changes"
            },
            "fan_out": {
                "name": "Fan-out (Imports)",
                "description": "Number of files this file directly imports. High fan-out indicates high coupling and change propagation risk.",
                "range": "[0, \u{221e})",
                "interpretation": "lower is better; MI penalty caps at ~40 imports"
            },
            "score": {
                "name": "Hotspot Score",
                "description": "normalized_churn \u{00d7} normalized_complexity \u{00d7} 100, where normalization is against the project maximum. Identifies files that are both complex AND frequently changing.",
                "range": "[0, 100]",
                "interpretation": "higher = riskier; prioritize refactoring high-score files"
            },
            "weighted_commits": {
                "name": "Weighted Commits",
                "description": "Recency-weighted commit count using exponential decay with 90-day half-life. Recent commits contribute more than older ones.",
                "range": "[0, \u{221e})",
                "interpretation": "higher = more recent churn activity"
            },
            "trend": {
                "name": "Churn Trend",
                "description": "Compares recent vs older commit frequency within the analysis window. accelerating = recent > 1.5\u{00d7} older, cooling = recent < 0.67\u{00d7} older, stable = in between.",
                "values": ["accelerating", "stable", "cooling"],
                "interpretation": "accelerating files need attention; cooling files are stabilizing"
            },
            "priority": {
                "name": "Refactoring Priority",
                "description": "Weighted score: complexity density (30%), hotspot boost (25%), dead code ratio (20%), fan-in (15%), fan-out (10%). Fan-in and fan-out normalization uses adaptive percentile-based thresholds (p95 of the project distribution). Does not use the maintainability index to avoid double-counting.",
                "range": "[0, 100]",
                "interpretation": "higher = more urgent to refactor"
            },
            "efficiency": {
                "name": "Efficiency Score",
                "description": "priority / effort_numeric (Low=1, Medium=2, High=3). Surfaces quick wins: high-priority, low-effort targets rank first. Default sort order.",
                "range": "[0, 100] \u{2014} effective max depends on effort: Low=100, Medium=50, High\u{2248}33",
                "interpretation": "higher = better quick-win value; targets are sorted by efficiency descending"
            },
            "effort": {
                "name": "Effort Estimate",
                "description": "Heuristic effort estimate based on file size, function count, and fan-in. Thresholds adapt to the project\u{2019}s distribution (percentile-based). Low: small file, few functions, low fan-in. High: large file, high fan-in, or many functions with high density. Medium: everything else.",
                "values": ["low", "medium", "high"],
                "interpretation": "low = quick win, high = needs planning and coordination"
            },
            "confidence": {
                "name": "Confidence Level",
                "description": "Reliability of the recommendation based on data source. High: deterministic graph/AST analysis (dead code, circular deps, complexity). Medium: heuristic thresholds (fan-in/fan-out coupling). Low: depends on git history quality (churn-based recommendations).",
                "values": ["high", "medium", "low"],
                "interpretation": "high = act on it, medium = verify context, low = treat as a signal, not a directive"
            },
            "health_score": {
                "name": "Health Score",
                "description": "Project-level aggregate score computed from vital signs: dead code, complexity, maintainability, hotspots, unused dependencies, and circular dependencies. Penalties subtracted from 100. Missing metrics (from pipelines that didn't run) don't penalize. Use --score to force full pipeline for maximum accuracy.",
                "range": "[0, 100]",
                "interpretation": "higher is better; A (85\u{2013}100), B (70\u{2013}84), C (55\u{2013}69), D (40\u{2013}54), F (0\u{2013}39)"
            },
            "crap_max": {
                "name": "Untested Complexity Risk (CRAP)",
                "description": "Change Risk Anti-Patterns score (Savoia & Evans, 2007). Formula: CC\u{00b2} \u{00d7} (1 - cov/100)\u{00b3} + CC. Default model (static_estimated): estimates per-function coverage from export references \u{2014} directly test-referenced exports get 85%, indirectly test-reachable functions get 40%, untested files get 0%. Provide --coverage <path> with Istanbul-format coverage-final.json (from Jest, Vitest, c8, nyc) for exact per-function CRAP scores.",
                "range": "[1, \u{221e})",
                "interpretation": "lower is better; >=30 is high-risk (CC >= 5 without test path)"
            },
            "bus_factor": {
                "name": "Bus Factor",
                "description": "Avelino truck factor: the minimum number of distinct contributors who together account for at least 50% of recency-weighted commits to this file in the analysis window. Bot authors are excluded.",
                "range": "[1, \u{221e})",
                "interpretation": "lower is higher knowledge-loss risk; 1 means a single contributor covers most of the recent history"
            },
            "contributor_count": {
                "name": "Contributor Count",
                "description": "Number of distinct authors who touched this file in the analysis window after bot-pattern filtering.",
                "range": "[0, \u{221e})",
                "interpretation": "higher generally indicates broader knowledge spread; pair with bus_factor for context"
            },
            "share": {
                "name": "Contributor Share",
                "description": "Recency-weighted share of total weighted commits attributed to a single contributor. Rounded to three decimals.",
                "range": "[0, 1]",
                "interpretation": "share close to 1.0 indicates dominance and pairs with low bus_factor"
            },
            "stale_days": {
                "name": "Stale Days",
                "description": "Days since this contributor last touched the file. Computed at analysis time.",
                "range": "[0, \u{221e})",
                "interpretation": "high stale_days on the top contributor often correlates with ownership drift"
            },
            "drift": {
                "name": "Ownership Drift",
                "description": "True when the file's original author (earliest first commit in the window) differs from the current top contributor, the file is at least 30 days old, and the original author's recency-weighted share is below 10%.",
                "values": [true, false],
                "interpretation": "true means the original author is no longer maintaining; route reviews to the current top contributor"
            },
            "unowned": {
                "name": "Unowned (Tristate)",
                "description": "true = a CODEOWNERS file exists but no rule matches this file; false = a rule matches; null = no CODEOWNERS file was discovered for the repository (cannot determine).",
                "values": [true, false, null],
                "interpretation": "true on a hotspot is a review-bottleneck risk; null means the signal is unavailable, not absent"
            },
            "runtime_coverage_verdict": {
                "name": "Runtime Coverage Verdict",
                "description": "Overall verdict across all runtime-coverage findings. `clean` = nothing cold; `cold-code-detected` = one or more tracked functions had zero invocations; `hot-path-changes-needed` = a function modified in the current change set is on the hot path; `license-expired-grace` = analysis ran but the license is in its post-expiry grace window; `unknown` = verdict could not be computed (degenerate input).",
                "values": ["clean", "hot-path-changes-needed", "cold-code-detected", "license-expired-grace", "unknown"],
                "interpretation": "`cold-code-detected` is the primary actionable signal; `hot-path-changes-needed` elevates code-review attention for touched hot paths"
            },
            "runtime_coverage_state": {
                "name": "Runtime Coverage State",
                "description": "Per-function observation: `called` = V8 saw at least one invocation; `never-called` = V8 tracked the function but it never ran; `coverage-unavailable` = the function was not in the V8 tracking set (e.g., lazy-parsed, worker thread, dynamic code); `unknown` = forward-compat sentinel for newer sidecar states.",
                "values": ["called", "never-called", "coverage-unavailable", "unknown"],
                "interpretation": "`never-called` in combination with static `unused` is the highest-confidence delete signal"
            },
            "runtime_coverage_confidence": {
                "name": "Runtime Coverage Confidence",
                "description": "Confidence in a runtime-coverage finding. `high` = tracked by V8 with a statistically meaningful observation volume; `medium` = either low observation volume or indirect evidence; `low` = minimal data; `unknown` = insufficient information to classify.",
                "values": ["high", "medium", "low", "unknown"],
                "interpretation": "high = act on it; medium = verify context; low = treat as a signal only"
            },
            "production_invocations": {
                "name": "Production Invocations",
                "description": "Observed invocation count for the function over the collected coverage window. For `coverage-unavailable` findings this is `0` and semantically means `null` (not tracked). Absolute counts are not directly comparable across services without normalizing by trace_count.",
                "range": "[0, \u{221e})",
                "interpretation": "0 + tracked = cold path; 0 + untracked = unknown; high + never-called cannot occur by definition"
            },
            "percent_dead_in_production": {
                "name": "Percent Dead in Production",
                "description": "Fraction of tracked functions with zero observed invocations, multiplied by 100. Computed before any `--top` truncation so the summary total is stable regardless of display limits.",
                "range": "[0, 100]",
                "interpretation": "lower is better; values above ~10% on a long-running service indicate a large cleanup opportunity"
            }
        }
    })
}

/// Build the `_meta` object for `fallow dupes --format json --explain`.
#[must_use]
pub fn dupes_meta() -> Value {
    json!({
        "docs": DUPES_DOCS,
        "metrics": {
            "duplication_percentage": {
                "name": "Duplication Percentage",
                "description": "Fraction of total source tokens that appear in at least one clone group. Computed over the full analyzed file set.",
                "range": "[0, 100]",
                "interpretation": "lower is better"
            },
            "token_count": {
                "name": "Token Count",
                "description": "Number of normalized source tokens in the clone group. Tokens are language-aware (keywords, identifiers, operators, punctuation). Higher token count = larger duplicate.",
                "range": "[1, \u{221e})",
                "interpretation": "larger clones have higher refactoring value"
            },
            "line_count": {
                "name": "Line Count",
                "description": "Number of source lines spanned by the clone instance. Approximation of clone size for human readability.",
                "range": "[1, \u{221e})",
                "interpretation": "larger clones are more impactful to deduplicate"
            },
            "clone_groups": {
                "name": "Clone Groups",
                "description": "A set of code fragments with identical or near-identical normalized token sequences. Each group has 2+ instances across different locations.",
                "interpretation": "each group is a single refactoring opportunity"
            },
            "clone_families": {
                "name": "Clone Families",
                "description": "Groups of clone groups that share the same set of files. Indicates systematic duplication patterns (e.g., mirrored directory structures).",
                "interpretation": "families suggest extract-module refactoring opportunities"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── rule_by_id ───────────────────────────────────────────────────

    #[test]
    fn rule_by_id_finds_check_rule() {
        let rule = rule_by_id("fallow/unused-file").unwrap();
        assert_eq!(rule.name, "Unused Files");
    }

    #[test]
    fn rule_by_id_finds_health_rule() {
        let rule = rule_by_id("fallow/high-cyclomatic-complexity").unwrap();
        assert_eq!(rule.name, "High Cyclomatic Complexity");
    }

    #[test]
    fn rule_by_id_finds_dupes_rule() {
        let rule = rule_by_id("fallow/code-duplication").unwrap();
        assert_eq!(rule.name, "Code Duplication");
    }

    #[test]
    fn rule_by_id_returns_none_for_unknown() {
        assert!(rule_by_id("fallow/nonexistent").is_none());
        assert!(rule_by_id("").is_none());
    }

    // ── rule_docs_url ────────────────────────────────────────────────

    #[test]
    fn rule_docs_url_format() {
        let rule = rule_by_id("fallow/unused-export").unwrap();
        let url = rule_docs_url(rule);
        assert!(url.starts_with("https://docs.fallow.tools/"));
        assert!(url.contains("unused-exports"));
    }

    // ── CHECK_RULES completeness ─────────────────────────────────────

    #[test]
    fn check_rules_all_have_fallow_prefix() {
        for rule in CHECK_RULES {
            assert!(
                rule.id.starts_with("fallow/"),
                "rule {} should start with fallow/",
                rule.id
            );
        }
    }

    #[test]
    fn check_rules_all_have_docs_path() {
        for rule in CHECK_RULES {
            assert!(
                !rule.docs_path.is_empty(),
                "rule {} should have a docs_path",
                rule.id
            );
        }
    }

    #[test]
    fn check_rules_no_duplicate_ids() {
        let mut seen = rustc_hash::FxHashSet::default();
        for rule in CHECK_RULES.iter().chain(HEALTH_RULES).chain(DUPES_RULES) {
            assert!(seen.insert(rule.id), "duplicate rule id: {}", rule.id);
        }
    }

    // ── check_meta ───────────────────────────────────────────────────

    #[test]
    fn check_meta_has_docs_and_rules() {
        let meta = check_meta();
        assert!(meta.get("docs").is_some());
        assert!(meta.get("rules").is_some());
        let rules = meta["rules"].as_object().unwrap();
        // Verify all 13 rule categories are present (stripped fallow/ prefix)
        assert_eq!(rules.len(), CHECK_RULES.len());
        assert!(rules.contains_key("unused-file"));
        assert!(rules.contains_key("unused-export"));
        assert!(rules.contains_key("unused-type"));
        assert!(rules.contains_key("unused-dependency"));
        assert!(rules.contains_key("unused-dev-dependency"));
        assert!(rules.contains_key("unused-optional-dependency"));
        assert!(rules.contains_key("unused-enum-member"));
        assert!(rules.contains_key("unused-class-member"));
        assert!(rules.contains_key("unresolved-import"));
        assert!(rules.contains_key("unlisted-dependency"));
        assert!(rules.contains_key("duplicate-export"));
        assert!(rules.contains_key("type-only-dependency"));
        assert!(rules.contains_key("circular-dependency"));
    }

    #[test]
    fn check_meta_rule_has_required_fields() {
        let meta = check_meta();
        let rules = meta["rules"].as_object().unwrap();
        for (key, value) in rules {
            assert!(value.get("name").is_some(), "rule {key} missing 'name'");
            assert!(
                value.get("description").is_some(),
                "rule {key} missing 'description'"
            );
            assert!(value.get("docs").is_some(), "rule {key} missing 'docs'");
        }
    }

    // ── health_meta ──────────────────────────────────────────────────

    #[test]
    fn health_meta_has_metrics() {
        let meta = health_meta();
        assert!(meta.get("docs").is_some());
        let metrics = meta["metrics"].as_object().unwrap();
        assert!(metrics.contains_key("cyclomatic"));
        assert!(metrics.contains_key("cognitive"));
        assert!(metrics.contains_key("maintainability_index"));
        assert!(metrics.contains_key("complexity_density"));
        assert!(metrics.contains_key("fan_in"));
        assert!(metrics.contains_key("fan_out"));
    }

    // ── dupes_meta ───────────────────────────────────────────────────

    #[test]
    fn dupes_meta_has_metrics() {
        let meta = dupes_meta();
        assert!(meta.get("docs").is_some());
        let metrics = meta["metrics"].as_object().unwrap();
        assert!(metrics.contains_key("duplication_percentage"));
        assert!(metrics.contains_key("token_count"));
        assert!(metrics.contains_key("clone_groups"));
        assert!(metrics.contains_key("clone_families"));
    }

    // ── HEALTH_RULES completeness ──────────────────────────────────

    #[test]
    fn health_rules_all_have_fallow_prefix() {
        for rule in HEALTH_RULES {
            assert!(
                rule.id.starts_with("fallow/"),
                "health rule {} should start with fallow/",
                rule.id
            );
        }
    }

    #[test]
    fn health_rules_all_have_docs_path() {
        for rule in HEALTH_RULES {
            assert!(
                !rule.docs_path.is_empty(),
                "health rule {} should have a docs_path",
                rule.id
            );
        }
    }

    #[test]
    fn health_rules_all_have_non_empty_fields() {
        for rule in HEALTH_RULES {
            assert!(
                !rule.name.is_empty(),
                "health rule {} missing name",
                rule.id
            );
            assert!(
                !rule.short.is_empty(),
                "health rule {} missing short description",
                rule.id
            );
            assert!(
                !rule.full.is_empty(),
                "health rule {} missing full description",
                rule.id
            );
        }
    }

    // ── DUPES_RULES completeness ───────────────────────────────────

    #[test]
    fn dupes_rules_all_have_fallow_prefix() {
        for rule in DUPES_RULES {
            assert!(
                rule.id.starts_with("fallow/"),
                "dupes rule {} should start with fallow/",
                rule.id
            );
        }
    }

    #[test]
    fn dupes_rules_all_have_docs_path() {
        for rule in DUPES_RULES {
            assert!(
                !rule.docs_path.is_empty(),
                "dupes rule {} should have a docs_path",
                rule.id
            );
        }
    }

    #[test]
    fn dupes_rules_all_have_non_empty_fields() {
        for rule in DUPES_RULES {
            assert!(!rule.name.is_empty(), "dupes rule {} missing name", rule.id);
            assert!(
                !rule.short.is_empty(),
                "dupes rule {} missing short description",
                rule.id
            );
            assert!(
                !rule.full.is_empty(),
                "dupes rule {} missing full description",
                rule.id
            );
        }
    }

    // ── CHECK_RULES field completeness ─────────────────────────────

    #[test]
    fn check_rules_all_have_non_empty_fields() {
        for rule in CHECK_RULES {
            assert!(!rule.name.is_empty(), "check rule {} missing name", rule.id);
            assert!(
                !rule.short.is_empty(),
                "check rule {} missing short description",
                rule.id
            );
            assert!(
                !rule.full.is_empty(),
                "check rule {} missing full description",
                rule.id
            );
        }
    }

    // ── rule_docs_url with health/dupes rules ──────────────────────

    #[test]
    fn rule_docs_url_health_rule() {
        let rule = rule_by_id("fallow/high-cyclomatic-complexity").unwrap();
        let url = rule_docs_url(rule);
        assert!(url.starts_with("https://docs.fallow.tools/"));
        assert!(url.contains("health"));
    }

    #[test]
    fn rule_docs_url_dupes_rule() {
        let rule = rule_by_id("fallow/code-duplication").unwrap();
        let url = rule_docs_url(rule);
        assert!(url.starts_with("https://docs.fallow.tools/"));
        assert!(url.contains("duplication"));
    }

    // ── health_meta metric structure ───────────────────────────────

    #[test]
    fn health_meta_all_metrics_have_name_and_description() {
        let meta = health_meta();
        let metrics = meta["metrics"].as_object().unwrap();
        for (key, value) in metrics {
            assert!(
                value.get("name").is_some(),
                "health metric {key} missing 'name'"
            );
            assert!(
                value.get("description").is_some(),
                "health metric {key} missing 'description'"
            );
            assert!(
                value.get("interpretation").is_some(),
                "health metric {key} missing 'interpretation'"
            );
        }
    }

    #[test]
    fn health_meta_has_all_expected_metrics() {
        let meta = health_meta();
        let metrics = meta["metrics"].as_object().unwrap();
        let expected = [
            "cyclomatic",
            "cognitive",
            "line_count",
            "lines",
            "maintainability_index",
            "complexity_density",
            "dead_code_ratio",
            "fan_in",
            "fan_out",
            "score",
            "weighted_commits",
            "trend",
            "priority",
            "efficiency",
            "effort",
            "confidence",
            "bus_factor",
            "contributor_count",
            "share",
            "stale_days",
            "drift",
            "unowned",
            "runtime_coverage_verdict",
            "runtime_coverage_state",
            "runtime_coverage_confidence",
            "production_invocations",
            "percent_dead_in_production",
        ];
        for key in &expected {
            assert!(
                metrics.contains_key(*key),
                "health_meta missing expected metric: {key}"
            );
        }
    }

    // ── dupes_meta metric structure ────────────────────────────────

    #[test]
    fn dupes_meta_all_metrics_have_name_and_description() {
        let meta = dupes_meta();
        let metrics = meta["metrics"].as_object().unwrap();
        for (key, value) in metrics {
            assert!(
                value.get("name").is_some(),
                "dupes metric {key} missing 'name'"
            );
            assert!(
                value.get("description").is_some(),
                "dupes metric {key} missing 'description'"
            );
        }
    }

    #[test]
    fn dupes_meta_has_line_count() {
        let meta = dupes_meta();
        let metrics = meta["metrics"].as_object().unwrap();
        assert!(metrics.contains_key("line_count"));
    }

    // ── docs URLs ─────────────────────────────────────────────────

    #[test]
    fn check_docs_url_valid() {
        assert!(CHECK_DOCS.starts_with("https://"));
        assert!(CHECK_DOCS.contains("dead-code"));
    }

    #[test]
    fn health_docs_url_valid() {
        assert!(HEALTH_DOCS.starts_with("https://"));
        assert!(HEALTH_DOCS.contains("health"));
    }

    #[test]
    fn dupes_docs_url_valid() {
        assert!(DUPES_DOCS.starts_with("https://"));
        assert!(DUPES_DOCS.contains("dupes"));
    }

    // ── check_meta docs URL matches constant ──────────────────────

    #[test]
    fn check_meta_docs_url_matches_constant() {
        let meta = check_meta();
        assert_eq!(meta["docs"].as_str().unwrap(), CHECK_DOCS);
    }

    #[test]
    fn health_meta_docs_url_matches_constant() {
        let meta = health_meta();
        assert_eq!(meta["docs"].as_str().unwrap(), HEALTH_DOCS);
    }

    #[test]
    fn dupes_meta_docs_url_matches_constant() {
        let meta = dupes_meta();
        assert_eq!(meta["docs"].as_str().unwrap(), DUPES_DOCS);
    }

    // ── rule_by_id finds all check rules ──────────────────────────

    #[test]
    fn rule_by_id_finds_all_check_rules() {
        for rule in CHECK_RULES {
            assert!(
                rule_by_id(rule.id).is_some(),
                "rule_by_id should find check rule {}",
                rule.id
            );
        }
    }

    #[test]
    fn rule_by_id_finds_all_health_rules() {
        for rule in HEALTH_RULES {
            assert!(
                rule_by_id(rule.id).is_some(),
                "rule_by_id should find health rule {}",
                rule.id
            );
        }
    }

    #[test]
    fn rule_by_id_finds_all_dupes_rules() {
        for rule in DUPES_RULES {
            assert!(
                rule_by_id(rule.id).is_some(),
                "rule_by_id should find dupes rule {}",
                rule.id
            );
        }
    }

    // ── Rule count verification ───────────────────────────────────

    #[test]
    fn check_rules_count() {
        assert_eq!(CHECK_RULES.len(), 15);
    }

    #[test]
    fn health_rules_count() {
        assert_eq!(HEALTH_RULES.len(), 12);
    }

    #[test]
    fn dupes_rules_count() {
        assert_eq!(DUPES_RULES.len(), 1);
    }
}
