//! Post-resolution specifier upgrade pass.
//!
//! Fixes non-deterministic resolution of bare specifiers that arises from per-file
//! tsconfig path alias discovery (`TsconfigDiscovery::Auto`). The same specifier
//! (e.g., `preact/hooks`) may resolve to `InternalModule` in files under a tsconfig
//! with matching path aliases, but to `NpmPackage` in files without such aliases.
//!
//! This pass scans all resolved imports and re-exports to find bare specifiers where
//! at least one file resolved to `InternalModule`, then upgrades all `NpmPackage`
//! results for that specifier to `InternalModule`. This is correct because if any
//! tsconfig maps a specifier to a project source file, that file is the canonical
//! origin.
//!
//! Run once after all parallel resolution completes, as the final step in
//! [`super::resolve_all_imports`].

use rustc_hash::FxHashMap;

use fallow_types::discover::FileId;

use super::ResolvedModule;
use super::path_info::is_bare_specifier;
use super::types::ResolveResult;

/// Post-resolution pass: deterministic specifier upgrade.
///
/// With `TsconfigDiscovery::Auto`, the same bare specifier (e.g., `preact/hooks`)
/// may resolve to `InternalModule` from files under a tsconfig with path aliases
/// but `NpmPackage` from files without such aliases. The parallel resolution cache
/// makes the per-file result depend on which thread resolved first (non-deterministic).
///
/// Scans all resolved imports/re-exports to find bare specifiers where ANY file resolved
/// to `InternalModule`. For those specifiers, upgrades all `NpmPackage` results to
/// `InternalModule`. This is correct because if any tsconfig context maps a specifier to
/// a project source file, that source file IS the origin of the package.
///
/// Note: if two tsconfigs map the same specifier to different `FileId`s, the first one
/// encountered (by module order = `FileId` order) wins. This is deterministic but may be
/// imprecise for that edge case — both files get connected regardless.
pub(super) fn apply_specifier_upgrades(resolved: &mut [ResolvedModule]) {
    let mut specifier_upgrades: FxHashMap<String, FileId> = FxHashMap::default();
    for module in resolved.iter() {
        for imp in module
            .resolved_imports
            .iter()
            .chain(module.resolved_dynamic_imports.iter())
        {
            if is_bare_specifier(&imp.info.source)
                && let ResolveResult::InternalModule(file_id) = &imp.target
            {
                specifier_upgrades
                    .entry(imp.info.source.clone())
                    .or_insert(*file_id);
            }
        }
        for re in &module.re_exports {
            if is_bare_specifier(&re.info.source)
                && let ResolveResult::InternalModule(file_id) = &re.target
            {
                specifier_upgrades
                    .entry(re.info.source.clone())
                    .or_insert(*file_id);
            }
        }
    }

    if specifier_upgrades.is_empty() {
        return;
    }

    // Apply upgrades: replace NpmPackage with InternalModule for matched specifiers
    for module in resolved.iter_mut() {
        for imp in module
            .resolved_imports
            .iter_mut()
            .chain(module.resolved_dynamic_imports.iter_mut())
        {
            if matches!(imp.target, ResolveResult::NpmPackage(_))
                && let Some(&file_id) = specifier_upgrades.get(&imp.info.source)
            {
                imp.target = ResolveResult::InternalModule(file_id);
            }
        }
        for re in &mut module.re_exports {
            if matches!(re.target, ResolveResult::NpmPackage(_))
                && let Some(&file_id) = specifier_upgrades.get(&re.info.source)
            {
                re.target = ResolveResult::InternalModule(file_id);
            }
        }
    }
}
