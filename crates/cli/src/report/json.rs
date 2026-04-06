use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use fallow_core::duplicates::DuplicationReport;
use fallow_core::results::AnalysisResults;

use super::emit_json;
use crate::explain;
use crate::report::grouping::{OwnershipResolver, ResultGroup};

pub(super) fn print_json(
    results: &AnalysisResults,
    root: &Path,
    elapsed: Duration,
    explain: bool,
    regression: Option<&crate::regression::RegressionOutcome>,
) -> ExitCode {
    match build_json(results, root, elapsed) {
        Ok(mut output) => {
            if let Some(outcome) = regression
                && let serde_json::Value::Object(ref mut map) = output
            {
                map.insert("regression".to_string(), outcome.to_json());
            }
            if explain {
                insert_meta(&mut output, explain::check_meta());
            }
            emit_json(&output, "JSON")
        }
        Err(e) => {
            eprintln!("Error: failed to serialize results: {e}");
            ExitCode::from(2)
        }
    }
}

/// Render grouped analysis results as a single JSON document.
///
/// Produces an envelope with `grouped_by` and `total_issues` at the top level,
/// then a `groups` array where each element contains the group `key`,
/// `total_issues`, and all the normal result fields with paths relativized.
#[must_use]
pub(super) fn print_grouped_json(
    groups: &[ResultGroup],
    original: &AnalysisResults,
    root: &Path,
    elapsed: Duration,
    explain: bool,
    resolver: &OwnershipResolver,
) -> ExitCode {
    let root_prefix = format!("{}/", root.display());

    let group_values: Vec<serde_json::Value> = groups
        .iter()
        .filter_map(|group| {
            let mut value = serde_json::to_value(&group.results).ok()?;
            strip_root_prefix(&mut value, &root_prefix);
            inject_actions(&mut value);

            if let serde_json::Value::Object(ref mut map) = value {
                // Insert key and total_issues at the front by rebuilding the map
                let mut ordered = serde_json::Map::new();
                ordered.insert("key".to_string(), serde_json::json!(group.key));
                ordered.insert(
                    "total_issues".to_string(),
                    serde_json::json!(group.results.total_issues()),
                );
                for (k, v) in map.iter() {
                    ordered.insert(k.clone(), v.clone());
                }
                Some(serde_json::Value::Object(ordered))
            } else {
                Some(value)
            }
        })
        .collect();

    let mut output = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "elapsed_ms": elapsed.as_millis() as u64,
        "grouped_by": resolver.mode_label(),
        "total_issues": original.total_issues(),
        "groups": group_values,
    });

    if explain {
        insert_meta(&mut output, explain::check_meta());
    }

    emit_json(&output, "JSON")
}

/// JSON output schema version as an integer (independent of tool version).
///
/// Bump this when the structure of the JSON output changes in a
/// backwards-incompatible way (removing/renaming fields, changing types).
/// Adding new fields is always backwards-compatible and does not require a bump.
const SCHEMA_VERSION: u32 = 3;

/// Build a JSON envelope with standard metadata fields at the top.
///
/// Creates a JSON object with `schema_version`, `version`, and `elapsed_ms`,
/// then merges all fields from `report_value` into the envelope.
/// Fields from `report_value` appear after the metadata header.
fn build_json_envelope(report_value: serde_json::Value, elapsed: Duration) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert(
        "schema_version".to_string(),
        serde_json::json!(SCHEMA_VERSION),
    );
    map.insert(
        "version".to_string(),
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    map.insert(
        "elapsed_ms".to_string(),
        serde_json::json!(elapsed.as_millis()),
    );
    if let serde_json::Value::Object(report_map) = report_value {
        for (key, value) in report_map {
            map.insert(key, value);
        }
    }
    serde_json::Value::Object(map)
}

/// Build the JSON output value for analysis results.
///
/// Metadata fields (`schema_version`, `version`, `elapsed_ms`, `total_issues`)
/// appear first in the output for readability. Paths are made relative to `root`.
///
/// # Errors
///
/// Returns an error if the results cannot be serialized to JSON.
pub fn build_json(
    results: &AnalysisResults,
    root: &Path,
    elapsed: Duration,
) -> Result<serde_json::Value, serde_json::Error> {
    let results_value = serde_json::to_value(results)?;

    let mut map = serde_json::Map::new();
    map.insert(
        "schema_version".to_string(),
        serde_json::json!(SCHEMA_VERSION),
    );
    map.insert(
        "version".to_string(),
        serde_json::json!(env!("CARGO_PKG_VERSION")),
    );
    map.insert(
        "elapsed_ms".to_string(),
        serde_json::json!(elapsed.as_millis()),
    );
    map.insert(
        "total_issues".to_string(),
        serde_json::json!(results.total_issues()),
    );

    // Entry-point detection summary (metadata, not serialized via serde)
    if let Some(ref ep) = results.entry_point_summary {
        let sources: serde_json::Map<String, serde_json::Value> = ep
            .by_source
            .iter()
            .map(|(k, v)| (k.replace(' ', "_"), serde_json::json!(v)))
            .collect();
        map.insert(
            "entry_points".to_string(),
            serde_json::json!({
                "total": ep.total,
                "sources": sources,
            }),
        );
    }

    // Per-category summary counts for CI dashboard consumption
    let summary = serde_json::json!({
        "total_issues": results.total_issues(),
        "unused_files": results.unused_files.len(),
        "unused_exports": results.unused_exports.len(),
        "unused_types": results.unused_types.len(),
        "unused_dependencies": results.unused_dependencies.len()
            + results.unused_dev_dependencies.len()
            + results.unused_optional_dependencies.len(),
        "unused_enum_members": results.unused_enum_members.len(),
        "unused_class_members": results.unused_class_members.len(),
        "unresolved_imports": results.unresolved_imports.len(),
        "unlisted_dependencies": results.unlisted_dependencies.len(),
        "duplicate_exports": results.duplicate_exports.len(),
        "type_only_dependencies": results.type_only_dependencies.len(),
        "test_only_dependencies": results.test_only_dependencies.len(),
        "circular_dependencies": results.circular_dependencies.len(),
        "boundary_violations": results.boundary_violations.len(),
    });
    map.insert("summary".to_string(), summary);

    if let serde_json::Value::Object(results_map) = results_value {
        for (key, value) in results_map {
            map.insert(key, value);
        }
    }

    let mut output = serde_json::Value::Object(map);
    let root_prefix = format!("{}/", root.display());
    // strip_root_prefix must run before inject_actions so that injected
    // action fields (static strings and package names) are not processed
    // by the path stripper.
    strip_root_prefix(&mut output, &root_prefix);
    inject_actions(&mut output);
    Ok(output)
}

/// Recursively strip the root prefix from all string values in the JSON tree.
///
/// This converts absolute paths (e.g., `/home/runner/work/repo/repo/src/utils.ts`)
/// to relative paths (`src/utils.ts`) for all output fields.
fn strip_root_prefix(value: &mut serde_json::Value, prefix: &str) {
    match value {
        serde_json::Value::String(s) => {
            if let Some(rest) = s.strip_prefix(prefix) {
                *s = rest.to_string();
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                strip_root_prefix(item, prefix);
            }
        }
        serde_json::Value::Object(map) => {
            for (_, v) in map.iter_mut() {
                strip_root_prefix(v, prefix);
            }
        }
        _ => {}
    }
}

// ── Fix action injection ────────────────────────────────────────

