use std::path::{Path, PathBuf};
use std::time::Duration;

use fallow_cli::health_types::*;
use fallow_cli::report::{
    build_codeclimate, build_compact_lines, build_duplication_codeclimate,
    build_duplication_markdown, build_health_codeclimate, build_health_markdown,
    build_health_sarif, build_json, build_markdown, build_sarif,
};
use fallow_config::RulesConfig;
use fallow_core::duplicates::{CloneGroup, CloneInstance, DuplicationReport, DuplicationStats};
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
        line: 5,
    });
    r.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
        line: 5,
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
        specifier_col: 0,
    });
    r.unlisted_dependencies.push(UnlistedDependency {
        package_name: "chalk".to_string(),
        imported_from: vec![ImportSite {
            path: root.join("src/cli.ts"),
            line: 2,
            col: 0,
        }],
    });
    r.duplicate_exports.push(DuplicateExport {
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
    r.unused_optional_dependencies.push(UnusedDependency {
        package_name: "fsevents".to_string(),
        location: DependencyLocation::OptionalDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    r.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    r.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });

    r
}

// ── JSON format ──────────────────────────────────────────────────

#[test]
fn json_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let elapsed = Duration::from_millis(42);
    let value = build_json(&results, &root, elapsed).expect("JSON build should succeed");
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
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let elapsed = Duration::from_millis(0);
    let value = build_json(&results, &root, elapsed).expect("JSON build should succeed");
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

    insta::assert_snapshot!("sarif_output", redact_sarif_version(&json_str));
}

#[test]
fn sarif_empty_results_snapshot() {
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");

    insta::assert_snapshot!("sarif_empty", redact_sarif_version(&json_str));
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

// ── Per-issue-type compact snapshots ────────────────────────────

#[test]
fn compact_unused_files_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    results.unused_files.push(UnusedFile {
        path: root.join("src/orphan.ts"),
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_files_only", lines.join("\n"));
}

#[test]
fn compact_unused_exports_only_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_exports_only", lines.join("\n"));
}

#[test]
fn compact_unused_types_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_types_only", lines.join("\n"));
}

#[test]
fn compact_unused_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    results.unused_dependencies.push(UnusedDependency {
        package_name: "moment".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_deps_only", lines.join("\n"));
}

#[test]
fn compact_unused_dev_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_dev_deps_only", lines.join("\n"));
}

#[test]
fn compact_unused_optional_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_optional_dependencies.push(UnusedDependency {
        package_name: "fsevents".to_string(),
        location: DependencyLocation::OptionalDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_optional_deps_only", lines.join("\n"));
}

#[test]
fn compact_unresolved_imports_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "./missing-module".to_string(),
        line: 3,
        col: 0,
        specifier_col: 0,
    });
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "@org/nonexistent".to_string(),
        line: 4,
        col: 0,
        specifier_col: 0,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unresolved_imports_only", lines.join("\n"));
}

#[test]
fn compact_unlisted_deps_only_snapshot() {
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
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unlisted_deps_only", lines.join("\n"));
}

#[test]
fn compact_unused_enum_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_enum_members_only", lines.join("\n"));
}

#[test]
fn compact_unused_class_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_unused_class_members_only", lines.join("\n"));
}

#[test]
fn compact_duplicate_exports_only_snapshot() {
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
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_duplicate_exports_only", lines.join("\n"));
}

// ── Re-export variant snapshots ─────────────────────────────────

