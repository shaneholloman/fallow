use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;

use fallow_core::duplicates::DuplicationReport;

/// Strip the project root from a path to produce a portable relative key.
fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Baseline data for comparison.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct BaselineData {
    pub unused_files: Vec<String>,
    pub unused_exports: Vec<String>,
    pub unused_types: Vec<String>,
    pub unused_dependencies: Vec<String>,
    pub unused_dev_dependencies: Vec<String>,
    /// Circular dependency chains, keyed by sorted file paths joined with `->`.
    #[serde(default)]
    pub circular_dependencies: Vec<String>,
    /// Unused optional dependencies, keyed by package name.
    #[serde(default)]
    pub unused_optional_dependencies: Vec<String>,
    /// Unused enum members, keyed by `file:parent.member`.
    #[serde(default)]
    pub unused_enum_members: Vec<String>,
    /// Unused class members, keyed by `file:parent.member`.
    #[serde(default)]
    pub unused_class_members: Vec<String>,
    /// Unresolved imports, keyed by `file:specifier`.
    #[serde(default)]
    pub unresolved_imports: Vec<String>,
    /// Unlisted dependencies, keyed by package name.
    #[serde(default)]
    pub unlisted_dependencies: Vec<String>,
    /// Duplicate exports, keyed by export name.
    #[serde(default)]
    pub duplicate_exports: Vec<String>,
    /// Type-only dependencies, keyed by package name.
    #[serde(default)]
    pub type_only_dependencies: Vec<String>,
    /// Test-only dependencies, keyed by package name.
    #[serde(default)]
    pub test_only_dependencies: Vec<String>,
    /// Boundary violations, keyed by `from_path->to_path`.
    #[serde(default)]
    pub boundary_violations: Vec<String>,
}

impl BaselineData {
    pub fn from_results(results: &fallow_core::results::AnalysisResults, root: &Path) -> Self {
        Self {
            unused_files: results
                .unused_files
                .iter()
                .map(|f| relative_path(&f.path, root))
                .collect(),
            unused_exports: results
                .unused_exports
                .iter()
                .map(|e| format!("{}:{}", relative_path(&e.path, root), e.export_name))
                .collect(),
            unused_types: results
                .unused_types
                .iter()
                .map(|e| format!("{}:{}", relative_path(&e.path, root), e.export_name))
                .collect(),
            unused_dependencies: results
                .unused_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            unused_dev_dependencies: results
                .unused_dev_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            circular_dependencies: results
                .circular_dependencies
                .iter()
                .map(|c| circular_dep_key(c, root))
                .collect(),
            unused_optional_dependencies: results
                .unused_optional_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            unused_enum_members: results
                .unused_enum_members
                .iter()
                .map(|m| {
                    format!(
                        "{}:{}.{}",
                        relative_path(&m.path, root),
                        m.parent_name,
                        m.member_name
                    )
                })
                .collect(),
            unused_class_members: results
                .unused_class_members
                .iter()
                .map(|m| {
                    format!(
                        "{}:{}.{}",
                        relative_path(&m.path, root),
                        m.parent_name,
                        m.member_name
                    )
                })
                .collect(),
            unresolved_imports: results
                .unresolved_imports
                .iter()
                .map(|i| format!("{}:{}", relative_path(&i.path, root), i.specifier))
                .collect(),
            unlisted_dependencies: results
                .unlisted_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            duplicate_exports: results
                .duplicate_exports
                .iter()
                .map(|d| duplicate_export_key(d, root))
                .collect(),
            type_only_dependencies: results
                .type_only_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            test_only_dependencies: results
                .test_only_dependencies
                .iter()
                .map(|d| d.package_name.clone())
                .collect(),
            boundary_violations: results
                .boundary_violations
                .iter()
                .map(|v| boundary_violation_key(v, root))
                .collect(),
        }
    }
}

/// Generate a stable key for a boundary violation: `from_path->to_path`.
fn boundary_violation_key(v: &fallow_core::results::BoundaryViolation, root: &Path) -> String {
    format!(
        "{}->{}",
        relative_path(&v.from_path, root),
        relative_path(&v.to_path, root),
    )
}

/// Generate a stable key for a duplicate export: `name|sorted_paths`.
fn duplicate_export_key(dup: &fallow_core::results::DuplicateExport, root: &Path) -> String {
    let mut locs: Vec<String> = dup
        .locations
        .iter()
        .map(|l| relative_path(&l.path, root))
        .collect();
    locs.sort();
    format!("{}|{}", dup.export_name, locs.join("|"))
}

/// Generate a stable key for a circular dependency based on sorted file paths.
fn circular_dep_key(dep: &fallow_core::results::CircularDependency, root: &Path) -> String {
    let mut paths: Vec<String> = dep.files.iter().map(|f| relative_path(f, root)).collect();
    paths.sort();
    paths.join("->")
}

