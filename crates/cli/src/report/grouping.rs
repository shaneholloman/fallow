//! Grouping infrastructure for `--group-by owner|directory`.
//!
//! Partitions `AnalysisResults` into labeled groups by ownership (CODEOWNERS)
//! or by first directory component.

use std::path::Path;

use fallow_core::results::AnalysisResults;
use rustc_hash::FxHashMap;

use super::relative_path;
use crate::codeowners::{self, CodeOwners, UNOWNED_LABEL};

/// Ownership resolver for `--group-by`.
///
/// Owns the `CodeOwners` data when grouping by owner, avoiding lifetime
/// complexity in the report context.
pub enum OwnershipResolver {
    /// Group by CODEOWNERS file (first owner, last matching rule).
    Owner(CodeOwners),
    /// Group by first directory component.
    Directory,
}

impl OwnershipResolver {
    /// Resolve the group key for a file path (relative to project root).
    pub fn resolve(&self, rel_path: &Path) -> String {
        match self {
            Self::Owner(co) => co.owner_of(rel_path).unwrap_or(UNOWNED_LABEL).to_string(),
            Self::Directory => codeowners::directory_group(rel_path).to_string(),
        }
    }

    /// Label for the grouping mode (used in JSON `grouped_by` field).
    pub fn mode_label(&self) -> &'static str {
        match self {
            Self::Owner(_) => "owner",
            Self::Directory => "directory",
        }
    }
}

/// A single group: a label and its subset of results.
pub struct ResultGroup {
    /// Group label (owner name or directory).
    pub key: String,
    /// Issues belonging to this group.
    pub results: AnalysisResults,
}

/// Partition analysis results into groups by ownership or directory.
///
/// Each issue is assigned to a group by extracting its primary file path
/// and resolving the group key via the `OwnershipResolver`.
/// Returns groups sorted alphabetically by key, with `(unowned)` last.
pub fn group_analysis_results(
    results: &AnalysisResults,
    root: &Path,
    resolver: &OwnershipResolver,
) -> Vec<ResultGroup> {
    let mut groups: FxHashMap<String, AnalysisResults> = FxHashMap::default();

    let key_for = |path: &Path| -> String { resolver.resolve(relative_path(path, root)) };

    // ── File-scoped issue types ─────────────────────────────────
    for item in &results.unused_files {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_files
            .push(item.clone());
    }
    for item in &results.unused_exports {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_exports
            .push(item.clone());
    }
    for item in &results.unused_types {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_types
            .push(item.clone());
    }
    for item in &results.unused_enum_members {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_enum_members
            .push(item.clone());
    }
    for item in &results.unused_class_members {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_class_members
            .push(item.clone());
    }
    for item in &results.unresolved_imports {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unresolved_imports
            .push(item.clone());
    }

    // ── Dependency-scoped (use package.json path) ───────────────
    for item in &results.unused_dependencies {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_dependencies
            .push(item.clone());
    }
    for item in &results.unused_dev_dependencies {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_dev_dependencies
            .push(item.clone());
    }
    for item in &results.unused_optional_dependencies {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .unused_optional_dependencies
            .push(item.clone());
    }
    for item in &results.type_only_dependencies {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .type_only_dependencies
            .push(item.clone());
    }
    for item in &results.test_only_dependencies {
        groups
            .entry(key_for(&item.path))
            .or_default()
            .test_only_dependencies
            .push(item.clone());
    }

    // ── Multi-location types (use first location) ───────────────
    for item in &results.unlisted_dependencies {
        let key = item
            .imported_from
            .first()
            .map_or_else(|| UNOWNED_LABEL.to_string(), |site| key_for(&site.path));
        groups
            .entry(key)
            .or_default()
            .unlisted_dependencies
            .push(item.clone());
    }
    for item in &results.duplicate_exports {
        let key = item
            .locations
            .first()
            .map_or_else(|| UNOWNED_LABEL.to_string(), |loc| key_for(&loc.path));
        groups
            .entry(key)
            .or_default()
            .duplicate_exports
            .push(item.clone());
    }
    for item in &results.circular_dependencies {
        let key = item
            .files
            .first()
            .map_or_else(|| UNOWNED_LABEL.to_string(), |f| key_for(f));
        groups
            .entry(key)
            .or_default()
            .circular_dependencies
            .push(item.clone());
    }
    for item in &results.boundary_violations {
        groups
            .entry(key_for(&item.from_path))
            .or_default()
            .boundary_violations
            .push(item.clone());
    }

    // ── Sort: alphabetical, (unowned) last ──────────────────────
    let mut sorted: Vec<_> = groups
        .into_iter()
        .map(|(key, results)| ResultGroup { key, results })
        .collect();
    sorted.sort_by(|a, b| {
        let a_unowned = a.key == UNOWNED_LABEL;
        let b_unowned = b.key == UNOWNED_LABEL;
        match (a_unowned, b_unowned) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.key.cmp(&b.key),
        }
    });
    sorted
}

/// Resolve the group key for a single path (for per-result tagging in SARIF/CodeClimate).
pub fn resolve_owner(path: &Path, root: &Path, resolver: &OwnershipResolver) -> String {
    resolver.resolve(relative_path(path, root))
}