#[test]
fn compact_re_export_variant_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_exports.push(UnusedExport {
        path: root.join("src/index.ts"),
        export_name: "reExportedFn".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: true,
    });
    results.unused_types.push(UnusedExport {
        path: root.join("src/index.ts"),
        export_name: "ReExportedType".to_string(),
        is_type_only: true,
        line: 2,
        col: 0,
        span_start: 30,
        is_re_export: true,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_re_export_variants", lines.join("\n"));
}

#[test]
fn json_re_export_variant_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_exports.push(UnusedExport {
        path: root.join("src/index.ts"),
        export_name: "reExportedFn".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: true,
    });
    let elapsed = Duration::from_millis(0);
    let value = build_json(&results, &root, elapsed).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!(
        "json_re_export_variant",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

#[test]
fn sarif_re_export_variant_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_exports.push(UnusedExport {
        path: root.join("src/index.ts"),
        export_name: "reExportedFn".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: true,
    });
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_re_export_variant", redact_sarif_version(&json_str));
}

// ── SARIF with mixed severity levels ────────────────────────────

#[test]
fn sarif_mixed_severity_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let rules = RulesConfig {
        unused_files: fallow_config::Severity::Error,
        unused_exports: fallow_config::Severity::Warn,
        unused_types: fallow_config::Severity::Warn,
        unused_dependencies: fallow_config::Severity::Error,
        unused_dev_dependencies: fallow_config::Severity::Warn,
        unused_optional_dependencies: fallow_config::Severity::Warn,
        unused_enum_members: fallow_config::Severity::Warn,
        unused_class_members: fallow_config::Severity::Warn,
        unresolved_imports: fallow_config::Severity::Error,
        unlisted_dependencies: fallow_config::Severity::Error,
        duplicate_exports: fallow_config::Severity::Warn,
        type_only_dependencies: fallow_config::Severity::Warn,
        circular_dependencies: fallow_config::Severity::Warn,
    };
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_mixed_severity", redact_sarif_version(&json_str));
}

// ── Type-only dependency snapshots ──────────────────────────────

#[test]
fn json_type_only_deps_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "@types/react".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    let elapsed = Duration::from_millis(10);
    let value = build_json(&results, &root, elapsed).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!(
        "json_type_only_deps",
        json_str.replace(
            &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
            "\"version\": \"[VERSION]\"",
        )
    );
}

// ── Per-issue-type JSON snapshots ───────────────────────────────

fn redact_version(json_str: &str) -> String {
    json_str.replace(
        &format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION")),
        "\"version\": \"[VERSION]\"",
    )
}

#[test]
fn json_unused_files_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_files_only", redact_version(&json_str));
}

#[test]
fn json_unused_exports_only_snapshot() {
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
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_exports_only", redact_version(&json_str));
}

#[test]
fn json_unused_types_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_types_only", redact_version(&json_str));
}

#[test]
fn json_unused_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_deps_only", redact_version(&json_str));
}

#[test]
fn json_unresolved_imports_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "./missing-module".to_string(),
        line: 3,
        col: 0,
        specifier_col: 0,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unresolved_imports_only", redact_version(&json_str));
}

#[test]
fn json_unlisted_deps_only_snapshot() {
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
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unlisted_deps_only", redact_version(&json_str));
}

#[test]
fn json_unused_enum_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_enum_members_only", redact_version(&json_str));
}

#[test]
fn json_unused_class_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_class_members_only", redact_version(&json_str));
}

#[test]
fn json_duplicate_exports_only_snapshot() {
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
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_duplicate_exports_only", redact_version(&json_str));
}

// ── Per-issue-type SARIF snapshots ──────────────────────────────

fn redact_sarif_version(json_str: &str) -> String {
    // Only redact the fallow tool version inside `"driver": { "name": "fallow", "version": "..." }`,
    // not the SARIF spec `"version": "2.1.0"` at the top level (which may collide).
    json_str.replace(
        &format!(
            "\"name\": \"fallow\",\n          \"version\": \"{}\"",
            env!("CARGO_PKG_VERSION")
        ),
        "\"name\": \"fallow\",\n          \"version\": \"[VERSION]\"",
    )
}

#[test]
fn sarif_unused_files_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_unused_files_only", redact_sarif_version(&json_str));
}

#[test]
fn sarif_unused_exports_only_snapshot() {
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
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_unused_exports_only", redact_sarif_version(&json_str));
}

#[test]
fn sarif_unused_types_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_unused_types_only", redact_sarif_version(&json_str));
}

#[test]
fn sarif_unused_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_unused_deps_only", redact_sarif_version(&json_str));
}

#[test]
fn sarif_unresolved_imports_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "./missing-module".to_string(),
        line: 3,
        col: 0,
        specifier_col: 0,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_unresolved_imports_only",
        redact_sarif_version(&json_str)
    );
}

#[test]
fn sarif_unlisted_deps_only_snapshot() {
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
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_unlisted_deps_only", redact_sarif_version(&json_str));
}

#[test]
fn sarif_unused_enum_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_unused_enum_members_only",
        redact_sarif_version(&json_str)
    );
}

#[test]
fn sarif_unused_class_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_unused_class_members_only",
        redact_sarif_version(&json_str)
    );
}