/// Filter results to only include issues not present in the baseline.
pub fn filter_new_issues(
    mut results: fallow_core::results::AnalysisResults,
    baseline: &BaselineData,
    root: &Path,
) -> fallow_core::results::AnalysisResults {
    let baseline_files: FxHashSet<&str> =
        baseline.unused_files.iter().map(String::as_str).collect();
    let baseline_exports: FxHashSet<&str> =
        baseline.unused_exports.iter().map(String::as_str).collect();
    let baseline_types: FxHashSet<&str> =
        baseline.unused_types.iter().map(String::as_str).collect();
    let baseline_deps: FxHashSet<&str> = baseline
        .unused_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    let baseline_dev_deps: FxHashSet<&str> = baseline
        .unused_dev_dependencies
        .iter()
        .map(String::as_str)
        .collect();

    results
        .unused_files
        .retain(|f| !baseline_files.contains(relative_path(&f.path, root).as_str()));
    results.unused_exports.retain(|e| {
        let key = format!("{}:{}", relative_path(&e.path, root), e.export_name);
        !baseline_exports.contains(key.as_str())
    });
    results.unused_types.retain(|e| {
        let key = format!("{}:{}", relative_path(&e.path, root), e.export_name);
        !baseline_types.contains(key.as_str())
    });
    results
        .unused_dependencies
        .retain(|d| !baseline_deps.contains(d.package_name.as_str()));
    results
        .unused_dev_dependencies
        .retain(|d| !baseline_dev_deps.contains(d.package_name.as_str()));

    let baseline_circular: FxHashSet<&str> = baseline
        .circular_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    results.circular_dependencies.retain(|c| {
        let key = circular_dep_key(c, root);
        !baseline_circular.contains(key.as_str())
    });

    let baseline_optional_deps: FxHashSet<&str> = baseline
        .unused_optional_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    results
        .unused_optional_dependencies
        .retain(|d| !baseline_optional_deps.contains(d.package_name.as_str()));

    let baseline_enum_members: FxHashSet<&str> = baseline
        .unused_enum_members
        .iter()
        .map(String::as_str)
        .collect();
    results.unused_enum_members.retain(|m| {
        let key = format!(
            "{}:{}.{}",
            relative_path(&m.path, root),
            m.parent_name,
            m.member_name
        );
        !baseline_enum_members.contains(key.as_str())
    });

    let baseline_class_members: FxHashSet<&str> = baseline
        .unused_class_members
        .iter()
        .map(String::as_str)
        .collect();
    results.unused_class_members.retain(|m| {
        let key = format!(
            "{}:{}.{}",
            relative_path(&m.path, root),
            m.parent_name,
            m.member_name
        );
        !baseline_class_members.contains(key.as_str())
    });

    let baseline_unresolved: FxHashSet<&str> = baseline
        .unresolved_imports
        .iter()
        .map(String::as_str)
        .collect();
    results.unresolved_imports.retain(|i| {
        let key = format!("{}:{}", relative_path(&i.path, root), i.specifier);
        !baseline_unresolved.contains(key.as_str())
    });

    let baseline_unlisted: FxHashSet<&str> = baseline
        .unlisted_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    results
        .unlisted_dependencies
        .retain(|d| !baseline_unlisted.contains(d.package_name.as_str()));

    let baseline_dup_exports: FxHashSet<&str> = baseline
        .duplicate_exports
        .iter()
        .map(String::as_str)
        .collect();
    results.duplicate_exports.retain(|d| {
        let key = duplicate_export_key(d, root);
        !baseline_dup_exports.contains(key.as_str())
    });

    let baseline_type_only: FxHashSet<&str> = baseline
        .type_only_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    results
        .type_only_dependencies
        .retain(|d| !baseline_type_only.contains(d.package_name.as_str()));

    let baseline_test_only: FxHashSet<&str> = baseline
        .test_only_dependencies
        .iter()
        .map(String::as_str)
        .collect();
    results
        .test_only_dependencies
        .retain(|d| !baseline_test_only.contains(d.package_name.as_str()));

    let baseline_boundary: FxHashSet<&str> = baseline
        .boundary_violations
        .iter()
        .map(String::as_str)
        .collect();
    results.boundary_violations.retain(|v| {
        let key = boundary_violation_key(v, root);
        !baseline_boundary.contains(key.as_str())
    });

    results
}

/// Baseline data for duplication comparison.
///
/// Each clone group is keyed by a canonical string derived from its sorted
/// (`file:start_line-end_line`) instance locations. This allows stable comparison
/// across runs even if group ordering changes.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct DuplicationBaselineData {
    /// Clone group keys: sorted list of `file:start-end` per group.
    pub clone_groups: Vec<String>,
}

impl DuplicationBaselineData {
    /// Build a duplication baseline from the current report.
    pub fn from_report(report: &DuplicationReport, root: &Path) -> Self {
        Self {
            clone_groups: report
                .clone_groups
                .iter()
                .map(|g| clone_group_key(g, root))
                .collect(),
        }
    }
}

/// Generate a stable key for a clone group based on its instance locations.
fn clone_group_key(group: &fallow_core::duplicates::CloneGroup, root: &Path) -> String {
    let mut parts: Vec<String> = group
        .instances
        .iter()
        .map(|i| {
            let relative = i
                .file
                .strip_prefix(root)
                .unwrap_or(&i.file)
                .to_string_lossy()
                .replace('\\', "/");
            format!("{}:{}-{}", relative, i.start_line, i.end_line)
        })
        .collect();
    parts.sort();
    parts.join("|")
}

/// Filter a duplication report to only include clone groups not present in the baseline.
pub fn filter_new_clone_groups(
    mut report: DuplicationReport,
    baseline: &DuplicationBaselineData,
    root: &Path,
) -> DuplicationReport {
    let baseline_keys: FxHashSet<&str> = baseline.clone_groups.iter().map(String::as_str).collect();

    report.clone_groups.retain(|g| {
        let key = clone_group_key(g, root);
        !baseline_keys.contains(key.as_str())
    });

    // Re-generate families from the filtered groups
    report.clone_families =
        fallow_core::duplicates::families::group_into_families(&report.clone_groups, root);
    report.mirrored_directories = fallow_core::duplicates::families::detect_mirrored_directories(
        &report.clone_families,
        root,
    );

    // Re-compute stats for the filtered groups
    report.stats = recompute_stats(&report);

    report
}

