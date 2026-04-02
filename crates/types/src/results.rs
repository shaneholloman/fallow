//! Analysis result types for all issue categories.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::extract::MemberKind;
use crate::serde_path;

/// Complete analysis results.
///
/// # Examples
///
/// ```
/// use fallow_types::results::{AnalysisResults, UnusedFile};
/// use std::path::PathBuf;
///
/// let mut results = AnalysisResults::default();
/// assert_eq!(results.total_issues(), 0);
/// assert!(!results.has_issues());
///
/// results.unused_files.push(UnusedFile {
///     path: PathBuf::from("src/dead.ts"),
/// });
/// assert_eq!(results.total_issues(), 1);
/// assert!(results.has_issues());
/// ```
#[derive(Debug, Default, Clone, Serialize)]
pub struct AnalysisResults {
    /// Files not reachable from any entry point.
    pub unused_files: Vec<UnusedFile>,
    /// Exports never imported by other modules.
    pub unused_exports: Vec<UnusedExport>,
    /// Type exports never imported by other modules.
    pub unused_types: Vec<UnusedExport>,
    /// Dependencies listed in package.json but never imported.
    pub unused_dependencies: Vec<UnusedDependency>,
    /// Dev dependencies listed in package.json but never imported.
    pub unused_dev_dependencies: Vec<UnusedDependency>,
    /// Optional dependencies listed in package.json but never imported.
    pub unused_optional_dependencies: Vec<UnusedDependency>,
    /// Enum members never accessed.
    pub unused_enum_members: Vec<UnusedMember>,
    /// Class members never accessed.
    pub unused_class_members: Vec<UnusedMember>,
    /// Import specifiers that could not be resolved.
    pub unresolved_imports: Vec<UnresolvedImport>,
    /// Dependencies used in code but not listed in package.json.
    pub unlisted_dependencies: Vec<UnlistedDependency>,
    /// Exports with the same name across multiple modules.
    pub duplicate_exports: Vec<DuplicateExport>,
    /// Production dependencies only used via type-only imports (could be devDependencies).
    /// Only populated in production mode.
    pub type_only_dependencies: Vec<TypeOnlyDependency>,
    /// Production dependencies only imported by test files (could be devDependencies).
    #[serde(default)]
    pub test_only_dependencies: Vec<TestOnlyDependency>,
    /// Circular dependency chains detected in the module graph.
    pub circular_dependencies: Vec<CircularDependency>,
    /// Imports that cross architecture boundary rules.
    #[serde(default)]
    pub boundary_violations: Vec<BoundaryViolation>,
    /// Usage counts for all exports across the project. Used by the LSP for Code Lens.
    /// Not included in issue counts -- this is metadata, not an issue type.
    /// Skipped during serialization: this is internal LSP data, not part of the JSON output schema.
    #[serde(skip)]
    pub export_usages: Vec<ExportUsage>,
}

impl AnalysisResults {
    /// Total number of issues found.
    ///
    /// Sums across all issue categories (unused files, exports, types,
    /// dependencies, members, unresolved imports, unlisted deps, duplicates,
    /// type-only deps, circular deps, and boundary violations).
    ///
    /// # Examples
    ///
    /// ```
    /// use fallow_types::results::{AnalysisResults, UnusedFile, UnresolvedImport};
    /// use std::path::PathBuf;
    ///
    /// let mut results = AnalysisResults::default();
    /// results.unused_files.push(UnusedFile { path: PathBuf::from("a.ts") });
    /// results.unresolved_imports.push(UnresolvedImport {
    ///     path: PathBuf::from("b.ts"),
    ///     specifier: "./missing".to_string(),
    ///     line: 1,
    ///     col: 0,
    ///     specifier_col: 0,
    /// });
    /// assert_eq!(results.total_issues(), 2);
    /// ```
    #[must_use]
    pub const fn total_issues(&self) -> usize {
        self.unused_files.len()
            + self.unused_exports.len()
            + self.unused_types.len()
            + self.unused_dependencies.len()
            + self.unused_dev_dependencies.len()
            + self.unused_optional_dependencies.len()
            + self.unused_enum_members.len()
            + self.unused_class_members.len()
            + self.unresolved_imports.len()
            + self.unlisted_dependencies.len()
            + self.duplicate_exports.len()
            + self.type_only_dependencies.len()
            + self.test_only_dependencies.len()
            + self.circular_dependencies.len()
            + self.boundary_violations.len()
    }