#[test]
fn sarif_duplicate_exports_only_snapshot() {
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
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_duplicate_exports_only",
        redact_sarif_version(&json_str)
    );
}

// ── Multiple items grouping ─────────────────────────────────────

#[test]
fn json_multiple_exports_same_file_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    results.unused_exports.push(UnusedExport {
        path: root.join("src/helpers.ts"),
        export_name: "capitalize".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: false,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_multiple_exports_same_file", redact_version(&json_str));
}

#[test]
fn sarif_multiple_exports_same_file_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_multiple_exports_same_file",
        redact_sarif_version(&json_str)
    );
}

#[test]
fn compact_multiple_exports_same_file_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_multiple_exports_same_file", lines.join("\n"));
}

// ── Workspace package.json path variant ─────────────────────────

#[test]
fn json_workspace_dep_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    results.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_workspace_deps", redact_version(&json_str));
}

#[test]
fn sarif_workspace_dep_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    let rules = RulesConfig::default();
    let sarif = build_sarif(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_workspace_deps", redact_sarif_version(&json_str));
}

// ── CodeClimate format ──────────────────────────────────────────

#[test]
fn codeclimate_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let rules = RulesConfig::default();
    let cc = build_codeclimate(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_output", json_str);
}

#[test]
fn codeclimate_empty_results_snapshot() {
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let rules = RulesConfig::default();
    let cc = build_codeclimate(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_empty", json_str);
}

// ── Per-issue-type CodeClimate snapshots ────────────────────────

#[test]
fn codeclimate_unused_files_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_files_only", json_str);
}

#[test]
fn codeclimate_unused_exports_only_snapshot() {
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
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_exports_only", json_str);
}

#[test]
fn codeclimate_unused_types_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_types_only", json_str);
}

#[test]
fn codeclimate_unused_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_deps_only", json_str);
}

#[test]
fn codeclimate_unresolved_imports_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/index.ts"),
        specifier: "./missing".to_string(),
        line: 3,
        col: 0,
        specifier_col: 0,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unresolved_imports_only", json_str);
}

#[test]
fn codeclimate_unlisted_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unlisted_dependencies.push(UnlistedDependency {
        package_name: "chalk".to_string(),
        imported_from: vec![ImportSite {
            path: root.join("src/logger.ts"),
            line: 1,
            col: 0,
        }],
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unlisted_deps_only", json_str);
}

#[test]
fn codeclimate_unused_enum_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_enum_members_only", json_str);
}

#[test]
fn codeclimate_unused_class_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_class_members_only", json_str);
}

#[test]
fn codeclimate_duplicate_exports_only_snapshot() {
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
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_duplicate_exports_only", json_str);
}

#[test]
fn codeclimate_re_export_variant_snapshot() {
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
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_re_export_variant", json_str);
}

#[test]
fn codeclimate_mixed_severity_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let rules = RulesConfig {
        unused_files: fallow_config::Severity::Error,
        unused_exports: fallow_config::Severity::Warn,
        unused_types: fallow_config::Severity::Warn,
        unused_dependencies: fallow_config::Severity::Error,
        unused_dev_dependencies: fallow_config::Severity::Warn,
        unused_optional_dependencies: fallow_config::Severity::Warn,
        unused_enum_members: fallow_config::Severity::Warn,
        unused_class_members: fallow_config::Severity::Warn,
        unresolved_imports: fallow_config::Severity::Error,
        unlisted_dependencies: fallow_config::Severity::Error,
        duplicate_exports: fallow_config::Severity::Warn,
        type_only_dependencies: fallow_config::Severity::Warn,
        circular_dependencies: fallow_config::Severity::Warn,
    };
    let cc = build_codeclimate(&results, &root, &rules);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_mixed_severity", json_str);
}

#[test]
fn codeclimate_type_only_deps_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_type_only_deps", json_str);
}

#[test]
fn codeclimate_unused_dev_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
        line: 12,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_dev_deps_only", json_str);
}

#[test]
fn codeclimate_unused_optional_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_optional_dependencies.push(UnusedDependency {
        package_name: "fsevents".to_string(),
        location: DependencyLocation::OptionalDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_unused_optional_deps_only", json_str);
}

#[test]
fn codeclimate_circular_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_circular_deps_only", json_str);
}

