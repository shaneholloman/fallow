use super::common::{create_config, fixture_path};

// ── Type-only circular dependency filtering ──────────────────

#[test]
fn type_only_bidirectional_import_not_reported_as_cycle() {
    let root = fixture_path("type-only-cycle");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // user.ts and post.ts have `import type` from each other.
    // This is NOT a runtime cycle and should not be reported.
    assert!(
        results.circular_dependencies.is_empty(),
        "type-only bidirectional imports should not be reported as circular dependencies, got: {:?}",
        results
            .circular_dependencies
            .iter()
            .map(|cd| &cd.files)
            .collect::<Vec<_>>()
    );
}

#[test]
fn type_only_cycle_still_detects_unused_exports() {
    let root = fixture_path("type-only-cycle");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // The value exports (createUser, createPost) are used by index.ts.
    // No files should be reported as unused.
    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(
        !unused_file_names.contains(&"user.ts".to_string()),
        "user.ts should not be unused, got: {unused_file_names:?}"
    );
    assert!(
        !unused_file_names.contains(&"post.ts".to_string()),
        "post.ts should not be unused, got: {unused_file_names:?}"
    );
}

// ── Duplicate export common-importer filtering ───────────────

#[test]
fn unrelated_route_files_not_flagged_as_duplicate_exports() {
    let root = fixture_path("route-duplicate-exports");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // foo/page.ts and bar/page.ts both export `Area` and `handler`.
    // Each page is imported by its own router (foo/router.ts, bar/router.ts),
    // not by a shared file. No common importer exists for the page files.
    // Neither `Area` nor `handler` should be flagged as duplicates.
    let route_dupes: Vec<&str> = results
        .duplicate_exports
        .iter()
        .filter(|d| d.export_name == "Area" || d.export_name == "handler")
        .map(|d| d.export_name.as_str())
        .collect();
    assert!(
        route_dupes.is_empty(),
        "route files with separate importers should not be flagged as duplicates, got: {route_dupes:?}"
    );
}

#[test]
fn shared_util_duplicates_with_common_importer_still_flagged() {
    let root = fixture_path("route-duplicate-exports");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // shared/utils.ts and shared/helpers.ts both export `formatDate`.
    // Both are imported by index.ts (shared importer) -- should be flagged.
    let format_date_dupe = results
        .duplicate_exports
        .iter()
        .find(|d| d.export_name == "formatDate");
    assert!(
        format_date_dupe.is_some(),
        "formatDate in shared files with common importer should be flagged, got dupes: {:?}",
        results
            .duplicate_exports
            .iter()
            .map(|d| &d.export_name)
            .collect::<Vec<_>>()
    );
}

// ── Broken tsconfig extends chain (issue #97) ────────────────

#[test]
fn broken_tsconfig_extends_does_not_poison_sibling_resolution() {
    // Solution-style `packages/my-app/tsconfig.json` references
    // `tsconfig.app.json` (valid) and `tsconfig.spec.json` (extends a
    // non-existent `../../tsconfig.json`). Before the fix, the broken
    // sibling's extends chain failed `oxc_resolver::resolve_file` for ALL
    // files in the workspace, including `main.ts` which is only covered by
    // the valid `tsconfig.app.json`. Every relative import was reported as
    // unresolved.
    //
    // The fallback in `resolve_file_with_tsconfig_fallback` retries via
    // `resolver.resolve(dir, specifier)`, bypassing tsconfig discovery.
    let root = fixture_path("tsconfig-broken-extends");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    assert!(
        results.unresolved_imports.is_empty(),
        "broken sibling tsconfig should not poison resolution for files covered \
         by a valid sibling; got unresolved imports: {:?}",
        results
            .unresolved_imports
            .iter()
            .map(|u| (u.path.display().to_string(), &u.specifier))
            .collect::<Vec<_>>()
    );
}

// ── Interface-mediated class member usage (issue #132) ──────

#[test]
fn interface_member_usage_does_not_flag_implementer_members() {
    let root = fixture_path("interface-member-usage");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_members: Vec<String> = results
        .unused_class_members
        .iter()
        .map(|member| format!("{}.{}", member.parent_name, member.member_name))
        .collect();

    assert!(
        !unused_members.contains(&"FixedSizeScrollStrategy.attached".to_string()),
        "attached should be credited through interface-typed access: {unused_members:?}"
    );
    assert!(
        !unused_members.contains(&"FixedSizeScrollStrategy.attach".to_string()),
        "attach should be credited through interface-typed access: {unused_members:?}"
    );
    assert!(
        !unused_members.contains(&"FixedSizeScrollStrategy.detach".to_string()),
        "detach should be credited through interface-typed access: {unused_members:?}"
    );
    assert!(
        unused_members.contains(&"FixedSizeScrollStrategy.unusedHelper".to_string()),
        "unrelated members should still be reported: {unused_members:?}"
    );
}