/// Recompute duplication statistics after filtering (baseline or `--changed-since`).
///
/// Uses per-file line deduplication (matching `compute_stats` in `detect.rs`)
/// so overlapping clone instances don't inflate the duplicated line count.
pub fn recompute_stats(report: &DuplicationReport) -> fallow_core::duplicates::DuplicationStats {
    let mut files_with_clones: FxHashSet<&Path> = FxHashSet::default();
    let mut file_dup_lines: FxHashMap<&Path, FxHashSet<usize>> = FxHashMap::default();
    let mut duplicated_tokens = 0usize;
    let mut clone_instances = 0usize;

    for group in &report.clone_groups {
        for instance in &group.instances {
            files_with_clones.insert(&instance.file);
            clone_instances += 1;
            let lines = file_dup_lines.entry(&instance.file).or_default();
            for line in instance.start_line..=instance.end_line {
                lines.insert(line);
            }
        }
        duplicated_tokens += group.token_count * group.instances.len();
    }

    let duplicated_lines: usize = file_dup_lines.values().map(FxHashSet::len).sum();

    fallow_core::duplicates::DuplicationStats {
        total_files: report.stats.total_files,
        files_with_clones: files_with_clones.len(),
        total_lines: report.stats.total_lines,
        duplicated_lines,
        total_tokens: report.stats.total_tokens,
        duplicated_tokens,
        clone_groups: report.clone_groups.len(),
        clone_instances,
        duplication_percentage: if report.stats.total_lines > 0 {
            (duplicated_lines as f64 / report.stats.total_lines as f64) * 100.0
        } else {
            0.0
        },
    }
}

// ── Health baseline ─────────────────────────────────────────────────

/// Baseline data for health (complexity) comparison.
///
/// Each finding is keyed by `relative_path:function_name:line` for stable comparison.
/// Target keys use `relative_path:category` so category changes surface as new targets.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct HealthBaselineData {
    pub findings: Vec<String>,
    /// Refactoring target keys: `relative_path:category`.
    #[serde(default)]
    pub target_keys: Vec<String>,
}

impl HealthBaselineData {
    /// Build a health baseline from findings and targets.
    pub fn from_findings(
        findings: &[crate::health_types::HealthFinding],
        targets: &[crate::health_types::RefactoringTarget],
        root: &Path,
    ) -> Self {
        Self {
            findings: findings
                .iter()
                .map(|f| health_finding_key(f, root))
                .collect(),
            target_keys: targets
                .iter()
                .map(|t| target_baseline_key(t, root))
                .collect(),
        }
    }
}

/// Generate a stable key for a refactoring target: `relative_path:category`.
fn target_baseline_key(target: &crate::health_types::RefactoringTarget, root: &Path) -> String {
    let relative = target
        .path
        .strip_prefix(root)
        .unwrap_or(&target.path)
        .to_string_lossy()
        .replace('\\', "/");
    format!("{}:{}", relative, target.category.label())
}

/// Generate a stable key for a health finding.
fn health_finding_key(finding: &crate::health_types::HealthFinding, root: &Path) -> String {
    let relative = finding
        .path
        .strip_prefix(root)
        .unwrap_or(&finding.path)
        .to_string_lossy()
        .replace('\\', "/");
    format!("{}:{}:{}", relative, finding.name, finding.line)
}

/// Filter health findings to only include those not present in the baseline.
pub fn filter_new_health_findings(
    mut findings: Vec<crate::health_types::HealthFinding>,
    baseline: &HealthBaselineData,
    root: &Path,
) -> Vec<crate::health_types::HealthFinding> {
    let baseline_keys: FxHashSet<&str> = baseline.findings.iter().map(String::as_str).collect();
    findings.retain(|f| {
        let key = health_finding_key(f, root);
        !baseline_keys.contains(key.as_str())
    });
    findings
}

/// Filter refactoring targets to only include those not present in the baseline.
pub fn filter_new_health_targets(
    mut targets: Vec<crate::health_types::RefactoringTarget>,
    baseline: &HealthBaselineData,
    root: &Path,
) -> Vec<crate::health_types::RefactoringTarget> {
    let baseline_keys: FxHashSet<&str> = baseline.target_keys.iter().map(String::as_str).collect();
    targets.retain(|t| {
        let key = target_baseline_key(t, root);
        !baseline_keys.contains(key.as_str())
    });
    targets
}

/// Per-category delta between current results and a baseline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CategoryDelta {
    pub current: usize,
    pub baseline: usize,
    pub delta: i64,
}

/// Deltas between current analysis results and a saved baseline.
///
/// Used in combined mode to show +/- counts in the failure summary and
/// to emit `baseline_deltas` in JSON output.
#[derive(Debug, Clone)]
pub struct BaselineDeltas {
    /// Net change in total issue count (positive = more issues).
    pub total_delta: i64,
    /// Per-category deltas keyed by category name.
    pub per_category: Vec<(String, CategoryDelta)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use fallow_core::duplicates::{CloneGroup, CloneInstance, DuplicationReport, DuplicationStats};
    use fallow_core::results::{
        AnalysisResults, DependencyLocation, UnusedDependency, UnusedExport, UnusedFile,
    };
    use std::path::PathBuf;