#[test]
fn codeclimate_multiple_exports_same_file_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    results.unused_exports.push(UnusedExport {
        path: root.join("src/helpers.ts"),
        export_name: "capitalize".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: false,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_multiple_exports_same_file", json_str);
}

#[test]
fn codeclimate_workspace_dep_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    let cc = build_codeclimate(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_workspace_deps", json_str);
}

// ── Cross-format parity: circular deps ──────────────────────────

#[test]
fn json_circular_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_circular_deps_only", redact_version(&json_str));
}

#[test]
fn sarif_circular_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_circular_deps_only", redact_sarif_version(&json_str));
}

#[test]
fn compact_circular_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_circular_deps_only", lines.join("\n"));
}

// ── Cross-format parity: type-only deps ─────────────────────────

#[test]
fn sarif_type_only_deps_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_type_only_deps", redact_sarif_version(&json_str));
}

#[test]
fn compact_type_only_deps_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_type_only_deps", lines.join("\n"));
}

// ── Cross-format parity: unused dev/optional deps ───────────────

#[test]
fn json_unused_dev_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
        line: 12,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_dev_deps_only", redact_version(&json_str));
}

#[test]
fn sarif_unused_dev_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dev_dependencies.push(UnusedDependency {
        package_name: "jest".to_string(),
        location: DependencyLocation::DevDependencies,
        path: root.join("package.json"),
        line: 12,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_unused_dev_deps_only",
        redact_sarif_version(&json_str)
    );
}

#[test]
fn json_unused_optional_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_optional_dependencies.push(UnusedDependency {
        package_name: "fsevents".to_string(),
        location: DependencyLocation::OptionalDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let value = build_json(&results, &root, Duration::ZERO).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_unused_optional_deps_only", redact_version(&json_str));
}

#[test]
fn sarif_unused_optional_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_optional_dependencies.push(UnusedDependency {
        package_name: "fsevents".to_string(),
        location: DependencyLocation::OptionalDependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let sarif = build_sarif(&results, &root, &RulesConfig::default());
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_unused_optional_deps_only",
        redact_sarif_version(&json_str)
    );
}

// ── Cross-format parity: multiple exports, workspace, mixed ─────

#[test]
fn compact_workspace_dep_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    let lines = build_compact_lines(&results, &root);
    insta::assert_snapshot!("compact_workspace_deps", lines.join("\n"));
}

#[test]
fn json_mixed_severity_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let elapsed = Duration::from_millis(42);
    // JSON includes severity metadata via _meta when explain is true,
    // but the raw format doesn't encode severity — this test verifies
    // the output is stable regardless of rules config.
    let value = build_json(&results, &root, elapsed).expect("JSON build should succeed");
    let json_str = serde_json::to_string_pretty(&value).expect("should serialize");
    insta::assert_snapshot!("json_mixed_severity", redact_version(&json_str));
}

// ── Markdown format ─────────────────────────────────────────────

#[test]
fn markdown_output_snapshot() {
    let root = PathBuf::from("/project");
    let results = sample_results(&root);
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_output", output);
}

#[test]
fn markdown_empty_results_snapshot() {
    let root = PathBuf::from("/project");
    let results = AnalysisResults::default();
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_empty", output);
}

