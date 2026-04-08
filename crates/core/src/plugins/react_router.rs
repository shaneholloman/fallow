//! React Router (v7+) framework plugin.
//!
//! Detects React Router projects and marks route files, root layout, and entry points.
//! Recognizes conventional route exports (loader, action, meta, etc.).

use super::Plugin;

const ENABLERS: &[&str] = &["@react-router/dev"];

const ENTRY_PATTERNS: &[&str] = &[
    "app/routes/**/*.{ts,tsx,js,jsx}",
    "app/root.{ts,tsx,js,jsx}",
    "app/entry.client.{ts,tsx,js,jsx}",
    "app/entry.server.{ts,tsx,js,jsx}",
];

const ALWAYS_USED: &[&str] = &["react-router.config.{ts,js}", "app/routes.{ts,js,mts,mjs}"];

const TOOLING_DEPENDENCIES: &[&str] = &[
    "@react-router/dev",
    "@react-router/serve",
    "@react-router/node",
];

macro_rules! route_module_exports {
    ($($export:literal),+ $(,)?) => {
        const ROUTE_EXPORTS: &[&str] = &[$($export),+];
        const ROOT_EXPORTS: &[&str] = &[$($export,)+ "Layout"];
    };
}

route_module_exports!(
    "default",
    "loader",
    "clientLoader",
    "action",
    "clientAction",
    "meta",
    "links",
    "headers",
    "handle",
    "ErrorBoundary",
    "HydrateFallback",
    "shouldRevalidate",
    "middleware",
    "clientMiddleware",
);

const ROUTE_CONFIG_EXPORTS: &[&str] = &["default"];

define_plugin! {
    struct ReactRouterPlugin => "react-router",
    enablers: ENABLERS,
    entry_patterns: ENTRY_PATTERNS,
    always_used: ALWAYS_USED,
    tooling_dependencies: TOOLING_DEPENDENCIES,
    used_exports: [
        ("app/routes/**/*.{ts,tsx,js,jsx}", ROUTE_EXPORTS),
        ("app/root.{ts,tsx,js,jsx}", ROOT_EXPORTS),
        ("app/routes.{ts,js,mts,mjs}", ROUTE_CONFIG_EXPORTS),
    ],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn used_exports_cover_root_and_route_config() {
        let plugin = ReactRouterPlugin;
        let exports = plugin.used_exports();

        assert!(exports.iter().any(|(pattern, names)| {
            pattern == &"app/root.{ts,tsx,js,jsx}" && names.contains(&"Layout")
        }));
        assert!(exports.iter().any(|(pattern, names)| {
            pattern == &"app/routes/**/*.{ts,tsx,js,jsx}"
                && names.contains(&"clientMiddleware")
                && names.contains(&"middleware")
        }));
        assert!(exports.iter().any(|(pattern, names)| {
            pattern == &"app/routes.{ts,js,mts,mjs}" && names == &["default"]
        }));
    }
}
