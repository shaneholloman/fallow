use super::common::{create_config, fixture_path};

// Regression test for issue #174: Angular external templates (`templateUrl`)
// referencing inherited members (via `extends BaseClass`) or DI-injected service
// members (`{{ service.method() }}`) must be credited as used and not reported
// as unused class members. See
// https://github.com/fallow-rs/fallow/issues/174.
#[test]
fn angular_external_template_credits_inherited_and_di_injected_members() {
    let root = fixture_path("angular-template-inherited-members");
    let config = create_config(root);
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused: Vec<(&str, &str)> = results
        .unused_class_members
        .iter()
        .map(|m| (m.parent_name.as_str(), m.member_name.as_str()))
        .collect();

    // Pattern 1: inherited members referenced in child's external template
    // must not be flagged as unused on the base class.
    assert!(
        !unused.contains(&("BaseFieldHandlerDirective", "trimValue")),
        "BaseFieldHandlerDirective.trimValue is used in child's external template via (blur)=\"trimValue()\", found: {unused:?}"
    );
    assert!(
        !unused.contains(&("BaseFieldHandlerDirective", "tooltipClass")),
        "BaseFieldHandlerDirective.tooltipClass is used in child's external template via [class]=\"tooltipClass\", found: {unused:?}"
    );

    // Pattern 2: DI-injected service members accessed via `service.method()`
    // in an external template must be credited through the component's
    // constructor parameter type annotation.
    assert!(
        !unused.contains(&("DataService", "getTotal")),
        "DataService.getTotal is used in external template via {{{{ dataService.getTotal() }}}}, found: {unused:?}"
    );
    assert!(
        !unused.contains(&("DataService", "isEmpty")),
        "DataService.isEmpty is used in external template via @if (!dataService.isEmpty()), found: {unused:?}"
    );

    // Whole-object use of `dataService.items` credits `items` as accessed.
    assert!(
        !unused.contains(&("DataService", "items")),
        "DataService.items is used in external template via @for (item of dataService.items), found: {unused:?}"
    );

    // Control cases: genuinely unused members should still be reported.
    assert!(
        unused.contains(&("BaseFieldHandlerDirective", "unusedBaseMethod")),
        "BaseFieldHandlerDirective.unusedBaseMethod is never used and should be flagged, found: {unused:?}"
    );
    assert!(
        unused.contains(&("DataService", "unusedServiceMethod")),
        "DataService.unusedServiceMethod is never used and should be flagged, found: {unused:?}"
    );
}