    /// Whether any issues were found.
    #[must_use]
    pub const fn has_issues(&self) -> bool {
        self.total_issues() > 0
    }

    /// Sort all result arrays for deterministic output ordering.
    ///
    /// Parallel collection (rayon, `FxHashMap` iteration) does not guarantee
    /// insertion order, so the same project can produce different orderings
    /// across runs. This method canonicalises every result list by sorting on
    /// (path, line, col, name) so that JSON/SARIF/human output is stable.
    pub fn sort(&mut self) {
        self.unused_files.sort_by(|a, b| a.path.cmp(&b.path));

        self.unused_exports.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.export_name.cmp(&b.export_name))
        });

        self.unused_types.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.export_name.cmp(&b.export_name))
        });

        self.unused_dependencies.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.package_name.cmp(&b.package_name))
        });

        self.unused_dev_dependencies.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.package_name.cmp(&b.package_name))
        });

        self.unused_optional_dependencies.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.package_name.cmp(&b.package_name))
        });

        self.unused_enum_members.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.parent_name.cmp(&b.parent_name))
                .then(a.member_name.cmp(&b.member_name))
        });

        self.unused_class_members.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.parent_name.cmp(&b.parent_name))
                .then(a.member_name.cmp(&b.member_name))
        });

        self.unresolved_imports.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.col.cmp(&b.col))
                .then(a.specifier.cmp(&b.specifier))
        });

        self.unlisted_dependencies
            .sort_by(|a, b| a.package_name.cmp(&b.package_name));
        for dep in &mut self.unlisted_dependencies {
            dep.imported_from
                .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        }

        self.duplicate_exports
            .sort_by(|a, b| a.export_name.cmp(&b.export_name));
        for dup in &mut self.duplicate_exports {
            dup.locations
                .sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        }

        self.type_only_dependencies.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.package_name.cmp(&b.package_name))
        });

        self.test_only_dependencies.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.package_name.cmp(&b.package_name))
        });

        self.circular_dependencies
            .sort_by(|a, b| a.files.cmp(&b.files).then(a.length.cmp(&b.length)));

        self.boundary_violations.sort_by(|a, b| {
            a.from_path
                .cmp(&b.from_path)
                .then(a.line.cmp(&b.line))
                .then(a.col.cmp(&b.col))
                .then(a.to_path.cmp(&b.to_path))
        });

        for usage in &mut self.export_usages {
            usage.reference_locations.sort_by(|a, b| {
                a.path
                    .cmp(&b.path)
                    .then(a.line.cmp(&b.line))
                    .then(a.col.cmp(&b.col))
            });
        }
        self.export_usages.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.line.cmp(&b.line))
                .then(a.export_name.cmp(&b.export_name))
        });
    }
}

/// A file that is not reachable from any entry point.
#[derive(Debug, Clone, Serialize)]
pub struct UnusedFile {
    /// Absolute path to the unused file.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
}

/// An export that is never imported by other modules.
#[derive(Debug, Clone, Serialize)]
pub struct UnusedExport {
    /// File containing the unused export.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// Name of the unused export.
    pub export_name: String,
    /// Whether this is a type-only export.
    pub is_type_only: bool,
    /// 1-based line number of the export.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
    /// Byte offset into the source file (used by the fix command).
    pub span_start: u32,
    /// Whether this finding comes from a barrel/index re-export rather than the source definition.
    pub is_re_export: bool,
}

