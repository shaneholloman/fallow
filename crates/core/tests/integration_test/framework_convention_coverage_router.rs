use super::common::{create_config, fixture_path};
use super::framework_convention_coverage_common::{
    collect_unused_exports, collect_unused_files, has_unused_export,
};

#[test]
fn react_router_route_config_root_and_route_exports_are_covered() {
    let root = fixture_path("react-router-conventions");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_files = collect_unused_files(&root, &results);
    assert!(
        !unused_files.iter().any(|path| path == "app/routes.ts"),
        "app/routes.ts should be treated as framework-used, unused files: {unused_files:?}"
    );

    let unused_exports = collect_unused_exports(&root, &results);
    for (path, export) in [
        ("app/routes.ts", "default"),
        ("app/root.tsx", "Layout"),
        ("app/root.tsx", "clientLoader"),
        ("app/root.tsx", "clientAction"),
        ("app/root.tsx", "HydrateFallback"),
        ("app/routes/_index.tsx", "middleware"),
        ("app/routes/_index.tsx", "clientMiddleware"),
        ("app/routes/_index.tsx", "shouldRevalidate"),
    ] {
        assert!(
            !has_unused_export(&unused_exports, path, export),
            "{path}:{export} should be treated as framework-used, found: {unused_exports:?}"
        );
    }

    for (path, export) in [
        ("app/routes.ts", "unusedRouteConfigHelper"),
        ("app/root.tsx", "unusedRootHelper"),
        ("app/routes/_index.tsx", "unusedRouteHelper"),
    ] {
        assert!(
            has_unused_export(&unused_exports, path, export),
            "{path}:{export} should still be reported as unused, found: {unused_exports:?}"
        );
    }
}

#[test]
fn remix_root_and_client_data_exports_are_covered() {
    let root = fixture_path("remix-conventions");
    let config = create_config(root.clone());
    let results = fallow_core::analyze(&config).expect("analysis should succeed");

    let unused_exports = collect_unused_exports(&root, &results);
    for (path, export) in [
        ("app/root.tsx", "Layout"),
        ("app/root.tsx", "clientLoader"),
        ("app/root.tsx", "clientAction"),
        ("app/root.tsx", "shouldRevalidate"),
        ("app/root.tsx", "HydrateFallback"),
        ("app/routes/_index.tsx", "clientLoader"),
        ("app/routes/_index.tsx", "clientAction"),
        ("app/routes/_index.tsx", "shouldRevalidate"),
        ("app/routes/_index.tsx", "HydrateFallback"),
    ] {
        assert!(
            !has_unused_export(&unused_exports, path, export),
            "{path}:{export} should be treated as framework-used, found: {unused_exports:?}"
        );
    }

    for (path, export) in [
        ("app/root.tsx", "unusedRootHelper"),
        ("app/routes/_index.tsx", "unusedRouteHelper"),
    ] {
        assert!(
            has_unused_export(&unused_exports, path, export),
            "{path}:{export} should still be reported as unused, found: {unused_exports:?}"
        );
    }
}