    fn make_results() -> AnalysisResults {
        AnalysisResults {
            unused_files: vec![
                UnusedFile {
                    path: PathBuf::from("src/old.ts"),
                },
                UnusedFile {
                    path: PathBuf::from("src/dead.ts"),
                },
            ],
            unused_exports: vec![UnusedExport {
                path: PathBuf::from("src/utils.ts"),
                export_name: "helperA".to_string(),
                is_type_only: false,
                line: 5,
                col: 0,
                span_start: 40,
                is_re_export: false,
            }],
            unused_types: vec![UnusedExport {
                path: PathBuf::from("src/types.ts"),
                export_name: "OldType".to_string(),
                is_type_only: true,
                line: 10,
                col: 0,
                span_start: 100,
                is_re_export: false,
            }],
            unused_dependencies: vec![UnusedDependency {
                package_name: "lodash".to_string(),
                location: DependencyLocation::Dependencies,
                path: PathBuf::from("package.json"),
                line: 5,
            }],
            unused_dev_dependencies: vec![UnusedDependency {
                package_name: "jest".to_string(),
                location: DependencyLocation::DevDependencies,
                path: PathBuf::from("package.json"),
                line: 5,
            }],
            ..Default::default()
        }
    }

    // ── BaselineData round-trip ──────────────────────────────────

    #[test]
    fn baseline_from_results_captures_all_fields() {
        let results = make_results();
        let baseline = BaselineData::from_results(&results, Path::new(""));
        assert_eq!(baseline.unused_files.len(), 2);
        assert!(baseline.unused_files.contains(&"src/old.ts".to_string()));
        assert!(baseline.unused_files.contains(&"src/dead.ts".to_string()));
        assert_eq!(baseline.unused_exports, vec!["src/utils.ts:helperA"]);
        assert_eq!(baseline.unused_types, vec!["src/types.ts:OldType"]);
        assert_eq!(baseline.unused_dependencies, vec!["lodash"]);
        assert_eq!(baseline.unused_dev_dependencies, vec!["jest"]);
    }