/// Suppress mechanism for an issue type.
enum SuppressKind {
    /// `// fallow-ignore-next-line <type>` on the line before.
    InlineComment,
    /// `// fallow-ignore-file <type>` at the top of the file.
    FileComment,
    /// Add to `ignoreDependencies` in fallow config.
    ConfigIgnoreDep,
}

/// Specification for actions to inject per issue type.
struct ActionSpec {
    fix_type: &'static str,
    auto_fixable: bool,
    description: &'static str,
    note: Option<&'static str>,
    suppress: SuppressKind,
    issue_kind: &'static str,
}

/// Map an issue array key to its action specification.
fn actions_for_issue_type(key: &str) -> Option<ActionSpec> {
    match key {
        "unused_files" => Some(ActionSpec {
            fix_type: "delete-file",
            auto_fixable: false,
            description: "Delete this file",
            note: Some(
                "File deletion may remove runtime functionality not visible to static analysis",
            ),
            suppress: SuppressKind::FileComment,
            issue_kind: "unused-file",
        }),
        "unused_exports" => Some(ActionSpec {
            fix_type: "remove-export",
            auto_fixable: true,
            description: "Remove the `export` keyword from the declaration",
            note: None,
            suppress: SuppressKind::InlineComment,
            issue_kind: "unused-export",
        }),
        "unused_types" => Some(ActionSpec {
            fix_type: "remove-export",
            auto_fixable: true,
            description: "Remove the `export` (or `export type`) keyword from the type declaration",
            note: None,
            suppress: SuppressKind::InlineComment,
            issue_kind: "unused-type",
        }),
        "unused_dependencies" => Some(ActionSpec {
            fix_type: "remove-dependency",
            auto_fixable: true,
            description: "Remove from dependencies in package.json",
            note: None,
            suppress: SuppressKind::ConfigIgnoreDep,
            issue_kind: "unused-dependency",
        }),
        "unused_dev_dependencies" => Some(ActionSpec {
            fix_type: "remove-dependency",
            auto_fixable: true,
            description: "Remove from devDependencies in package.json",
            note: None,
            suppress: SuppressKind::ConfigIgnoreDep,
            issue_kind: "unused-dev-dependency",
        }),
        "unused_optional_dependencies" => Some(ActionSpec {
            fix_type: "remove-dependency",
            auto_fixable: true,
            description: "Remove from optionalDependencies in package.json",
            note: None,
            suppress: SuppressKind::ConfigIgnoreDep,
            // No IssueKind variant exists for optional deps — uses config suppress only.
            issue_kind: "unused-dependency",
        }),
        "unused_enum_members" => Some(ActionSpec {
            fix_type: "remove-enum-member",
            auto_fixable: true,
            description: "Remove this enum member",
            note: None,
            suppress: SuppressKind::InlineComment,
            issue_kind: "unused-enum-member",
        }),
        "unused_class_members" => Some(ActionSpec {
            fix_type: "remove-class-member",
            auto_fixable: false,
            description: "Remove this class member",
            note: Some("Class member may be used via dependency injection or decorators"),
            suppress: SuppressKind::InlineComment,
            issue_kind: "unused-class-member",
        }),
        "unresolved_imports" => Some(ActionSpec {
            fix_type: "resolve-import",
            auto_fixable: false,
            description: "Fix the import specifier or install the missing module",
            note: Some("Verify the module path and check tsconfig paths configuration"),
            suppress: SuppressKind::InlineComment,
            issue_kind: "unresolved-import",
        }),
        "unlisted_dependencies" => Some(ActionSpec {
            fix_type: "install-dependency",
            auto_fixable: false,
            description: "Add this package to dependencies in package.json",
            note: Some("Verify this package should be a direct dependency before adding"),
            suppress: SuppressKind::ConfigIgnoreDep,
            issue_kind: "unlisted-dependency",
        }),
        "duplicate_exports" => Some(ActionSpec {
            fix_type: "remove-duplicate",
            auto_fixable: false,
            description: "Keep one canonical export location and remove the others",
            note: Some("Review all locations to determine which should be the canonical export"),
            suppress: SuppressKind::InlineComment,
            issue_kind: "duplicate-export",
        }),
        "type_only_dependencies" => Some(ActionSpec {
            fix_type: "move-to-dev",
            auto_fixable: false,
            description: "Move to devDependencies (only type imports are used)",
            note: Some(
                "Type imports are erased at runtime so this dependency is not needed in production",
            ),
            suppress: SuppressKind::ConfigIgnoreDep,
            issue_kind: "type-only-dependency",
        }),
        "test_only_dependencies" => Some(ActionSpec {
            fix_type: "move-to-dev",
            auto_fixable: false,
            description: "Move to devDependencies (only test files import this)",
            note: Some(
                "Only test files import this package so it does not need to be a production dependency",
            ),
            suppress: SuppressKind::ConfigIgnoreDep,
            issue_kind: "test-only-dependency",
        }),
        "circular_dependencies" => Some(ActionSpec {
            fix_type: "refactor-cycle",
            auto_fixable: false,
            description: "Extract shared logic into a separate module to break the cycle",
            note: Some(
                "Circular imports can cause initialization issues and make code harder to reason about",
            ),
            suppress: SuppressKind::InlineComment,
            issue_kind: "circular-dependency",
        }),
        "boundary_violations" => Some(ActionSpec {
            fix_type: "refactor-boundary",
            auto_fixable: false,
            description: "Move the import through an allowed zone or restructure the dependency",
            note: Some(
                "This import crosses an architecture boundary that is not permitted by the configured rules",
            ),
            suppress: SuppressKind::InlineComment,
            issue_kind: "boundary-violation",
        }),
        _ => None,
    }
}

