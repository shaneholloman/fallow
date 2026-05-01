use super::common::{create_config, fixture_path};

#[test]
fn entry_exports_skipped_by_default() {
    let root = fixture_path("entry-export-validation");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    // With default config, entry point exports are skipped
    assert!(
        !unused_export_names.contains(&"meatdata"),
        "meatdata should not be flagged (entry exports skipped by default), found: {unused_export_names:?}"
    );
    assert!(
        !unused_export_names.contains(&"config"),
        "config should not be flagged (entry exports skipped by default), found: {unused_export_names:?}"
    );
}

#[test]
fn entry_exports_detected_when_include_entry_exports_enabled() {
    let root = fixture_path("entry-export-validation");
    let mut config = create_config(root);
    config.include_entry_exports = true;
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    // With include_entry_exports, unreferenced entry exports should be flagged
    assert!(
        unused_export_names.contains(&"meatdata"),
        "meatdata should be flagged with include_entry_exports, found: {unused_export_names:?}"
    );
    assert!(
        unused_export_names.contains(&"config"),
        "config should be flagged with include_entry_exports, found: {unused_export_names:?}"
    );

    // helper is imported by consumer.ts, so it should NOT be flagged
    assert!(
        !unused_export_names.contains(&"helper"),
        "helper should not be flagged (imported by consumer.ts), found: {unused_export_names:?}"
    );
}

#[test]
fn entry_exports_detected_via_config_file_include_entry_exports() {
    // Issue #249: fixture has `.fallowrc.json` with `includeEntryExports: true`.
    // Same expectations as the CLI-flag path: meatdata + config flagged, helper not.
    let root = fixture_path("entry-export-validation-config");
    let (loaded, _path) = fallow_config::FallowConfig::find_and_load(&root)
        .expect("config load")
        .expect("fixture has .fallowrc.json");
    assert!(loaded.include_entry_exports);
    let config = loaded.resolve(root, fallow_config::OutputFormat::Human, 1, true, true);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_export_names: Vec<&str> = results
        .unused_exports
        .iter()
        .map(|e| e.export_name.as_str())
        .collect();

    assert!(
        unused_export_names.contains(&"meatdata"),
        "meatdata should be flagged via config file, found: {unused_export_names:?}"
    );
    assert!(
        unused_export_names.contains(&"config"),
        "config should be flagged via config file, found: {unused_export_names:?}"
    );
    assert!(
        !unused_export_names.contains(&"helper"),
        "helper should not be flagged (imported by consumer.ts), found: {unused_export_names:?}"
    );
}
