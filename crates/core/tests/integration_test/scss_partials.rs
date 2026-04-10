use super::common::{create_config, fixture_path};

#[test]
fn scss_partial_files_resolved_via_underscore_convention() {
    let root = fixture_path("scss-partial-project");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    // _variables.scss and _mixins.scss should NOT be reported as unused files
    let unused_file_names: Vec<String> = results
        .unused_files
        .iter()
        .filter_map(|f| f.path.file_name())
        .filter_map(|n| n.to_str())
        .map(ToString::to_string)
        .collect();
    assert!(
        !unused_file_names.contains(&"_variables.scss".to_string()),
        "_variables.scss should be used via @use: {unused_file_names:?}"
    );
    assert!(
        !unused_file_names.contains(&"_mixins.scss".to_string()),
        "_mixins.scss should be used via @use: {unused_file_names:?}"
    );

    // No unresolved imports for SCSS partial references
    let unresolved_specs: Vec<&str> = results
        .unresolved_imports
        .iter()
        .map(|u| u.specifier.as_str())
        .collect();
    assert!(
        !unresolved_specs.iter().any(|s| s.contains("variables")),
        "variables should be resolved: {unresolved_specs:?}"
    );
    assert!(
        !unresolved_specs.iter().any(|s| s.contains("mixins")),
        "mixins should be resolved: {unresolved_specs:?}"
    );

    // No unlisted dependencies for SCSS partials
    let unlisted: Vec<&str> = results
        .unlisted_dependencies
        .iter()
        .map(|u| u.package_name.as_str())
        .collect();
    assert!(
        !unlisted.contains(&"variables"),
        "'variables' should not be an unlisted dep: {unlisted:?}"
    );

    // Directory index: _index.scss should be resolved via @use 'components'
    assert!(
        !unused_file_names.contains(&"_index.scss".to_string()),
        "_index.scss should be used via @use 'components': {unused_file_names:?}"
    );
    assert!(
        !unresolved_specs.iter().any(|s| s.contains("components")),
        "components should be resolved via _index.scss: {unresolved_specs:?}"
    );
}