/// A dependency that is listed in package.json but never imported.
#[derive(Debug, Clone, Serialize)]
pub struct UnusedDependency {
    /// npm package name.
    pub package_name: String,
    /// Whether this is in `dependencies`, `devDependencies`, or `optionalDependencies`.
    pub location: DependencyLocation,
    /// Path to the package.json where this dependency is listed.
    /// For root deps this is `<root>/package.json`, for workspace deps it is `<ws>/package.json`.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number of the dependency entry in package.json.
    pub line: u32,
}

/// Where in package.json a dependency is listed.
///
/// # Examples
///
/// ```
/// use fallow_types::results::DependencyLocation;
///
/// // All three variants are constructible
/// let loc = DependencyLocation::Dependencies;
/// let dev = DependencyLocation::DevDependencies;
/// let opt = DependencyLocation::OptionalDependencies;
/// // Debug output includes the variant name
/// assert!(format!("{loc:?}").contains("Dependencies"));
/// assert!(format!("{dev:?}").contains("DevDependencies"));
/// assert!(format!("{opt:?}").contains("OptionalDependencies"));
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyLocation {
    /// Listed in `dependencies`.
    Dependencies,
    /// Listed in `devDependencies`.
    DevDependencies,
    /// Listed in `optionalDependencies`.
    OptionalDependencies,
}

/// An unused enum or class member.
#[derive(Debug, Clone, Serialize)]
pub struct UnusedMember {
    /// File containing the unused member.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// Name of the parent enum or class.
    pub parent_name: String,
    /// Name of the unused member.
    pub member_name: String,
    /// Whether this is an enum member, class method, or class property.
    pub kind: MemberKind,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
}

/// An import that could not be resolved.
#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedImport {
    /// File containing the unresolved import.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// The import specifier that could not be resolved.
    pub specifier: String,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset of the import statement.
    pub col: u32,
    /// 0-based byte column offset of the source string literal (the specifier in quotes).
    /// Used by the LSP to underline just the specifier, not the entire import line.
    pub specifier_col: u32,
}

/// A dependency used in code but not listed in package.json.
#[derive(Debug, Clone, Serialize)]
pub struct UnlistedDependency {
    /// npm package name.
    pub package_name: String,
    /// Import sites where this unlisted dependency is used (file path, line, column).
    pub imported_from: Vec<ImportSite>,
}

/// A location where an import occurs.
#[derive(Debug, Clone, Serialize)]
pub struct ImportSite {
    /// File containing the import.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
}

/// An export that appears multiple times across the project.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateExport {
    /// The duplicated export name.
    pub export_name: String,
    /// Locations where this export name appears.
    pub locations: Vec<DuplicateLocation>,
}

/// A location where a duplicate export appears.
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateLocation {
    /// File containing the duplicate export.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
}

/// A production dependency that is only used via type-only imports.
/// In production builds, type imports are erased, so this dependency
/// is not needed at runtime and could be moved to devDependencies.
#[derive(Debug, Clone, Serialize)]
pub struct TypeOnlyDependency {
    /// npm package name.
    pub package_name: String,
    /// Path to the package.json where the dependency is listed.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number of the dependency entry in package.json.
    pub line: u32,
}

/// A production dependency that is only imported by test files.
/// Since it is never used in production code, it could be moved to devDependencies.
#[derive(Debug, Clone, Serialize)]
pub struct TestOnlyDependency {
    /// npm package name.
    pub package_name: String,
    /// Path to the package.json where the dependency is listed.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number of the dependency entry in package.json.
    pub line: u32,
}

/// A circular dependency chain detected in the module graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircularDependency {
    /// Files forming the cycle, in import order.
    #[serde(serialize_with = "serde_path::serialize_vec")]
    pub files: Vec<PathBuf>,
    /// Number of files in the cycle.
    pub length: usize,
    /// 1-based line number of the import that starts the cycle (in the first file).
    #[serde(default)]
    pub line: u32,
    /// 0-based byte column offset of the import that starts the cycle.
    #[serde(default)]
    pub col: u32,
}