/// Build the `actions` array for a single issue item.
fn build_actions(
    item: &serde_json::Value,
    issue_key: &str,
    spec: &ActionSpec,
) -> serde_json::Value {
    let mut actions = Vec::with_capacity(2);

    // Primary fix action
    let mut fix_action = serde_json::json!({
        "type": spec.fix_type,
        "auto_fixable": spec.auto_fixable,
        "description": spec.description,
    });
    if let Some(note) = spec.note {
        fix_action["note"] = serde_json::json!(note);
    }
    // Warn about re-exports that may be part of the public API surface.
    if (issue_key == "unused_exports" || issue_key == "unused_types")
        && item
            .get("is_re_export")
            .and_then(serde_json::Value::as_bool)
            == Some(true)
    {
        fix_action["note"] = serde_json::json!(
            "This finding originates from a re-export; verify it is not part of your public API before removing"
        );
    }
    actions.push(fix_action);

    // Suppress action — every action carries `auto_fixable` for uniform filtering.
    match spec.suppress {
        SuppressKind::InlineComment => {
            let mut suppress = serde_json::json!({
                "type": "suppress-line",
                "auto_fixable": false,
                "description": "Suppress with an inline comment above the line",
                "comment": format!("// fallow-ignore-next-line {}", spec.issue_kind),
            });
            // duplicate_exports has N locations, not one — flag multi-location scope.
            if issue_key == "duplicate_exports" {
                suppress["scope"] = serde_json::json!("per-location");
            }
            actions.push(suppress);
        }
        SuppressKind::FileComment => {
            actions.push(serde_json::json!({
                "type": "suppress-file",
                "auto_fixable": false,
                "description": "Suppress with a file-level comment at the top of the file",
                "comment": format!("// fallow-ignore-file {}", spec.issue_kind),
            }));
        }
        SuppressKind::ConfigIgnoreDep => {
            // Extract the package name from the item for a concrete suggestion.
            let pkg = item
                .get("package_name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("package-name");
            actions.push(serde_json::json!({
                "type": "add-to-config",
                "auto_fixable": false,
                "description": format!("Add \"{pkg}\" to ignoreDependencies in fallow config"),
                "config_key": "ignoreDependencies",
                "value": pkg,
            }));
        }
    }

    serde_json::Value::Array(actions)
}

/// Inject `actions` arrays into every issue item in the JSON output.
///
/// Walks each known issue-type array and appends an `actions` field
/// to every item, providing machine-actionable fix and suppress hints.
fn inject_actions(output: &mut serde_json::Value) {
    let Some(map) = output.as_object_mut() else {
        return;
    };

    for (key, value) in map.iter_mut() {
        let Some(spec) = actions_for_issue_type(key) else {
            continue;
        };
        let Some(arr) = value.as_array_mut() else {
            continue;
        };
        for item in arr {
            let actions = build_actions(item, key, &spec);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }
}

// ── Health action injection ─────────────────────────────────────

/// Build a JSON representation of baseline deltas for the combined JSON envelope.
///
/// Accepts a total delta and an iterator of per-category entries to avoid
/// coupling the report module (compiled in both lib and bin) to the
/// binary-only `baseline` module.
pub fn build_baseline_deltas_json<'a>(
    total_delta: i64,
    per_category: impl Iterator<Item = (&'a str, usize, usize, i64)>,
) -> serde_json::Value {
    let mut per_cat = serde_json::Map::new();
    for (cat, current, baseline, delta) in per_category {
        per_cat.insert(
            cat.to_string(),
            serde_json::json!({
                "current": current,
                "baseline": baseline,
                "delta": delta,
            }),
        );
    }
    serde_json::json!({
        "total_delta": total_delta,
        "per_category": per_cat
    })
}

/// Inject `actions` arrays into complexity findings in a health JSON output.
///
/// Walks `findings` and `targets` arrays, appending machine-actionable
/// fix and suppress hints to each item.
#[allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) needed — used by audit.rs via re-export, but not part of public API"
)]
pub(crate) fn inject_health_actions(output: &mut serde_json::Value) {
    let Some(map) = output.as_object_mut() else {
        return;
    };

    // Complexity findings: refactor the function to reduce complexity
    if let Some(findings) = map.get_mut("findings").and_then(|v| v.as_array_mut()) {
        for item in findings {
            let actions = build_health_finding_actions(item);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }

    // Refactoring targets: apply the recommended refactoring
    if let Some(targets) = map.get_mut("targets").and_then(|v| v.as_array_mut()) {
        for item in targets {
            let actions = build_refactoring_target_actions(item);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }

    // Hotspots: files that are both complex and frequently changing
    if let Some(hotspots) = map.get_mut("hotspots").and_then(|v| v.as_array_mut()) {
        for item in hotspots {
            let actions = build_hotspot_actions(item);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }

    // Coverage gaps: untested files and exports
    if let Some(gaps) = map.get_mut("coverage_gaps").and_then(|v| v.as_object_mut()) {
        if let Some(files) = gaps.get_mut("files").and_then(|v| v.as_array_mut()) {
            for item in files {
                let actions = build_untested_file_actions(item);
                if let serde_json::Value::Object(obj) = item {
                    obj.insert("actions".to_string(), actions);
                }
            }
        }
        if let Some(exports) = gaps.get_mut("exports").and_then(|v| v.as_array_mut()) {
            for item in exports {
                let actions = build_untested_export_actions(item);
                if let serde_json::Value::Object(obj) = item {
                    obj.insert("actions".to_string(), actions);
                }
            }
        }
    }
}

/// Build the `actions` array for a single complexity finding.
fn build_health_finding_actions(item: &serde_json::Value) -> serde_json::Value {
    let name = item
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("function");

    let mut actions = vec![serde_json::json!({
        "type": "refactor-function",
        "auto_fixable": false,
        "description": format!("Refactor `{name}` to reduce complexity (extract helper functions, simplify branching)"),
        "note": "Consider splitting into smaller functions with single responsibilities",
    })];

    actions.push(serde_json::json!({
        "type": "suppress-line",
        "auto_fixable": false,
        "description": "Suppress with an inline comment above the function declaration",
        "comment": "// fallow-ignore-next-line complexity",
        "placement": "above-function-declaration",
    }));

    serde_json::Value::Array(actions)
}

/// Build the `actions` array for a single hotspot entry.
fn build_hotspot_actions(item: &serde_json::Value) -> serde_json::Value {
    let path = item
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("file");

    let actions = vec![
        serde_json::json!({
            "type": "refactor-file",
            "auto_fixable": false,
            "description": format!("Refactor `{path}` — high complexity combined with frequent changes makes this a maintenance risk"),
            "note": "Prioritize extracting complex functions, adding tests, or splitting the module",
        }),
        serde_json::json!({
            "type": "add-tests",
            "auto_fixable": false,
            "description": format!("Add test coverage for `{path}` to reduce change risk"),
            "note": "Frequently changed complex files benefit most from comprehensive test coverage",
        }),
    ];

    serde_json::Value::Array(actions)
}

/// Build the `actions` array for a single refactoring target.
fn build_refactoring_target_actions(item: &serde_json::Value) -> serde_json::Value {
    let recommendation = item
        .get("recommendation")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Apply the recommended refactoring");

    let category = item
        .get("category")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("refactoring");

    let mut actions = vec![serde_json::json!({
        "type": "apply-refactoring",
        "auto_fixable": false,
        "description": recommendation,
        "category": category,
    })];

    // Targets with evidence linking to specific functions get a suppress action
    if item.get("evidence").is_some() {
        actions.push(serde_json::json!({
            "type": "suppress-line",
            "auto_fixable": false,
            "description": "Suppress the underlying complexity finding",
            "comment": "// fallow-ignore-next-line complexity",
        }));
    }

    serde_json::Value::Array(actions)
}

/// Build the `actions` array for an untested file.
fn build_untested_file_actions(item: &serde_json::Value) -> serde_json::Value {
    let path = item
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("file");

    serde_json::Value::Array(vec![
        serde_json::json!({
            "type": "add-tests",
            "auto_fixable": false,
            "description": format!("Add test coverage for `{path}`"),
            "note": "No test dependency path reaches this runtime file",
        }),
        serde_json::json!({
            "type": "suppress-file",
            "auto_fixable": false,
            "description": format!("Suppress coverage gap reporting for `{path}`"),
            "comment": "// fallow-ignore-file coverage-gaps",
        }),
    ])
}

/// Build the `actions` array for an untested export.
fn build_untested_export_actions(item: &serde_json::Value) -> serde_json::Value {
    let path = item
        .get("path")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("file");
    let export_name = item
        .get("export_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("export");

    serde_json::Value::Array(vec![
        serde_json::json!({
            "type": "add-test-import",
            "auto_fixable": false,
            "description": format!("Import and test `{export_name}` from `{path}`"),
            "note": "This export is runtime-reachable but no test-reachable module references it",
        }),
        serde_json::json!({
            "type": "suppress-file",
            "auto_fixable": false,
            "description": format!("Suppress coverage gap reporting for `{path}`"),
            "comment": "// fallow-ignore-file coverage-gaps",
        }),
    ])
}

// ── Duplication action injection ────────────────────────────────

/// Inject `actions` arrays into clone families/groups in a duplication JSON output.
///
/// Walks `clone_families` and `clone_groups` arrays, appending
/// machine-actionable fix and config hints to each item.
#[allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) needed — used by audit.rs via re-export, but not part of public API"
)]
pub(crate) fn inject_dupes_actions(output: &mut serde_json::Value) {
    let Some(map) = output.as_object_mut() else {
        return;
    };

    // Clone families: extract shared module/function
    if let Some(families) = map.get_mut("clone_families").and_then(|v| v.as_array_mut()) {
        for item in families {
            let actions = build_clone_family_actions(item);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }

    // Clone groups: extract shared code
    if let Some(groups) = map.get_mut("clone_groups").and_then(|v| v.as_array_mut()) {
        for item in groups {
            let actions = build_clone_group_actions(item);
            if let serde_json::Value::Object(obj) = item {
                obj.insert("actions".to_string(), actions);
            }
        }
    }
}

/// Build the `actions` array for a single clone family.
fn build_clone_family_actions(item: &serde_json::Value) -> serde_json::Value {
    let group_count = item
        .get("groups")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);

    let total_lines = item
        .get("total_duplicated_lines")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let mut actions = vec![serde_json::json!({
        "type": "extract-shared",
        "auto_fixable": false,
        "description": format!(
            "Extract {group_count} duplicated code block{} ({total_lines} lines) into a shared module",
            if group_count == 1 { "" } else { "s" }
        ),
        "note": "These clone groups share the same files, indicating a structural relationship — refactor together",
    })];

    // Include any refactoring suggestions from the family
    if let Some(suggestions) = item.get("suggestions").and_then(|v| v.as_array()) {
        for suggestion in suggestions {
            if let Some(desc) = suggestion
                .get("description")
                .and_then(serde_json::Value::as_str)
            {
                actions.push(serde_json::json!({
                    "type": "apply-suggestion",
                    "auto_fixable": false,
                    "description": desc,
                }));
            }
        }
    }

    actions.push(serde_json::json!({
        "type": "suppress-line",
        "auto_fixable": false,
        "description": "Suppress with an inline comment above the duplicated code",
        "comment": "// fallow-ignore-next-line code-duplication",
    }));

    serde_json::Value::Array(actions)
}

/// Build the `actions` array for a single clone group.
fn build_clone_group_actions(item: &serde_json::Value) -> serde_json::Value {
    let instance_count = item
        .get("instances")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);

    let line_count = item
        .get("line_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    let actions = vec![
        serde_json::json!({
            "type": "extract-shared",
            "auto_fixable": false,
            "description": format!(
                "Extract duplicated code ({line_count} lines, {instance_count} instance{}) into a shared function",
                if instance_count == 1 { "" } else { "s" }
            ),
        }),
        serde_json::json!({
            "type": "suppress-line",
            "auto_fixable": false,
            "description": "Suppress with an inline comment above the duplicated code",
            "comment": "// fallow-ignore-next-line code-duplication",
        }),
    ];

    serde_json::Value::Array(actions)
}

/// Insert a `_meta` key into a JSON object value.
fn insert_meta(output: &mut serde_json::Value, meta: serde_json::Value) {
    if let serde_json::Value::Object(map) = output {
        map.insert("_meta".to_string(), meta);
    }
}

pub(super) fn print_health_json(
    report: &crate::health_types::HealthReport,
    root: &Path,
    elapsed: Duration,
    explain: bool,
) -> ExitCode {
    let report_value = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: failed to serialize health report: {e}");
            return ExitCode::from(2);
        }
    };

    let mut output = build_json_envelope(report_value, elapsed);
    let root_prefix = format!("{}/", root.display());
    strip_root_prefix(&mut output, &root_prefix);
    inject_health_actions(&mut output);

    if explain {
        insert_meta(&mut output, explain::health_meta());
    }

    emit_json(&output, "JSON")
}