#[test]
fn markdown_single_unused_file_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_files.push(UnusedFile {
        path: root.join("src/dead.ts"),
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_single_unused_file", output);
}

#[test]
fn markdown_unused_exports_only_snapshot() {
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
    results.unused_exports.push(UnusedExport {
        path: root.join("src/utils.ts"),
        export_name: "formatDate".to_string(),
        is_type_only: false,
        line: 25,
        col: 0,
        span_start: 300,
        is_re_export: false,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unused_exports_only", output);
}

#[test]
fn markdown_unused_types_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_types.push(UnusedExport {
        path: root.join("src/types.ts"),
        export_name: "OldType".to_string(),
        is_type_only: true,
        line: 5,
        col: 0,
        span_start: 60,
        is_re_export: false,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unused_types_only", output);
}

#[test]
fn markdown_unused_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("package.json"),
        line: 5,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unused_deps_only", output);
}

#[test]
fn markdown_unresolved_imports_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unresolved_imports.push(UnresolvedImport {
        path: root.join("src/app.ts"),
        specifier: "./missing-module".to_string(),
        line: 3,
        col: 0,
        specifier_col: 0,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unresolved_imports_only", output);
}

#[test]
fn markdown_unlisted_deps_only_snapshot() {
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
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unlisted_deps_only", output);
}

#[test]
fn markdown_unused_enum_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_enum_members.push(UnusedMember {
        path: root.join("src/enums.ts"),
        parent_name: "Status".to_string(),
        member_name: "Deprecated".to_string(),
        kind: MemberKind::EnumMember,
        line: 8,
        col: 2,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unused_enum_members_only", output);
}

#[test]
fn markdown_unused_class_members_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_class_members.push(UnusedMember {
        path: root.join("src/service.ts"),
        parent_name: "UserService".to_string(),
        member_name: "legacyMethod".to_string(),
        kind: MemberKind::ClassMethod,
        line: 42,
        col: 4,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_unused_class_members_only", output);
}

#[test]
fn markdown_duplicate_exports_only_snapshot() {
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
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_duplicate_exports_only", output);
}

#[test]
fn markdown_circular_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.circular_dependencies.push(CircularDependency {
        files: vec![root.join("src/a.ts"), root.join("src/b.ts")],
        length: 2,
        line: 3,
        col: 0,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_circular_deps_only", output);
}

#[test]
fn markdown_type_only_deps_only_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.type_only_dependencies.push(TypeOnlyDependency {
        package_name: "zod".to_string(),
        path: root.join("package.json"),
        line: 8,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_type_only_deps_only", output);
}

#[test]
fn markdown_re_export_variant_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_exports.push(UnusedExport {
        path: root.join("src/index.ts"),
        export_name: "reExportedFn".to_string(),
        is_type_only: false,
        line: 1,
        col: 0,
        span_start: 0,
        is_re_export: true,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_re_export_variant", output);
}

#[test]
fn markdown_workspace_dep_snapshot() {
    let root = PathBuf::from("/project");
    let mut results = AnalysisResults::default();
    results.unused_dependencies.push(UnusedDependency {
        package_name: "lodash".to_string(),
        location: DependencyLocation::Dependencies,
        path: root.join("packages/ui/package.json"),
        line: 5,
    });
    let output = build_markdown(&results, &root);
    insta::assert_snapshot!("markdown_workspace_deps", output);
}

// ── Health report snapshots ─────────────────────────────────────

/// Build a minimal health report with one finding for snapshot tests.
fn sample_health_report(root: &Path) -> HealthReport {
    HealthReport {
        findings: vec![HealthFinding {
            path: root.join("src/complex.ts"),
            name: "processData".to_string(),
            line: 42,
            col: 0,
            cyclomatic: 25,
            cognitive: 30,
            line_count: 120,
            exceeded: ExceededThreshold::Both,
        }],
        summary: HealthSummary {
            files_analyzed: 50,
            functions_analyzed: 200,
            functions_above_threshold: 1,
            max_cyclomatic_threshold: 20,
            max_cognitive_threshold: 15,
            files_scored: None,
            average_maintainability: None,
        },
        vital_signs: None,
        file_scores: vec![],
        hotspots: vec![],
        hotspot_summary: None,
        targets: vec![],
        target_thresholds: None,
    }
}

/// Build an empty health report (no findings).
fn empty_health_report() -> HealthReport {
    HealthReport {
        findings: vec![],
        summary: HealthSummary {
            files_analyzed: 50,
            functions_analyzed: 200,
            functions_above_threshold: 0,
            max_cyclomatic_threshold: 20,
            max_cognitive_threshold: 15,
            files_scored: None,
            average_maintainability: None,
        },
        vital_signs: None,
        file_scores: vec![],
        hotspots: vec![],
        hotspot_summary: None,
        targets: vec![],
        target_thresholds: None,
    }
}

#[test]
fn markdown_health_output_snapshot() {
    let root = PathBuf::from("/project");
    let report = sample_health_report(&root);
    let output = build_health_markdown(&report, &root);
    insta::assert_snapshot!("markdown_health_output", output);
}

#[test]
fn markdown_health_empty_snapshot() {
    let root = PathBuf::from("/project");
    let report = empty_health_report();
    let output = build_health_markdown(&report, &root);
    insta::assert_snapshot!("markdown_health_empty", output);
}

#[test]
fn markdown_health_with_vital_signs_snapshot() {
    let root = PathBuf::from("/project");
    let mut report = sample_health_report(&root);
    report.vital_signs = Some(VitalSigns {
        dead_file_pct: Some(3.2),
        dead_export_pct: Some(8.1),
        avg_cyclomatic: 4.7,
        p90_cyclomatic: 12,
        duplication_pct: None,
        hotspot_count: None,
        maintainability_avg: Some(72.4),
        unused_dep_count: Some(3),
        circular_dep_count: Some(1),
    });
    let output = build_health_markdown(&report, &root);
    insta::assert_snapshot!("markdown_health_with_vital_signs", output);
}

fn redact_health_sarif_version(json_str: &str) -> String {
    json_str.replace(
        &format!(
            "\"name\": \"fallow\",\n          \"version\": \"{}\"",
            env!("CARGO_PKG_VERSION")
        ),
        "\"name\": \"fallow\",\n          \"version\": \"[VERSION]\"",
    )
}

#[test]
fn sarif_health_output_snapshot() {
    let root = PathBuf::from("/project");
    let report = sample_health_report(&root);
    let sarif = build_health_sarif(&report, &root);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!(
        "sarif_health_output",
        redact_health_sarif_version(&json_str)
    );
}

#[test]
fn sarif_health_empty_snapshot() {
    let root = PathBuf::from("/project");
    let report = empty_health_report();
    let sarif = build_health_sarif(&report, &root);
    let json_str = serde_json::to_string_pretty(&sarif).expect("should serialize");
    insta::assert_snapshot!("sarif_health_empty", redact_health_sarif_version(&json_str));
}

#[test]
fn codeclimate_health_output_snapshot() {
    let root = PathBuf::from("/project");
    let report = sample_health_report(&root);
    let cc = build_health_codeclimate(&report, &root);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_health_output", json_str);
}

#[test]
fn codeclimate_health_empty_snapshot() {
    let root = PathBuf::from("/project");
    let report = empty_health_report();
    let cc = build_health_codeclimate(&report, &root);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_health_empty", json_str);
}

// ── Duplication report snapshots ────────────────────────────────

/// Build a sample duplication report for snapshot tests.
fn sample_duplication_report(root: &Path) -> DuplicationReport {
    DuplicationReport {
        clone_groups: vec![CloneGroup {
            instances: vec![
                CloneInstance {
                    file: root.join("src/utils.ts"),
                    start_line: 10,
                    end_line: 20,
                    start_col: 0,
                    end_col: 1,
                    fragment:
                        "function validate(input) {\n  if (!input) return false;\n  return true;\n}"
                            .to_string(),
                },
                CloneInstance {
                    file: root.join("src/helpers.ts"),
                    start_line: 5,
                    end_line: 15,
                    start_col: 0,
                    end_col: 1,
                    fragment:
                        "function validate(input) {\n  if (!input) return false;\n  return true;\n}"
                            .to_string(),
                },
            ],
            token_count: 25,
            line_count: 11,
        }],
        clone_families: vec![],
        stats: DuplicationStats {
            total_files: 100,
            files_with_clones: 2,
            total_lines: 5000,
            duplicated_lines: 11,
            total_tokens: 25000,
            duplicated_tokens: 25,
            clone_groups: 1,
            clone_instances: 2,
            duplication_percentage: 0.22,
        },
    }
}

#[test]
fn markdown_duplication_output_snapshot() {
    let root = PathBuf::from("/project");
    let report = sample_duplication_report(&root);
    let output = build_duplication_markdown(&report, &root);
    insta::assert_snapshot!("markdown_duplication_output", output);
}

#[test]
fn markdown_duplication_empty_snapshot() {
    let root = PathBuf::from("/project");
    let report = DuplicationReport::default();
    let output = build_duplication_markdown(&report, &root);
    insta::assert_snapshot!("markdown_duplication_empty", output);
}

#[test]
fn codeclimate_duplication_output_snapshot() {
    let root = PathBuf::from("/project");
    let report = sample_duplication_report(&root);
    let cc = build_duplication_codeclimate(&report, &root);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_duplication_output", json_str);
}

#[test]
fn codeclimate_duplication_empty_snapshot() {
    let root = PathBuf::from("/project");
    let report = DuplicationReport::default();
    let cc = build_duplication_codeclimate(&report, &root);
    let json_str = serde_json::to_string_pretty(&cc).expect("should serialize");
    insta::assert_snapshot!("codeclimate_duplication_empty", json_str);
}