/// An import that crosses an architecture boundary rule.
#[derive(Debug, Clone, Serialize)]
pub struct BoundaryViolation {
    /// The file making the disallowed import.
    #[serde(serialize_with = "serde_path::serialize")]
    pub from_path: PathBuf,
    /// The file being imported that violates the boundary.
    #[serde(serialize_with = "serde_path::serialize")]
    pub to_path: PathBuf,
    /// The zone the importing file belongs to.
    pub from_zone: String,
    /// The zone the imported file belongs to.
    pub to_zone: String,
    /// The raw import specifier from the source file.
    pub import_specifier: String,
    /// 1-based line number of the import statement in the source file.
    pub line: u32,
    /// 0-based byte column offset of the import statement.
    pub col: u32,
}

/// Usage count for an export symbol. Used by the LSP Code Lens to show
/// reference counts above each export declaration.
#[derive(Debug, Clone, Serialize)]
pub struct ExportUsage {
    /// File containing the export.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// Name of the exported symbol.
    pub export_name: String,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
    /// Number of files that reference this export.
    pub reference_count: usize,
    /// Locations where this export is referenced. Used by the LSP Code Lens
    /// to enable click-to-navigate via `editor.action.showReferences`.
    pub reference_locations: Vec<ReferenceLocation>,
}

/// A location where an export is referenced (import site in another file).
#[derive(Debug, Clone, Serialize)]
pub struct ReferenceLocation {
    /// File containing the import that references the export.
    #[serde(serialize_with = "serde_path::serialize")]
    pub path: PathBuf,
    /// 1-based line number.
    pub line: u32,
    /// 0-based byte column offset.
    pub col: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_results_no_issues() {
        let results = AnalysisResults::default();
        assert_eq!(results.total_issues(), 0);
        assert!(!results.has_issues());
    }

