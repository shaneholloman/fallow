use super::common::{create_config, fixture_path};

// ── HTML entry file parsing ──────────────────────────────────

#[test]
fn html_entry_makes_referenced_script_reachable() {
    let root = fixture_path("html-entry");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // entry.ts is referenced by index.html <script src>, so it should NOT be unused
    assert!(
        !unused_file_names.contains(&"entry.ts".to_string()),
        "entry.ts should be reachable via HTML <script src>, unused files: {unused_file_names:?}"
    );

    // helper.ts is imported by entry.ts, so it should NOT be unused
    assert!(
        !unused_file_names.contains(&"helper.ts".to_string()),
        "helper.ts should be transitively reachable via HTML entry, unused files: {unused_file_names:?}"
    );
}

#[test]
fn html_entry_makes_referenced_stylesheet_reachable() {
    let root = fixture_path("html-entry");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // global.css is referenced by index.html <link rel="stylesheet">, so it should NOT be unused
    assert!(
        !unused_file_names.contains(&"global.css".to_string()),
        "global.css should be reachable via HTML <link href>, unused files: {unused_file_names:?}"
    );
}

#[test]
fn html_entry_does_not_suppress_unused_exports() {
    let root = fixture_path("html-entry");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // The `unused` export in helper.ts should still be detected as unused
    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();
    assert!(
        unused_export_names.contains(&"unused"),
        "unused export should still be detected, got: {unused_export_names:?}"
    );
}

#[test]
fn html_files_not_reported_as_unused() {
    let root = fixture_path("html-entry");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .map(|f| f.path.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // HTML files should never appear in unused-file output
    assert!(
        !unused_file_names.iter().any(|f| std::path::Path::new(f)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))),
        "HTML files should be excluded from unused-file detection, got: {unused_file_names:?}"
    );
}

#[test]
fn html_entry_no_unresolved_imports() {
    let root = fixture_path("html-entry");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // All HTML asset references should resolve successfully
    let html_unresolved: Vec<&str> = results
        .unresolved_imports
        .iter()
        .filter(|u| u.path.to_string_lossy().ends_with(".html"))
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        html_unresolved.is_empty(),
        "HTML asset references should resolve, got unresolved: {html_unresolved:?}"
    );
}