pub(super) fn print_duplication_json(
    report: &DuplicationReport,
    elapsed: Duration,
    explain: bool,
) -> ExitCode {
    let report_value = match serde_json::to_value(report) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: failed to serialize duplication report: {e}");
            return ExitCode::from(2);
        }
    };

    let mut output = build_json_envelope(report_value, elapsed);
    inject_dupes_actions(&mut output);

    if explain {
        insert_meta(&mut output, explain::dupes_meta());
    }

    emit_json(&output, "JSON")
}

pub(super) fn print_trace_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(e) => {
            eprintln!("Error: failed to serialize trace output: {e}");
            #[expect(
                clippy::exit,
                reason = "fatal serialization error requires immediate exit"
            )]
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::test_helpers::sample_results;
    use fallow_core::extract::MemberKind;
    use fallow_core::results::*;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn json_output_has_metadata_fields() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(123);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["schema_version"], 3);
        assert!(output["version"].is_string());
        assert_eq!(output["elapsed_ms"], 123);
        assert_eq!(output["total_issues"], 0);
    }

    #[test]
    fn json_output_includes_issue_arrays() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let elapsed = Duration::from_millis(50);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["unused_files"].as_array().unwrap().len(), 1);
        assert_eq!(output["unused_exports"].as_array().unwrap().len(), 1);
        assert_eq!(output["unused_types"].as_array().unwrap().len(), 1);
        assert_eq!(output["unused_dependencies"].as_array().unwrap().len(), 1);
        assert_eq!(
            output["unused_dev_dependencies"].as_array().unwrap().len(),
            1
        );
        assert_eq!(output["unused_enum_members"].as_array().unwrap().len(), 1);
        assert_eq!(output["unused_class_members"].as_array().unwrap().len(), 1);
        assert_eq!(output["unresolved_imports"].as_array().unwrap().len(), 1);
        assert_eq!(output["unlisted_dependencies"].as_array().unwrap().len(), 1);
        assert_eq!(output["duplicate_exports"].as_array().unwrap().len(), 1);
        assert_eq!(
            output["type_only_dependencies"].as_array().unwrap().len(),
            1
        );
        assert_eq!(output["circular_dependencies"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn json_metadata_fields_appear_first() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");
        let keys: Vec<&String> = output.as_object().unwrap().keys().collect();
        assert_eq!(keys[0], "schema_version");
        assert_eq!(keys[1], "version");
        assert_eq!(keys[2], "elapsed_ms");
        assert_eq!(keys[3], "total_issues");
    }

    #[test]
    fn json_total_issues_matches_results() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let total = results.total_issues();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["total_issues"], total);
    }

    #[test]
    fn json_unused_export_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_exports.push(UnusedExport {
            path: root.join("src/utils.ts"),
            export_name: "helperFn".to_string(),
            is_type_only: false,
            line: 10,
            col: 4,
            span_start: 120,
            is_re_export: false,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let export = &output["unused_exports"][0];
        assert_eq!(export["export_name"], "helperFn");
        assert_eq!(export["line"], 10);
        assert_eq!(export["col"], 4);
        assert_eq!(export["is_type_only"], false);
        assert_eq!(export["span_start"], 120);
        assert_eq!(export["is_re_export"], false);
    }

    #[test]
    fn json_serializes_to_valid_json() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let elapsed = Duration::from_millis(42);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let json_str = serde_json::to_string_pretty(&output).expect("should stringify");
        let reparsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("JSON output should be valid JSON");
        assert_eq!(reparsed, output);
    }

    // ── Empty results ───────────────────────────────────────────────

    #[test]
    fn json_empty_results_produce_valid_structure() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["total_issues"], 0);
        assert_eq!(output["unused_files"].as_array().unwrap().len(), 0);
        assert_eq!(output["unused_exports"].as_array().unwrap().len(), 0);
        assert_eq!(output["unused_types"].as_array().unwrap().len(), 0);
        assert_eq!(output["unused_dependencies"].as_array().unwrap().len(), 0);
        assert_eq!(
            output["unused_dev_dependencies"].as_array().unwrap().len(),
            0
        );
        assert_eq!(output["unused_enum_members"].as_array().unwrap().len(), 0);
        assert_eq!(output["unused_class_members"].as_array().unwrap().len(), 0);
        assert_eq!(output["unresolved_imports"].as_array().unwrap().len(), 0);
        assert_eq!(output["unlisted_dependencies"].as_array().unwrap().len(), 0);
        assert_eq!(output["duplicate_exports"].as_array().unwrap().len(), 0);
        assert_eq!(
            output["type_only_dependencies"].as_array().unwrap().len(),
            0
        );
        assert_eq!(output["circular_dependencies"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn json_empty_results_round_trips_through_string() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let json_str = serde_json::to_string(&output).expect("should stringify");
        let reparsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("should parse back");
        assert_eq!(reparsed["total_issues"], 0);
    }

    // ── Path stripping ──────────────────────────────────────────────

    #[test]
    fn json_paths_are_relative_to_root() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: root.join("src/deep/nested/file.ts"),
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let path = output["unused_files"][0]["path"].as_str().unwrap();
        assert_eq!(path, "src/deep/nested/file.ts");
        assert!(!path.starts_with("/project"));
    }

    #[test]
    fn json_strips_root_from_nested_locations() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unlisted_dependencies.push(UnlistedDependency {
            package_name: "chalk".to_string(),
            imported_from: vec![ImportSite {
                path: root.join("src/cli.ts"),
                line: 2,
                col: 0,
            }],
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let site_path = output["unlisted_dependencies"][0]["imported_from"][0]["path"]
            .as_str()
            .unwrap();
        assert_eq!(site_path, "src/cli.ts");
    }

    #[test]
    fn json_strips_root_from_duplicate_export_locations() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.duplicate_exports.push(DuplicateExport {
            export_name: "Config".to_string(),
            locations: vec![
                DuplicateLocation {
                    path: root.join("src/config.ts"),
                    line: 15,
                    col: 0,
                },
                DuplicateLocation {
                    path: root.join("src/types.ts"),
                    line: 30,
                    col: 0,
                },
            ],
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let loc0 = output["duplicate_exports"][0]["locations"][0]["path"]
            .as_str()
            .unwrap();
        let loc1 = output["duplicate_exports"][0]["locations"][1]["path"]
            .as_str()
            .unwrap();
        assert_eq!(loc0, "src/config.ts");
        assert_eq!(loc1, "src/types.ts");
    }

    #[test]
    fn json_strips_root_from_circular_dependency_files() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.circular_dependencies.push(CircularDependency {
            files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let files = output["circular_dependencies"][0]["files"]
            .as_array()
            .unwrap();
        assert_eq!(files[0].as_str().unwrap(), "src/a.ts");
        assert_eq!(files[1].as_str().unwrap(), "src/b.ts");
    }

    #[test]
    fn json_path_outside_root_not_stripped() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("/other/project/src/file.ts"),
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let path = output["unused_files"][0]["path"].as_str().unwrap();
        assert!(path.contains("/other/project/"));
    }

    // ── Individual issue type field verification ────────────────────

    #[test]
    fn json_unused_file_contains_path() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: root.join("src/orphan.ts"),
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let file = &output["unused_files"][0];
        assert_eq!(file["path"], "src/orphan.ts");
    }

    #[test]
    fn json_unused_type_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_types.push(UnusedExport {
            path: root.join("src/types.ts"),
            export_name: "OldInterface".to_string(),
            is_type_only: true,
            line: 20,
            col: 0,
            span_start: 300,
            is_re_export: false,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let typ = &output["unused_types"][0];
        assert_eq!(typ["export_name"], "OldInterface");
        assert_eq!(typ["is_type_only"], true);
        assert_eq!(typ["line"], 20);
        assert_eq!(typ["path"], "src/types.ts");
    }

    #[test]
    fn json_unused_dependency_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_dependencies.push(UnusedDependency {
            package_name: "axios".to_string(),
            location: DependencyLocation::Dependencies,
            path: root.join("package.json"),
            line: 10,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dep = &output["unused_dependencies"][0];
        assert_eq!(dep["package_name"], "axios");
        assert_eq!(dep["line"], 10);
    }

    #[test]
    fn json_unused_dev_dependency_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_dev_dependencies.push(UnusedDependency {
            package_name: "vitest".to_string(),
            location: DependencyLocation::DevDependencies,
            path: root.join("package.json"),
            line: 15,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dep = &output["unused_dev_dependencies"][0];
        assert_eq!(dep["package_name"], "vitest");
    }

    #[test]
    fn json_unused_optional_dependency_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_optional_dependencies.push(UnusedDependency {
            package_name: "fsevents".to_string(),
            location: DependencyLocation::OptionalDependencies,
            path: root.join("package.json"),
            line: 12,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dep = &output["unused_optional_dependencies"][0];
        assert_eq!(dep["package_name"], "fsevents");
        assert_eq!(output["total_issues"], 1);
    }

    #[test]
    fn json_unused_enum_member_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_enum_members.push(UnusedMember {
            path: root.join("src/enums.ts"),
            parent_name: "Color".to_string(),
            member_name: "Purple".to_string(),
            kind: MemberKind::EnumMember,
            line: 5,
            col: 2,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let member = &output["unused_enum_members"][0];
        assert_eq!(member["parent_name"], "Color");
        assert_eq!(member["member_name"], "Purple");
        assert_eq!(member["line"], 5);
        assert_eq!(member["path"], "src/enums.ts");
    }

    #[test]
    fn json_unused_class_member_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_class_members.push(UnusedMember {
            path: root.join("src/api.ts"),
            parent_name: "ApiClient".to_string(),
            member_name: "deprecatedFetch".to_string(),
            kind: MemberKind::ClassMethod,
            line: 100,
            col: 4,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let member = &output["unused_class_members"][0];
        assert_eq!(member["parent_name"], "ApiClient");
        assert_eq!(member["member_name"], "deprecatedFetch");
        assert_eq!(member["line"], 100);
    }

    #[test]
    fn json_unresolved_import_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unresolved_imports.push(UnresolvedImport {
            path: root.join("src/app.ts"),
            specifier: "@acme/missing-pkg".to_string(),
            line: 7,
            col: 0,
            specifier_col: 0,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let import = &output["unresolved_imports"][0];
        assert_eq!(import["specifier"], "@acme/missing-pkg");
        assert_eq!(import["line"], 7);
        assert_eq!(import["path"], "src/app.ts");
    }

    #[test]
    fn json_unlisted_dependency_contains_import_sites() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unlisted_dependencies.push(UnlistedDependency {
            package_name: "dotenv".to_string(),
            imported_from: vec![
                ImportSite {
                    path: root.join("src/config.ts"),
                    line: 1,
                    col: 0,
                },
                ImportSite {
                    path: root.join("src/server.ts"),
                    line: 3,
                    col: 0,
                },
            ],
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dep = &output["unlisted_dependencies"][0];
        assert_eq!(dep["package_name"], "dotenv");
        let sites = dep["imported_from"].as_array().unwrap();
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0]["path"], "src/config.ts");
        assert_eq!(sites[1]["path"], "src/server.ts");
    }

    #[test]
    fn json_duplicate_export_contains_locations() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.duplicate_exports.push(DuplicateExport {
            export_name: "Button".to_string(),
            locations: vec![
                DuplicateLocation {
                    path: root.join("src/ui.ts"),
                    line: 10,
                    col: 0,
                },
                DuplicateLocation {
                    path: root.join("src/components.ts"),
                    line: 25,
                    col: 0,
                },
            ],
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dup = &output["duplicate_exports"][0];
        assert_eq!(dup["export_name"], "Button");
        let locs = dup["locations"].as_array().unwrap();
        assert_eq!(locs.len(), 2);
        assert_eq!(locs[0]["line"], 10);
        assert_eq!(locs[1]["line"], 25);
    }

    #[test]
    fn json_type_only_dependency_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.type_only_dependencies.push(TypeOnlyDependency {
            package_name: "zod".to_string(),
            path: root.join("package.json"),
            line: 8,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let dep = &output["type_only_dependencies"][0];
        assert_eq!(dep["package_name"], "zod");
        assert_eq!(dep["line"], 8);
    }

    #[test]
    fn json_circular_dependency_contains_expected_fields() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.circular_dependencies.push(CircularDependency {
            files: vec![
                root.join("src/a.ts"),
                root.join("src/b.ts"),
                root.join("src/c.ts"),
            ],
            length: 3,
            line: 5,
            col: 0,
            is_cross_package: false,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let cycle = &output["circular_dependencies"][0];
        assert_eq!(cycle["length"], 3);
        assert_eq!(cycle["line"], 5);
        let files = cycle["files"].as_array().unwrap();
        assert_eq!(files.len(), 3);
    }

    // ── Re-export tagging ───────────────────────────────────────────

    #[test]
    fn json_re_export_flagged_correctly() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_exports.push(UnusedExport {
            path: root.join("src/index.ts"),
            export_name: "reExported".to_string(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: true,
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["unused_exports"][0]["is_re_export"], true);
    }

    // ── Schema version stability ────────────────────────────────────

    #[test]
    fn json_schema_version_is_3() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["schema_version"], SCHEMA_VERSION);
        assert_eq!(output["schema_version"], 3);
    }

    // ── Version string ──────────────────────────────────────────────

    #[test]
    fn json_version_matches_cargo_pkg_version() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["version"], env!("CARGO_PKG_VERSION"));
    }

    // ── Elapsed time encoding ───────────────────────────────────────

    #[test]
    fn json_elapsed_ms_zero_duration() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let output = build_json(&results, &root, Duration::ZERO).expect("should serialize");

        assert_eq!(output["elapsed_ms"], 0);
    }

    #[test]
    fn json_elapsed_ms_large_duration() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_secs(120);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["elapsed_ms"], 120_000);
    }

    #[test]
    fn json_elapsed_ms_sub_millisecond_truncated() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        // 500 microseconds = 0 milliseconds (truncated)
        let elapsed = Duration::from_micros(500);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["elapsed_ms"], 0);
    }

    // ── Multiple issues of same type ────────────────────────────────

    #[test]
    fn json_multiple_unused_files() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: root.join("src/a.ts"),
        });
        results.unused_files.push(UnusedFile {
            path: root.join("src/b.ts"),
        });
        results.unused_files.push(UnusedFile {
            path: root.join("src/c.ts"),
        });
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        assert_eq!(output["unused_files"].as_array().unwrap().len(), 3);
        assert_eq!(output["total_issues"], 3);
    }

    // ── strip_root_prefix unit tests ────────────────────────────────

    #[test]
    fn strip_root_prefix_on_string_value() {
        let mut value = serde_json::json!("/project/src/file.ts");
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value, "src/file.ts");
    }

    #[test]
    fn strip_root_prefix_leaves_non_matching_string() {
        let mut value = serde_json::json!("/other/src/file.ts");
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value, "/other/src/file.ts");
    }

    #[test]
    fn strip_root_prefix_recurses_into_arrays() {
        let mut value = serde_json::json!(["/project/a.ts", "/project/b.ts", "/other/c.ts"]);
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value[0], "a.ts");
        assert_eq!(value[1], "b.ts");
        assert_eq!(value[2], "/other/c.ts");
    }

    #[test]
    fn strip_root_prefix_recurses_into_nested_objects() {
        let mut value = serde_json::json!({
            "outer": {
                "path": "/project/src/nested.ts"
            }
        });
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value["outer"]["path"], "src/nested.ts");
    }

    #[test]
    fn strip_root_prefix_leaves_numbers_and_booleans() {
        let mut value = serde_json::json!({
            "line": 42,
            "is_type_only": false,
            "path": "/project/src/file.ts"
        });
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value["line"], 42);
        assert_eq!(value["is_type_only"], false);
        assert_eq!(value["path"], "src/file.ts");
    }

    #[test]
    fn strip_root_prefix_handles_empty_string_after_strip() {
        // Edge case: the string IS the prefix (without trailing content).
        // This shouldn't happen in practice but should not panic.
        let mut value = serde_json::json!("/project/");
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value, "");
    }

    #[test]
    fn strip_root_prefix_deeply_nested_array_of_objects() {
        let mut value = serde_json::json!({
            "groups": [{
                "instances": [{
                    "file": "/project/src/a.ts"
                }, {
                    "file": "/project/src/b.ts"
                }]
            }]
        });
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value["groups"][0]["instances"][0]["file"], "src/a.ts");
        assert_eq!(value["groups"][0]["instances"][1]["file"], "src/b.ts");
    }

    // ── Full sample results round-trip ──────────────────────────────

    #[test]
    fn json_full_sample_results_total_issues_correct() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let elapsed = Duration::from_millis(100);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        // sample_results adds one of each issue type (12 total).
        // unused_files + unused_exports + unused_types + unused_dependencies
        // + unused_dev_dependencies + unused_enum_members + unused_class_members
        // + unresolved_imports + unlisted_dependencies + duplicate_exports
        // + type_only_dependencies + circular_dependencies
        assert_eq!(output["total_issues"], results.total_issues());
    }

    #[test]
    fn json_full_sample_no_absolute_paths_in_output() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let json_str = serde_json::to_string(&output).expect("should stringify");
        // The root prefix should be stripped from all paths.
        assert!(!json_str.contains("/project/src/"));
        assert!(!json_str.contains("/project/package.json"));
    }

    // ── JSON output is deterministic ────────────────────────────────

    #[test]
    fn json_output_is_deterministic() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let elapsed = Duration::from_millis(50);

        let output1 = build_json(&results, &root, elapsed).expect("first build");
        let output2 = build_json(&results, &root, elapsed).expect("second build");

        assert_eq!(output1, output2);
    }

    // ── Metadata not overwritten by results fields ──────────────────

    #[test]
    fn json_results_fields_do_not_shadow_metadata() {
        // Ensure that serialized results don't contain keys like "schema_version"
        // that could overwrite the metadata fields we insert first.
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(99);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        // Metadata should reflect our explicit values, not anything from AnalysisResults.
        assert_eq!(output["schema_version"], 3);
        assert_eq!(output["elapsed_ms"], 99);
    }

    // ── All 14 issue type arrays present ────────────────────────────

    #[test]
    fn json_all_issue_type_arrays_present_in_empty_results() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let expected_arrays = [
            "unused_files",
            "unused_exports",
            "unused_types",
            "unused_dependencies",
            "unused_dev_dependencies",
            "unused_optional_dependencies",
            "unused_enum_members",
            "unused_class_members",
            "unresolved_imports",
            "unlisted_dependencies",
            "duplicate_exports",
            "type_only_dependencies",
            "test_only_dependencies",
            "circular_dependencies",
        ];
        for key in &expected_arrays {
            assert!(
                output[key].is_array(),
                "expected '{key}' to be an array in JSON output"
            );
        }
    }

    // ── insert_meta ─────────────────────────────────────────────────

    #[test]
    fn insert_meta_adds_key_to_object() {
        let mut output = serde_json::json!({ "foo": 1 });
        let meta = serde_json::json!({ "docs": "https://example.com" });
        insert_meta(&mut output, meta.clone());
        assert_eq!(output["_meta"], meta);
    }

    #[test]
    fn insert_meta_noop_on_non_object() {
        let mut output = serde_json::json!([1, 2, 3]);
        let meta = serde_json::json!({ "docs": "https://example.com" });
        insert_meta(&mut output, meta);
        // Should not panic or add anything
        assert!(output.is_array());
    }

    #[test]
    fn insert_meta_overwrites_existing_meta() {
        let mut output = serde_json::json!({ "_meta": "old" });
        let meta = serde_json::json!({ "new": true });
        insert_meta(&mut output, meta.clone());
        assert_eq!(output["_meta"], meta);
    }

    // ── build_json_envelope ─────────────────────────────────────────

    #[test]
    fn build_json_envelope_has_metadata_fields() {
        let report = serde_json::json!({ "findings": [] });
        let elapsed = Duration::from_millis(42);
        let output = build_json_envelope(report, elapsed);

        assert_eq!(output["schema_version"], 3);
        assert!(output["version"].is_string());
        assert_eq!(output["elapsed_ms"], 42);
        assert!(output["findings"].is_array());
    }

    #[test]
    fn build_json_envelope_metadata_appears_first() {
        let report = serde_json::json!({ "data": "value" });
        let output = build_json_envelope(report, Duration::from_millis(10));

        let keys: Vec<&String> = output.as_object().unwrap().keys().collect();
        assert_eq!(keys[0], "schema_version");
        assert_eq!(keys[1], "version");
        assert_eq!(keys[2], "elapsed_ms");
    }

    #[test]
    fn build_json_envelope_non_object_report() {
        // If report_value is not an Object, only metadata fields appear
        let report = serde_json::json!("not an object");
        let output = build_json_envelope(report, Duration::from_millis(0));

        let obj = output.as_object().unwrap();
        assert_eq!(obj.len(), 3);
        assert!(obj.contains_key("schema_version"));
        assert!(obj.contains_key("version"));
        assert!(obj.contains_key("elapsed_ms"));
    }

    // ── strip_root_prefix with null value ──

    #[test]
    fn strip_root_prefix_null_unchanged() {
        let mut value = serde_json::Value::Null;
        strip_root_prefix(&mut value, "/project/");
        assert!(value.is_null());
    }

    // ── strip_root_prefix with empty string ──

    #[test]
    fn strip_root_prefix_empty_string() {
        let mut value = serde_json::json!("");
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value, "");
    }

    // ── strip_root_prefix on mixed nested structure ──

    #[test]
    fn strip_root_prefix_mixed_types() {
        let mut value = serde_json::json!({
            "path": "/project/src/file.ts",
            "line": 42,
            "flag": true,
            "nested": {
                "items": ["/project/a.ts", 99, null, "/project/b.ts"],
                "deep": { "path": "/project/c.ts" }
            }
        });
        strip_root_prefix(&mut value, "/project/");
        assert_eq!(value["path"], "src/file.ts");
        assert_eq!(value["line"], 42);
        assert_eq!(value["flag"], true);
        assert_eq!(value["nested"]["items"][0], "a.ts");
        assert_eq!(value["nested"]["items"][1], 99);
        assert!(value["nested"]["items"][2].is_null());
        assert_eq!(value["nested"]["items"][3], "b.ts");
        assert_eq!(value["nested"]["deep"]["path"], "c.ts");
    }

    // ── JSON with explain meta for check ──

    #[test]
    fn json_check_meta_integrates_correctly() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let elapsed = Duration::from_millis(0);
        let mut output = build_json(&results, &root, elapsed).expect("should serialize");
        insert_meta(&mut output, crate::explain::check_meta());

        assert!(output["_meta"]["docs"].is_string());
        assert!(output["_meta"]["rules"].is_object());
    }

    // ── JSON unused member kind serialization ──

    #[test]
    fn json_unused_member_kind_serialized() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_enum_members.push(UnusedMember {
            path: root.join("src/enums.ts"),
            parent_name: "Color".to_string(),
            member_name: "Red".to_string(),
            kind: MemberKind::EnumMember,
            line: 3,
            col: 2,
        });
        results.unused_class_members.push(UnusedMember {
            path: root.join("src/class.ts"),
            parent_name: "Foo".to_string(),
            member_name: "bar".to_string(),
            kind: MemberKind::ClassMethod,
            line: 10,
            col: 4,
        });

        let elapsed = Duration::from_millis(0);
        let output = build_json(&results, &root, elapsed).expect("should serialize");

        let enum_member = &output["unused_enum_members"][0];
        assert!(enum_member["kind"].is_string());
        let class_member = &output["unused_class_members"][0];
        assert!(class_member["kind"].is_string());
    }

    // ── Actions injection ──────────────────────────────────────────

    #[test]
    fn json_unused_export_has_actions() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_exports.push(UnusedExport {
            path: root.join("src/utils.ts"),
            export_name: "helperFn".to_string(),
            is_type_only: false,
            line: 10,
            col: 4,
            span_start: 120,
            is_re_export: false,
        });
        let output = build_json(&results, &root, Duration::ZERO).unwrap();

        let actions = output["unused_exports"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);

        // Fix action
        assert_eq!(actions[0]["type"], "remove-export");
        assert_eq!(actions[0]["auto_fixable"], true);
        assert!(actions[0].get("note").is_none());

        // Suppress action
        assert_eq!(actions[1]["type"], "suppress-line");
        assert_eq!(
            actions[1]["comment"],
            "// fallow-ignore-next-line unused-export"
        );
    }

    #[test]
    fn json_unused_file_has_file_suppress_and_note() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: root.join("src/dead.ts"),
        });
        let output = build_json(&results, &root, Duration::ZERO).unwrap();

        let actions = output["unused_files"][0]["actions"].as_array().unwrap();
        assert_eq!(actions[0]["type"], "delete-file");
        assert_eq!(actions[0]["auto_fixable"], false);
        assert!(actions[0]["note"].is_string());
        assert_eq!(actions[1]["type"], "suppress-file");
        assert_eq!(actions[1]["comment"], "// fallow-ignore-file unused-file");
    }

    #[test]
    fn json_unused_dependency_has_config_suppress_with_package_name() {
        let root = PathBuf::from("/project");
        let mut results = AnalysisResults::default();
        results.unused_dependencies.push(UnusedDependency {
            package_name: "lodash".to_string(),
            location: DependencyLocation::Dependencies,
            path: root.join("package.json"),
            line: 5,
        });
        let output = build_json(&results, &root, Duration::ZERO).unwrap();

        let actions = output["unused_dependencies"][0]["actions"]
            .as_array()
            .unwrap();
        assert_eq!(actions[0]["type"], "remove-dependency");
        assert_eq!(actions[0]["auto_fixable"], true);

        // Config suppress includes actual package name
        assert_eq!(actions[1]["type"], "add-to-config");
        assert_eq!(actions[1]["config_key"], "ignoreDependencies");
        assert_eq!(actions[1]["value"], "lodash");
    }

    #[test]
    fn json_empty_results_have_no_actions_in_empty_arrays() {
        let root = PathBuf::from("/project");
        let results = AnalysisResults::default();
        let output = build_json(&results, &root, Duration::ZERO).unwrap();

        // Empty arrays should remain empty
        assert!(output["unused_exports"].as_array().unwrap().is_empty());
        assert!(output["unused_files"].as_array().unwrap().is_empty());
    }

    #[test]
    fn json_all_issue_types_have_actions() {
        let root = PathBuf::from("/project");
        let results = sample_results(&root);
        let output = build_json(&results, &root, Duration::ZERO).unwrap();

        let issue_keys = [
            "unused_files",
            "unused_exports",
            "unused_types",
            "unused_dependencies",
            "unused_dev_dependencies",
            "unused_optional_dependencies",
            "unused_enum_members",
            "unused_class_members",
            "unresolved_imports",
            "unlisted_dependencies",
            "duplicate_exports",
            "type_only_dependencies",
            "test_only_dependencies",
            "circular_dependencies",
        ];

        for key in &issue_keys {
            let arr = output[key].as_array().unwrap();
            if !arr.is_empty() {
                let actions = arr[0]["actions"].as_array();
                assert!(
                    actions.is_some() && !actions.unwrap().is_empty(),
                    "missing actions for {key}"
                );
            }
        }
    }

    // ── Health actions injection ───────────────────────────────────

    #[test]
    fn health_finding_has_actions() {
        let mut output = serde_json::json!({
            "findings": [{
                "path": "src/utils.ts",
                "name": "processData",
                "line": 10,
                "col": 0,
                "cyclomatic": 25,
                "cognitive": 30,
                "line_count": 150,
                "exceeded": "both"
            }]
        });

        inject_health_actions(&mut output);

        let actions = output["findings"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["type"], "refactor-function");
        assert_eq!(actions[0]["auto_fixable"], false);
        assert!(
            actions[0]["description"]
                .as_str()
                .unwrap()
                .contains("processData")
        );
        assert_eq!(actions[1]["type"], "suppress-line");
        assert_eq!(
            actions[1]["comment"],
            "// fallow-ignore-next-line complexity"
        );
    }

    #[test]
    fn refactoring_target_has_actions() {
        let mut output = serde_json::json!({
            "targets": [{
                "path": "src/big-module.ts",
                "priority": 85.0,
                "efficiency": 42.5,
                "recommendation": "Split module: 12 exports, 4 unused",
                "category": "split_high_impact",
                "effort": "medium",
                "confidence": "high",
                "evidence": { "unused_exports": 4 }
            }]
        });

        inject_health_actions(&mut output);

        let actions = output["targets"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["type"], "apply-refactoring");
        assert_eq!(
            actions[0]["description"],
            "Split module: 12 exports, 4 unused"
        );
        assert_eq!(actions[0]["category"], "split_high_impact");
        // Target with evidence gets suppress action
        assert_eq!(actions[1]["type"], "suppress-line");
    }

    #[test]
    fn refactoring_target_without_evidence_has_no_suppress() {
        let mut output = serde_json::json!({
            "targets": [{
                "path": "src/simple.ts",
                "priority": 30.0,
                "efficiency": 15.0,
                "recommendation": "Consider extracting helper functions",
                "category": "extract_complex_functions",
                "effort": "small",
                "confidence": "medium"
            }]
        });

        inject_health_actions(&mut output);

        let actions = output["targets"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0]["type"], "apply-refactoring");
    }

    #[test]
    fn health_empty_findings_no_actions() {
        let mut output = serde_json::json!({
            "findings": [],
            "targets": []
        });

        inject_health_actions(&mut output);

        assert!(output["findings"].as_array().unwrap().is_empty());
        assert!(output["targets"].as_array().unwrap().is_empty());
    }

    #[test]
    fn hotspot_has_actions() {
        let mut output = serde_json::json!({
            "hotspots": [{
                "path": "src/utils.ts",
                "complexity_score": 45.0,
                "churn_score": 12,
                "hotspot_score": 540.0
            }]
        });

        inject_health_actions(&mut output);

        let actions = output["hotspots"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["type"], "refactor-file");
        assert!(
            actions[0]["description"]
                .as_str()
                .unwrap()
                .contains("src/utils.ts")
        );
        assert_eq!(actions[1]["type"], "add-tests");
    }

    #[test]
    fn health_finding_suppress_has_placement() {
        let mut output = serde_json::json!({
            "findings": [{
                "path": "src/utils.ts",
                "name": "processData",
                "line": 10,
                "col": 0,
                "cyclomatic": 25,
                "cognitive": 30,
                "line_count": 150,
                "exceeded": "both"
            }]
        });

        inject_health_actions(&mut output);

        let suppress = &output["findings"][0]["actions"][1];
        assert_eq!(suppress["placement"], "above-function-declaration");
    }

    // ── Duplication actions injection ─────────────────────────────

    #[test]
    fn clone_family_has_actions() {
        let mut output = serde_json::json!({
            "clone_families": [{
                "files": ["src/a.ts", "src/b.ts"],
                "groups": [
                    { "instances": [{"file": "src/a.ts"}, {"file": "src/b.ts"}], "token_count": 100, "line_count": 20 }
                ],
                "total_duplicated_lines": 20,
                "total_duplicated_tokens": 100,
                "suggestions": [
                    { "kind": "ExtractFunction", "description": "Extract shared validation logic", "estimated_savings": 15 }
                ]
            }]
        });

        inject_dupes_actions(&mut output);

        let actions = output["clone_families"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(actions[0]["type"], "extract-shared");
        assert_eq!(actions[0]["auto_fixable"], false);
        assert!(
            actions[0]["description"]
                .as_str()
                .unwrap()
                .contains("20 lines")
        );
        // Suggestion forwarded as action
        assert_eq!(actions[1]["type"], "apply-suggestion");
        assert!(
            actions[1]["description"]
                .as_str()
                .unwrap()
                .contains("validation logic")
        );
        // Suppress action
        assert_eq!(actions[2]["type"], "suppress-line");
        assert_eq!(
            actions[2]["comment"],
            "// fallow-ignore-next-line code-duplication"
        );
    }

    #[test]
    fn clone_group_has_actions() {
        let mut output = serde_json::json!({
            "clone_groups": [{
                "instances": [
                    {"file": "src/a.ts", "start_line": 1, "end_line": 10},
                    {"file": "src/b.ts", "start_line": 5, "end_line": 14}
                ],
                "token_count": 50,
                "line_count": 10
            }]
        });

        inject_dupes_actions(&mut output);

        let actions = output["clone_groups"][0]["actions"].as_array().unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0]["type"], "extract-shared");
        assert!(
            actions[0]["description"]
                .as_str()
                .unwrap()
                .contains("10 lines")
        );
        assert!(
            actions[0]["description"]
                .as_str()
                .unwrap()
                .contains("2 instances")
        );
        assert_eq!(actions[1]["type"], "suppress-line");
    }

    #[test]
    fn dupes_empty_results_no_actions() {
        let mut output = serde_json::json!({
            "clone_families": [],
            "clone_groups": []
        });

        inject_dupes_actions(&mut output);

        assert!(output["clone_families"].as_array().unwrap().is_empty());
        assert!(output["clone_groups"].as_array().unwrap().is_empty());
    }
}
