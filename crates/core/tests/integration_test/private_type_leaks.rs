use super::common::{create_config, fixture_path};

#[test]
fn exported_signatures_report_same_file_private_types() {
    let root = fixture_path("private-type-leaks");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let leaks: Vec<(&str, &str)> = results
        .private_type_leaks
        .iter()
        .map(|leak| (leak.export_name.as_str(), leak.type_name.as_str()))
        .collect();

    assert!(
        leaks.contains(&("Component", "Props")),
        "Component should report Props as a private type leak, found: {leaks:?}"
    );
    assert!(
        leaks.contains(&("Service", "Options")),
        "Service should report Options as a private type leak, found: {leaks:?}"
    );
    assert!(
        !leaks.contains(&("Service", "InternalState")),
        "ECMAScript private fields should not be treated as public signature leaks: {leaks:?}"
    );
    assert!(
        !leaks.contains(&("UsesExportedType", "PublicBacking")),
        "exported backing types should not be reported as private leaks: {leaks:?}"
    );
}

#[test]
fn exported_signature_backing_types_are_not_unused_type_exports() {
    let root = fixture_path("private-type-leaks");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_types: Vec<&str> = results
        .unused_types
        .iter()
        .map(|export| export.export_name.as_str())
        .collect();

    assert!(
        !unused_types.contains(&"PublicBacking"),
        "PublicBacking backs public signatures and should not become an unused type export: {unused_types:?}"
    );
}

#[test]
fn storybook_story_files_are_skipped() {
    let root = fixture_path("private-type-leaks");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // The fixture's Component.stories.ts uses the canonical
    // `type Story = StoryObj<...>; export const Default: Story = ...`
    // pattern. Without the storybook-suffix skip, every story export would
    // be reported as a private-type-leak. Reverting `is_storybook_file`
    // makes this assertion fail.
    let storybook_leaks: Vec<&str> = results
        .private_type_leaks
        .iter()
        .filter(|leak| leak.path.ends_with("Component.stories.ts"))
        .map(|leak| leak.export_name.as_str())
        .collect();

    assert!(
        storybook_leaks.is_empty(),
        "storybook story files should be skipped, but found leaks for: {storybook_leaks:?}"
    );
}