    #[test]
    fn results_with_unused_file() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("test.ts"),
        });
        assert_eq!(results.total_issues(), 1);
        assert!(results.has_issues());
    }

    #[test]
    fn results_with_unused_export() {
        let mut results = AnalysisResults::default();
        results.unused_exports.push(UnusedExport {
            path: PathBuf::from("test.ts"),
            export_name: "foo".to_string(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });
        assert_eq!(results.total_issues(), 1);
        assert!(results.has_issues());
    }

    #[test]
    fn results_total_counts_all_types() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        results.unused_exports.push(UnusedExport {
            path: PathBuf::from("b.ts"),
            export_name: "x".to_string(),
            is_type_only: false,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });
        results.unused_types.push(UnusedExport {
            path: PathBuf::from("c.ts"),
            export_name: "T".to_string(),
            is_type_only: true,
            line: 1,
            col: 0,
            span_start: 0,
            is_re_export: false,
        });
        results.unused_dependencies.push(UnusedDependency {
            package_name: "dep".to_string(),
            location: DependencyLocation::Dependencies,
            path: PathBuf::from("package.json"),
            line: 5,
        });
        results.unused_dev_dependencies.push(UnusedDependency {
            package_name: "dev".to_string(),
            location: DependencyLocation::DevDependencies,
            path: PathBuf::from("package.json"),
            line: 5,
        });
        results.unused_enum_members.push(UnusedMember {
            path: PathBuf::from("d.ts"),
            parent_name: "E".to_string(),
            member_name: "A".to_string(),
            kind: MemberKind::EnumMember,
            line: 1,
            col: 0,
        });
        results.unused_class_members.push(UnusedMember {
            path: PathBuf::from("e.ts"),
            parent_name: "C".to_string(),
            member_name: "m".to_string(),
            kind: MemberKind::ClassMethod,
            line: 1,
            col: 0,
        });
        results.unresolved_imports.push(UnresolvedImport {
            path: PathBuf::from("f.ts"),
            specifier: "./missing".to_string(),
            line: 1,
            col: 0,
            specifier_col: 0,
        });
        results.unlisted_dependencies.push(UnlistedDependency {
            package_name: "unlisted".to_string(),
            imported_from: vec![ImportSite {
                path: PathBuf::from("g.ts"),
                line: 1,
                col: 0,
            }],
        });
        results.duplicate_exports.push(DuplicateExport {
            export_name: "dup".to_string(),
            locations: vec![
                DuplicateLocation {
                    path: PathBuf::from("h.ts"),
                    line: 15,
                    col: 0,
                },
                DuplicateLocation {
                    path: PathBuf::from("i.ts"),
                    line: 30,
                    col: 0,
                },
            ],
        });
        results.unused_optional_dependencies.push(UnusedDependency {
            package_name: "optional".to_string(),
            location: DependencyLocation::OptionalDependencies,
            path: PathBuf::from("package.json"),
            line: 5,
        });
        results.type_only_dependencies.push(TypeOnlyDependency {
            package_name: "type-only".to_string(),
            path: PathBuf::from("package.json"),
            line: 8,
        });
        results.test_only_dependencies.push(TestOnlyDependency {
            package_name: "test-only".to_string(),
            path: PathBuf::from("package.json"),
            line: 9,
        });
        results.circular_dependencies.push(CircularDependency {
            files: vec![PathBuf::from("a.ts"), PathBuf::from("b.ts")],
            length: 2,
            line: 3,
            col: 0,
        });
        results.boundary_violations.push(BoundaryViolation {
            from_path: PathBuf::from("src/ui/Button.tsx"),
            to_path: PathBuf::from("src/db/queries.ts"),
            from_zone: "ui".to_string(),
            to_zone: "database".to_string(),
            import_specifier: "../db/queries".to_string(),
            line: 3,
            col: 0,
        });

        // 15 categories, one of each
        assert_eq!(results.total_issues(), 15);
        assert!(results.has_issues());
    }

    // ── total_issues / has_issues consistency ──────────────────

    #[test]
    fn total_issues_and_has_issues_are_consistent() {
        let results = AnalysisResults::default();
        assert_eq!(results.total_issues(), 0);
        assert!(!results.has_issues());
        assert_eq!(results.total_issues() > 0, results.has_issues());
    }

    // ── total_issues counts each category independently ─────────

    #[test]
    fn total_issues_sums_all_categories_independently() {
        let mut results = AnalysisResults::default();
        results.unused_files.push(UnusedFile {
            path: PathBuf::from("a.ts"),
        });
        assert_eq!(results.total_issues(), 1);

        results.unused_files.push(UnusedFile {
            path: PathBuf::from("b.ts"),
        });
        assert_eq!(results.total_issues(), 2);

        results.unresolved_imports.push(UnresolvedImport {
            path: PathBuf::from("c.ts"),
            specifier: "./missing".to_string(),
            line: 1,
            col: 0,
            specifier_col: 0,
        });
        assert_eq!(results.total_issues(), 3);
    }

    // ── default is truly empty ──────────────────────────────────

    #[test]
    fn default_results_all_fields_empty() {
        let r = AnalysisResults::default();
        assert!(r.unused_files.is_empty());
        assert!(r.unused_exports.is_empty());
        assert!(r.unused_types.is_empty());
        assert!(r.unused_dependencies.is_empty());
        assert!(r.unused_dev_dependencies.is_empty());
        assert!(r.unused_optional_dependencies.is_empty());
        assert!(r.unused_enum_members.is_empty());
        assert!(r.unused_class_members.is_empty());
        assert!(r.unresolved_imports.is_empty());
        assert!(r.unlisted_dependencies.is_empty());
        assert!(r.duplicate_exports.is_empty());
        assert!(r.type_only_dependencies.is_empty());
        assert!(r.test_only_dependencies.is_empty());
        assert!(r.circular_dependencies.is_empty());
        assert!(r.boundary_violations.is_empty());
        assert!(r.export_usages.is_empty());
    }
}
