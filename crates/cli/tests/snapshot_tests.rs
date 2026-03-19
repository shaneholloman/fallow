use std::path::{Path, PathBuf};
use std::time::Duration;

use fallow_cli::report::{build_compact_lines, build_json, build_sarif};
use fallow_config::RulesConfig;
use fallow_core::extract::MemberKind;
use fallow_core::results::*;

/// Build sample `AnalysisResults` with one issue of each type for consistent snapshots.
fn sample_results(root: &Path) -> AnalysisResults {
    let mut r = AnalysisResults::default();

    r.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    r.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "helperFn".to_string(),
        is_type_only: false,
        line: 10,
        col: 4,
        span_start: 120,
        is_re_export: false,
    });
    r.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    r.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
    });
    r.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
    });
    r.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    r.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    r.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "./missing-module".to_string(),
        line: 3,
        col: 0,
    });
    r.unlisted_dependencies.push(UnlistedDependency {
        package_name: "chalk".to_string(),
        imported_from: vec![root.join("src/cli.ts")],
    });
    r.duplicate_exports.push(DuplicateExport {
        export_name: "Config".to_string(),
        locations: vec![root.join("src/config.ts"), root.join("src/types.ts")],
    });

    r
}

// ── JSON format ──────────────────────────────────────────────────

#[test]
fn json_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let elapsed = Duration::from_millis(42);
    let value = build_json(&results, elapsed).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");

    // Redact dynamic values (version changes with releases, elapsed_ms may vary)
    insta::assert_snapshot!(
        "json_output",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

#[test]
fn json_empty_results_snapshot() {
    let results = AnalysisResults::default();
    let elapsed = Duration::from_millis(0);
    let value = build_json(&results, elapsed).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");

    insta::assert_snapshot!(
        "json_empty",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

// ── SARIF format ─────────────────────────────────────────────────

#[test]
fn sarif_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");

    insta::assert_snapshot!(
        "sarif_output",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

#[test]
fn sarif_empty_results_snapshot() {
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");

    insta::assert_snapshot!(
        "sarif_empty",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

// ── Compact format ───────────────────────────────────────────────

#[test]
fn compact_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let lines = build_compact_lines(&results, &root);
    let output = lines.join("\n");

    insta::assert_snapshot!("compact_output", output);
}

#[test]
fn compact_empty_results_snapshot() {
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let lines = build_compact_lines(&results, &root);
    let output = lines.join("\n");

    insta::assert_snapshot!("compact_empty", output);
}