    #[test]
    fn baseline_serialization_roundtrip() {
        let results = make_results();
        let baseline = BaselineData::from_results(&results, Path::new(""));
        let json = serde_json::to_string(&baseline).unwrap();
        let deserialized: BaselineData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.unused_files, baseline.unused_files);
        assert_eq!(deserialized.unused_exports, baseline.unused_exports);
        assert_eq!(deserialized.unused_types, baseline.unused_types);
        assert_eq!(
            deserialized.unused_dependencies,
            baseline.unused_dependencies
        );
        assert_eq!(
            deserialized.unused_dev_dependencies,
            baseline.unused_dev_dependencies
        );
    }

    // ── filter_new_issues ────────────────────────────────────────

    #[test]
    fn filter_removes_baseline_issues() {
        let results = make_results();
        let baseline = BaselineData::from_results(&results, Path::new(""));
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert!(
            filtered.unused_files.is_empty(),
            "all files were in baseline"
        );
        assert!(
            filtered.unused_exports.is_empty(),
            "all exports were in baseline"
        );
        assert!(
            filtered.unused_types.is_empty(),
            "all types were in baseline"
        );
        assert!(
            filtered.unused_dependencies.is_empty(),
            "all deps were in baseline"
        );
        assert!(
            filtered.unused_dev_dependencies.is_empty(),
            "all dev deps were in baseline"
        );
    }

    #[test]
    fn filter_keeps_new_issues_not_in_baseline() {
        let baseline = BaselineData {
            unused_files: vec!["src/old.ts".to_string()],
            unused_exports: vec![],
            unused_types: vec![],
            unused_dependencies: vec![],
            unused_dev_dependencies: vec![],
            circular_dependencies: vec![],
            unused_optional_dependencies: vec![],
            unused_enum_members: vec![],
            unused_class_members: vec![],
            unresolved_imports: vec![],
            unlisted_dependencies: vec![],
            duplicate_exports: vec![],
            type_only_dependencies: vec![],
            test_only_dependencies: vec![],
            boundary_violations: vec![],
        };
        let results = AnalysisResults {
            unused_files: vec![
                UnusedFile {
                    path: PathBuf::from("src/old.ts"),
                },
                UnusedFile {
                    path: PathBuf::from("src/new-dead.ts"),
                },
            ],
            ..Default::default()
        };
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert_eq!(filtered.unused_files.len(), 1);
        assert_eq!(
            filtered.unused_files[0].path,
            PathBuf::from("src/new-dead.ts")
        );
    }

    #[test]
    fn filter_with_empty_baseline_keeps_all() {
        let baseline = BaselineData {
            unused_files: vec![],
            unused_exports: vec![],
            unused_types: vec![],
            unused_dependencies: vec![],
            unused_dev_dependencies: vec![],
            circular_dependencies: vec![],
            unused_optional_dependencies: vec![],
            unused_enum_members: vec![],
            unused_class_members: vec![],
            unresolved_imports: vec![],
            unlisted_dependencies: vec![],
            duplicate_exports: vec![],
            type_only_dependencies: vec![],
            test_only_dependencies: vec![],
            boundary_violations: vec![],
        };
        let results = make_results();
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert_eq!(filtered.unused_files.len(), 2);
        assert_eq!(filtered.unused_exports.len(), 1);
    }

    #[test]
    fn filter_new_exports_by_file_and_name() {
        let baseline = BaselineData {
            unused_files: vec![],
            unused_exports: vec!["src/utils.ts:helperA".to_string()],
            unused_types: vec![],
            unused_dependencies: vec![],
            unused_dev_dependencies: vec![],
            circular_dependencies: vec![],
            unused_optional_dependencies: vec![],
            unused_enum_members: vec![],
            unused_class_members: vec![],
            unresolved_imports: vec![],
            unlisted_dependencies: vec![],
            duplicate_exports: vec![],
            type_only_dependencies: vec![],
            test_only_dependencies: vec![],
            boundary_violations: vec![],
        };
        let results = AnalysisResults {
            unused_exports: vec![
                UnusedExport {
                    path: PathBuf::from("src/utils.ts"),
                    export_name: "helperA".to_string(),
                    is_type_only: false,
                    line: 5,
                    col: 0,
                    span_start: 40,
                    is_re_export: false,
                },
                UnusedExport {
                    path: PathBuf::from("src/utils.ts"),
                    export_name: "helperB".to_string(),
                    is_type_only: false,
                    line: 10,
                    col: 0,
                    span_start: 80,
                    is_re_export: false,
                },
            ],
            ..Default::default()
        };
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert_eq!(filtered.unused_exports.len(), 1);
        assert_eq!(filtered.unused_exports[0].export_name, "helperB");
    }

    // ── DuplicationBaselineData ──────────────────────────────────

    fn make_clone_group(instances: Vec<(&str, usize, usize)>) -> CloneGroup {
        CloneGroup {
            instances: instances
                .into_iter()
                .map(|(file, start, end)| CloneInstance {
                    file: PathBuf::from(file),
                    start_line: start,
                    end_line: end,
                    start_col: 0,
                    end_col: 0,
                    fragment: String::new(),
                })
                .collect(),
            token_count: 50,
            line_count: 10,
        }
    }

    fn make_duplication_report(groups: Vec<CloneGroup>) -> DuplicationReport {
        DuplicationReport {
            clone_groups: groups,
            clone_families: vec![],
            mirrored_directories: vec![],
            stats: DuplicationStats {
                total_files: 10,
                files_with_clones: 2,
                total_lines: 1000,
                duplicated_lines: 100,
                total_tokens: 5000,
                duplicated_tokens: 500,
                clone_groups: 1,
                clone_instances: 2,
                duplication_percentage: 10.0,
            },
        }
    }

    #[test]
    fn clone_group_key_is_deterministic() {
        let root = Path::new("/project");
        let group = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let key1 = clone_group_key(&group, root);
        let key2 = clone_group_key(&group, root);
        assert_eq!(key1, key2);
    }

    #[test]
    fn clone_group_key_is_sorted() {
        let root = Path::new("/project");
        // Order of instances in group shouldn't matter for the key
        let group_ab = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let group_ba = make_clone_group(vec![
            ("/project/src/b.ts", 5, 15),
            ("/project/src/a.ts", 1, 10),
        ]);
        assert_eq!(
            clone_group_key(&group_ab, root),
            clone_group_key(&group_ba, root),
            "key should be stable regardless of instance order"
        );
    }

    #[test]
    fn duplication_baseline_roundtrip() {
        let root = Path::new("/project");
        let group = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let report = make_duplication_report(vec![group]);
        let baseline = DuplicationBaselineData::from_report(&report, root);
        let json = serde_json::to_string(&baseline).unwrap();
        let deserialized: DuplicationBaselineData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.clone_groups, baseline.clone_groups);
    }

    #[test]
    fn filter_new_clone_groups_removes_baseline() {
        let root = Path::new("/project");
        let group = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let report = make_duplication_report(vec![group]);
        let baseline = DuplicationBaselineData::from_report(&report, root);
        let filtered = filter_new_clone_groups(report, &baseline, root);
        assert!(
            filtered.clone_groups.is_empty(),
            "baseline group should be filtered out"
        );
    }

    #[test]
    fn filter_new_clone_groups_keeps_new_groups() {
        let root = Path::new("/project");
        let baseline_group = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let new_group = make_clone_group(vec![
            ("/project/src/c.ts", 20, 30),
            ("/project/src/d.ts", 25, 35),
        ]);
        let baseline_report = make_duplication_report(vec![baseline_group]);
        let baseline = DuplicationBaselineData::from_report(&baseline_report, root);

        let report = make_duplication_report(vec![
            make_clone_group(vec![
                ("/project/src/a.ts", 1, 10),
                ("/project/src/b.ts", 5, 15),
            ]),
            new_group,
        ]);
        let filtered = filter_new_clone_groups(report, &baseline, root);
        assert_eq!(
            filtered.clone_groups.len(),
            1,
            "only the new group should remain"
        );
    }

    #[test]
    fn recompute_stats_after_filtering() {
        let root = Path::new("/project");
        let group = make_clone_group(vec![
            ("/project/src/a.ts", 1, 10),
            ("/project/src/b.ts", 5, 15),
        ]);
        let report = make_duplication_report(vec![group]);
        let baseline = DuplicationBaselineData::from_report(&report, root);
        let filtered = filter_new_clone_groups(report, &baseline, root);
        assert_eq!(filtered.stats.clone_groups, 0);
        assert_eq!(filtered.stats.clone_instances, 0);
        assert_eq!(filtered.stats.duplicated_lines, 0);
    }

    #[test]
    fn recompute_stats_zero_total_lines() {
        let report = DuplicationReport {
            clone_groups: vec![],
            clone_families: vec![],
            mirrored_directories: vec![],
            stats: DuplicationStats {
                total_files: 0,
                files_with_clones: 0,
                total_lines: 0,
                duplicated_lines: 0,
                total_tokens: 0,
                duplicated_tokens: 0,
                clone_groups: 0,
                clone_instances: 0,
                duplication_percentage: 0.0,
            },
        };
        let stats = super::recompute_stats(&report);
        assert!((stats.duplication_percentage - 0.0).abs() < f64::EPSILON);
    }

    // ── HealthBaselineData ──────────────────────────────────────────

    fn make_health_finding(
        root: &Path,
        name: &str,
        line: u32,
    ) -> crate::health_types::HealthFinding {
        crate::health_types::HealthFinding {
            path: root.join("src/utils.ts"),
            name: name.to_string(),
            line,
            col: 0,
            cyclomatic: 25,
            cognitive: 30,
            line_count: 80,
            exceeded: crate::health_types::ExceededThreshold::Both,
        }
    }

    #[test]
    fn health_baseline_roundtrip() {
        let root = PathBuf::from("/project");
        let findings = vec![make_health_finding(&root, "parseExpression", 42)];
        let baseline = HealthBaselineData::from_findings(&findings, &[], &root);
        let json = serde_json::to_string(&baseline).unwrap();
        let deserialized: HealthBaselineData = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.findings, baseline.findings);
        assert_eq!(baseline.findings, vec!["src/utils.ts:parseExpression:42"]);
    }

    #[test]
    fn health_baseline_filters_known_findings() {
        let root = PathBuf::from("/project");
        let findings = vec![
            make_health_finding(&root, "parseExpression", 42),
            make_health_finding(&root, "newFunction", 100),
        ];
        let baseline = HealthBaselineData::from_findings(&findings[..1], &[], &root);
        let filtered = filter_new_health_findings(findings, &baseline, &root);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "newFunction");
    }

    #[test]
    fn health_baseline_empty_keeps_all() {
        let root = PathBuf::from("/project");
        let findings = vec![make_health_finding(&root, "parseExpression", 42)];
        let baseline = HealthBaselineData {
            findings: vec![],
            target_keys: vec![],
        };
        let filtered = filter_new_health_findings(findings, &baseline, &root);
        assert_eq!(filtered.len(), 1);
    }

    // ── circular_dep_key sort stability ─────────────────────────

    #[test]
    fn circular_dep_key_is_order_independent() {
        use fallow_core::results::CircularDependency;

        let dep_ab = CircularDependency {
            files: vec![PathBuf::from("src/a.ts"), PathBuf::from("src/b.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        let dep_ba = CircularDependency {
            files: vec![PathBuf::from("src/b.ts"), PathBuf::from("src/a.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        assert_eq!(
            super::circular_dep_key(&dep_ab, Path::new("")),
            super::circular_dep_key(&dep_ba, Path::new("")),
            "same files in different order should produce identical keys"
        );
    }

    #[test]
    fn circular_dep_key_different_files_different_keys() {
        use fallow_core::results::CircularDependency;

        let dep1 = CircularDependency {
            files: vec![PathBuf::from("src/a.ts"), PathBuf::from("src/b.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        let dep2 = CircularDependency {
            files: vec![PathBuf::from("src/a.ts"), PathBuf::from("src/c.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        assert_ne!(
            super::circular_dep_key(&dep1, Path::new("")),
            super::circular_dep_key(&dep2, Path::new("")),
        );
    }

    #[test]
    fn circular_dep_key_three_files_order_independent() {
        use fallow_core::results::CircularDependency;

        let dep_abc = CircularDependency {
            files: vec![
                PathBuf::from("src/a.ts"),
                PathBuf::from("src/b.ts"),
                PathBuf::from("src/c.ts"),
            ],
            length: 3,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        let dep_cab = CircularDependency {
            files: vec![
                PathBuf::from("src/c.ts"),
                PathBuf::from("src/a.ts"),
                PathBuf::from("src/b.ts"),
            ],
            length: 3,
            line: 1,
            col: 0,
            is_cross_package: false,
        };
        assert_eq!(
            super::circular_dep_key(&dep_abc, Path::new("")),
            super::circular_dep_key(&dep_cab, Path::new("")),
        );
    }

    // ── filter_new_issues: extended issue types ────────────────

    fn make_full_results() -> AnalysisResults {
        use fallow_core::extract::MemberKind;
        use fallow_core::results::*;

        let mut r = make_results();
        r.circular_dependencies.push(CircularDependency {
            files: vec![PathBuf::from("src/a.ts"), PathBuf::from("src/b.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        });
        r.unused_optional_dependencies.push(UnusedDependency {
            package_name: "fsevents".to_string(),
            location: DependencyLocation::OptionalDependencies,
            path: PathBuf::from("package.json"),
            line: 15,
        });
        r.unused_enum_members.push(UnusedMember {
            path: PathBuf::from("src/enums.ts"),
            parent_name: "Status".to_string(),
            member_name: "Deprecated".to_string(),
            kind: MemberKind::EnumMember,
            line: 8,
            col: 0,
        });
        r.unused_class_members.push(UnusedMember {
            path: PathBuf::from("src/service.ts"),
            parent_name: "UserService".to_string(),
            member_name: "legacy".to_string(),
            kind: MemberKind::ClassMethod,
            line: 42,
            col: 0,
        });
        r.unresolved_imports
            .push(fallow_core::results::UnresolvedImport {
                path: PathBuf::from("src/app.ts"),
                specifier: "./missing".to_string(),
                line: 3,
                col: 0,
                specifier_col: 0,
            });
        r.unlisted_dependencies
            .push(fallow_core::results::UnlistedDependency {
                package_name: "chalk".to_string(),
                imported_from: vec![],
            });
        r.duplicate_exports
            .push(fallow_core::results::DuplicateExport {
                export_name: "Config".to_string(),
                locations: vec![
                    fallow_core::results::DuplicateLocation {
                        path: PathBuf::from("src/a.ts"),
                        line: 1,
                        col: 0,
                    },
                    fallow_core::results::DuplicateLocation {
                        path: PathBuf::from("src/b.ts"),
                        line: 5,
                        col: 0,
                    },
                ],
            });
        r.type_only_dependencies
            .push(fallow_core::results::TypeOnlyDependency {
                package_name: "zod".to_string(),
                path: PathBuf::from("package.json"),
                line: 8,
            });
        r.test_only_dependencies
            .push(fallow_core::results::TestOnlyDependency {
                package_name: "vitest".to_string(),
                path: PathBuf::from("package.json"),
                line: 10,
            });
        r.boundary_violations
            .push(fallow_core::results::BoundaryViolation {
                from_path: PathBuf::from("src/ui/btn.ts"),
                to_path: PathBuf::from("src/db/query.ts"),
                from_zone: "ui".to_string(),
                to_zone: "db".to_string(),
                import_specifier: "../db/query".to_string(),
                line: 1,
                col: 0,
            });
        r
    }

    #[test]
    fn baseline_from_results_captures_all_extended_fields() {
        let results = make_full_results();
        let baseline = BaselineData::from_results(&results, Path::new(""));
        assert_eq!(baseline.circular_dependencies.len(), 1);
        assert_eq!(baseline.unused_optional_dependencies, vec!["fsevents"]);
        assert_eq!(baseline.unused_enum_members.len(), 1);
        assert!(baseline.unused_enum_members[0].contains("Status.Deprecated"));
        assert_eq!(baseline.unused_class_members.len(), 1);
        assert!(baseline.unused_class_members[0].contains("UserService.legacy"));
        assert_eq!(baseline.unresolved_imports.len(), 1);
        assert!(baseline.unresolved_imports[0].contains("./missing"));
        assert_eq!(baseline.unlisted_dependencies, vec!["chalk"]);
        assert_eq!(baseline.duplicate_exports.len(), 1);
        assert!(baseline.duplicate_exports[0].starts_with("Config|"));
        assert_eq!(baseline.type_only_dependencies, vec!["zod"]);
        assert_eq!(baseline.test_only_dependencies, vec!["vitest"]);
        assert_eq!(baseline.boundary_violations.len(), 1);
        assert!(baseline.boundary_violations[0].contains("->"));
    }

    #[test]
    fn filter_removes_all_extended_baseline_issues() {
        let results = make_full_results();
        let baseline = BaselineData::from_results(&results, Path::new(""));
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert!(filtered.circular_dependencies.is_empty());
        assert!(filtered.unused_optional_dependencies.is_empty());
        assert!(filtered.unused_enum_members.is_empty());
        assert!(filtered.unused_class_members.is_empty());
        assert!(filtered.unresolved_imports.is_empty());
        assert!(filtered.unlisted_dependencies.is_empty());
        assert!(filtered.duplicate_exports.is_empty());
        assert!(filtered.type_only_dependencies.is_empty());
        assert!(filtered.test_only_dependencies.is_empty());
        assert!(filtered.boundary_violations.is_empty());
    }

    #[test]
    fn filter_keeps_new_circular_deps() {
        use fallow_core::results::CircularDependency;
        let baseline = BaselineData {
            circular_dependencies: vec!["src/a.ts->src/b.ts".to_string()],
            ..BaselineData::from_results(&AnalysisResults::default(), Path::new(""))
        };
        let mut results = AnalysisResults::default();
        // One in baseline, one new
        results.circular_dependencies.push(CircularDependency {
            files: vec![PathBuf::from("src/a.ts"), PathBuf::from("src/b.ts")],
            length: 2,
            line: 1,
            col: 0,
            is_cross_package: false,
        });
        results.circular_dependencies.push(CircularDependency {
            files: vec![PathBuf::from("src/x.ts"), PathBuf::from("src/y.ts")],
            length: 2,
            line: 5,
            col: 0,
            is_cross_package: false,
        });
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert_eq!(filtered.circular_dependencies.len(), 1);
    }

    #[test]
    fn filter_keeps_new_boundary_violations() {
        use fallow_core::results::BoundaryViolation;
        let baseline = BaselineData {
            boundary_violations: vec!["src/a.ts->src/b.ts".to_string()],
            ..BaselineData::from_results(&AnalysisResults::default(), Path::new(""))
        };
        let mut results = AnalysisResults::default();
        results.boundary_violations.push(BoundaryViolation {
            from_path: PathBuf::from("src/a.ts"),
            to_path: PathBuf::from("src/b.ts"),
            from_zone: "a".to_string(),
            to_zone: "b".to_string(),
            import_specifier: "../b".to_string(),
            line: 1,
            col: 0,
        });
        results.boundary_violations.push(BoundaryViolation {
            from_path: PathBuf::from("src/new.ts"),
            to_path: PathBuf::from("src/secret.ts"),
            from_zone: "new".to_string(),
            to_zone: "secret".to_string(),
            import_specifier: "../secret".to_string(),
            line: 1,
            col: 0,
        });
        let filtered = filter_new_issues(results, &baseline, Path::new(""));
        assert_eq!(filtered.boundary_violations.len(), 1);
    }

    // ── filter_new_health_targets ──────────────────────────────

    #[test]
    fn health_targets_baseline_filters_known() {
        let root = PathBuf::from("/project");
        let targets = vec![
            crate::health_types::RefactoringTarget {
                path: root.join("src/complex.ts"),
                priority: 80.0,
                efficiency: 40.0,
                recommendation: "Split file".to_string(),
                category: crate::health_types::RecommendationCategory::SplitHighImpact,
                effort: crate::health_types::EffortEstimate::Medium,
                confidence: crate::health_types::Confidence::Medium,
                factors: vec![],
                evidence: None,
            },
            crate::health_types::RefactoringTarget {
                path: root.join("src/new-issue.ts"),
                priority: 60.0,
                efficiency: 30.0,
                recommendation: "Extract function".to_string(),
                category: crate::health_types::RecommendationCategory::ExtractComplexFunctions,
                effort: crate::health_types::EffortEstimate::Low,
                confidence: crate::health_types::Confidence::High,
                factors: vec![],
                evidence: None,
            },
        ];
        let baseline = HealthBaselineData::from_findings(&[], &targets[..1], &root);
        let filtered = filter_new_health_targets(targets, &baseline, &root);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, root.join("src/new-issue.ts"));
    }

    // ── duplicate_export_key ───────────────────────────────────

    #[test]
    fn duplicate_export_key_is_sorted() {
        use fallow_core::results::{DuplicateExport, DuplicateLocation};
        let dup_ab = DuplicateExport {
            export_name: "foo".to_string(),
            locations: vec![
                DuplicateLocation {
                    path: PathBuf::from("src/a.ts"),
                    line: 1,
                    col: 0,
                },
                DuplicateLocation {
                    path: PathBuf::from("src/b.ts"),
                    line: 5,
                    col: 0,
                },
            ],
        };
        let dup_ba = DuplicateExport {
            export_name: "foo".to_string(),
            locations: vec![
                DuplicateLocation {
                    path: PathBuf::from("src/b.ts"),
                    line: 5,
                    col: 0,
                },
                DuplicateLocation {
                    path: PathBuf::from("src/a.ts"),
                    line: 1,
                    col: 0,
                },
            ],
        };
        assert_eq!(
            super::duplicate_export_key(&dup_ab, Path::new("")),
            super::duplicate_export_key(&dup_ba, Path::new("")),
        );
    }

    // ── boundary_violation_key ─────────────────────────────────

    #[test]
    fn boundary_violation_key_format() {
        use fallow_core::results::BoundaryViolation;
        let v = BoundaryViolation {
            from_path: PathBuf::from("src/ui/btn.ts"),
            to_path: PathBuf::from("src/db/query.ts"),
            from_zone: "ui".to_string(),
            to_zone: "db".to_string(),
            import_specifier: "../db/query".to_string(),
            line: 1,
            col: 0,
        };
        let key = super::boundary_violation_key(&v, Path::new(""));
        assert_eq!(key, "src/ui/btn.ts->src/db/query.ts");
    }

    // ── cross-machine baseline portability (#87) ──────────────

    /// Regression test: baseline saved on one machine (different absolute root)
    /// must match issues found on another machine.
    #[test]
    fn baseline_keys_are_relative_to_root() {
        use fallow_core::results::BoundaryViolation;

        // Simulate saving baseline on a developer's machine
        let local_root = Path::new("/Users/dev/project");
        let mut results = AnalysisResults {
            unused_files: vec![UnusedFile {
                path: PathBuf::from("/Users/dev/project/src/old.ts"),
            }],
            unused_exports: vec![UnusedExport {
                path: PathBuf::from("/Users/dev/project/src/utils.ts"),
                export_name: "helper".to_string(),
                is_type_only: false,
                line: 5,
                col: 0,
                span_start: 40,
                is_re_export: false,
            }],
            boundary_violations: vec![BoundaryViolation {
                from_path: PathBuf::from("/Users/dev/project/src/ui/btn.ts"),
                to_path: PathBuf::from("/Users/dev/project/src/db/query.ts"),
                from_zone: "ui".to_string(),
                to_zone: "db".to_string(),
                import_specifier: "../db/query".to_string(),
                line: 1,
                col: 0,
            }],
            ..Default::default()
        };

        let baseline = BaselineData::from_results(&results, local_root);

        // Keys should be relative
        assert_eq!(baseline.unused_files, vec!["src/old.ts"]);
        assert_eq!(baseline.unused_exports, vec!["src/utils.ts:helper"]);
        assert_eq!(
            baseline.boundary_violations,
            vec!["src/ui/btn.ts->src/db/query.ts"]
        );

        // Simulate loading baseline on CI (different absolute root, same relative structure)
        let ci_root = Path::new("/home/runner/work/project/project");
        results.unused_files[0].path =
            PathBuf::from("/home/runner/work/project/project/src/old.ts");
        results.unused_exports[0].path =
            PathBuf::from("/home/runner/work/project/project/src/utils.ts");
        results.boundary_violations[0].from_path =
            PathBuf::from("/home/runner/work/project/project/src/ui/btn.ts");
        results.boundary_violations[0].to_path =
            PathBuf::from("/home/runner/work/project/project/src/db/query.ts");

        let filtered = filter_new_issues(results, &baseline, ci_root);
        assert!(
            filtered.unused_files.is_empty(),
            "baseline should filter unused files across different roots"
        );
        assert!(
            filtered.unused_exports.is_empty(),
            "baseline should filter unused exports across different roots"
        );
        assert!(
            filtered.boundary_violations.is_empty(),
            "baseline should filter boundary violations across different roots"
        );
    }
}
