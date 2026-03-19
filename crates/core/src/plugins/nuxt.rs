//! Nuxt framework plugin.
//!
//! Detects Nuxt projects and marks pages, layouts, middleware, server API,
//! plugins, composables, and utils as entry points. Recognizes conventional
//! server API and middleware exports. Parses nuxt.config.ts to extract modules,
//! CSS files, plugins, and other configuration.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

const ENABLERS: &[&str] = &["nuxt"];

const ENTRY_PATTERNS: &[&str] = &[
    // Standard Nuxt directories
    "pages/**/*.{vue,ts,tsx,js,jsx}",
    "layouts/**/*.{vue,ts,tsx,js,jsx}",
    "middleware/**/*.{ts,js}",
    "server/api/**/*.{ts,js}",
    "server/routes/**/*.{ts,js}",
    "server/middleware/**/*.{ts,js}",
    "server/utils/**/*.{ts,js}",
    "plugins/**/*.{ts,js}",
    "composables/**/*.{ts,js}",
    "utils/**/*.{ts,js}",
    "components/**/*.{vue,ts,tsx,js,jsx}",
    // Nuxt auto-scans modules/ for custom modules
    "modules/**/*.{ts,js}",
    // Nuxt 3 app/ directory structure
    "app/pages/**/*.{vue,ts,tsx,js,jsx}",
    "app/layouts/**/*.{vue,ts,tsx,js,jsx}",
    "app/middleware/**/*.{ts,js}",
    "app/plugins/**/*.{ts,js}",
    "app/composables/**/*.{ts,js}",
    "app/utils/**/*.{ts,js}",
    "app/components/**/*.{vue,ts,tsx,js,jsx}",
    "app/modules/**/*.{ts,js}",
];

const CONFIG_PATTERNS: &[&str] = &["nuxt.config.{ts,js}"];

const ALWAYS_USED: &[&str] = &[
    "nuxt.config.{ts,js}",
    "app.vue",
    "app.config.{ts,js}",
    "error.vue",
    // Nuxt 3 app/ directory structure
    "app/app.vue",
    "app/error.vue",
];

/// Implicit dependencies that Nuxt provides — these should not be flagged as unlisted.
const TOOLING_DEPENDENCIES: &[&str] = &[
    "nuxt",
    "@nuxt/devtools",
    "@nuxt/test-utils",
    "@nuxt/schema",
    "@nuxt/kit",
    // Implicit Nuxt runtime dependencies (re-exported by Nuxt at build time)
    "vue",
    "vue-router",
    "ofetch",
    "h3",
    "@unhead/vue",
    "@unhead/schema",
    "nitropack",
    "defu",
    "hookable",
    "ufo",
    "unctx",
    "unenv",
    "ohash",
    "pathe",
    "scule",
    "unimport",
    "unstorage",
    "radix3",
    "cookie-es",
    "crossws",
    "consola",
];

const USED_EXPORTS_SERVER_API: &[&str] = &["default", "defineEventHandler"];
const USED_EXPORTS_MIDDLEWARE: &[&str] = &["default"];

/// Virtual module prefixes provided by Nuxt at build time.
const VIRTUAL_MODULE_PREFIXES: &[&str] = &["#"];

pub struct NuxtPlugin;

impl Plugin for NuxtPlugin {
    fn name(&self) -> &'static str {
        "nuxt"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
    }

    fn config_patterns(&self) -> &'static [&'static str] {
        CONFIG_PATTERNS
    }

    fn always_used(&self) -> &'static [&'static str] {
        ALWAYS_USED
    }

    fn tooling_dependencies(&self) -> &'static [&'static str] {
        TOOLING_DEPENDENCIES
    }

    fn virtual_module_prefixes(&self) -> &'static [&'static str] {
        VIRTUAL_MODULE_PREFIXES
    }

    fn path_aliases(&self, root: &Path) -> Vec<(&'static str, String)> {
        // Nuxt's srcDir defaults to `app/` when the directory exists, otherwise root.
        let src_dir = if root.join("app").is_dir() {
            "app".to_string()
        } else {
            String::new()
        };
        let mut aliases = vec![
            // ~/  → srcDir (app/ or root)
            ("~/", src_dir.clone()),
            // ~~/ → rootDir (project root)
            ("~~/", String::new()),
            // #shared/ → shared/ directory
            ("#shared/", "shared".to_string()),
            // #server/ → server/ directory
            ("#server/", "server".to_string()),
        ];
        // Also map the bare `~` and `~~` (without trailing slash) for edge cases
        // like `import '~/composables/foo'` — already covered by `~/` prefix.
        // Map #shared (without slash) for bare imports like `import '#shared'`
        aliases.push(("#shared", "shared".to_string()));
        aliases.push(("#server", "server".to_string()));
        aliases
    }

    fn used_exports(&self) -> Vec<(&'static str, &'static [&'static str])> {
        vec![
            ("server/api/**/*.{ts,js}", USED_EXPORTS_SERVER_API),
            ("middleware/**/*.{ts,js}", USED_EXPORTS_MIDDLEWARE),
        ]
    }

    fn resolve_config(&self, config_path: &Path, source: &str, root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        // Detect whether this project uses the app/ directory structure.
        // In Nuxt 3, `~` resolves to srcDir (defaults to `app/` when the directory exists).
        let has_app_dir = root.join("app").is_dir();

        // Extract import sources as referenced dependencies
        let imports = config_parser::extract_imports(source, config_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        // modules: [...] → referenced dependencies (Nuxt modules are npm packages)
        let modules = config_parser::extract_config_string_array(source, config_path, &["modules"]);
        for module in &modules {
            let dep = crate::resolve::extract_package_name(module);
            result.referenced_dependencies.push(dep);
        }

        // css: [...] → always-used files or referenced dependencies
        // Nuxt aliases: `~/` = srcDir (app/ or root), `~~/` = rootDir
        // npm package CSS (e.g., `@unocss/reset/tailwind.css`) → referenced dependency
        let css = config_parser::extract_config_string_array(source, config_path, &["css"]);
        for entry in &css {
            if let Some(stripped) = entry.strip_prefix("~/") {
                // ~ = srcDir: resolve to app/ if it exists, otherwise project root
                if has_app_dir {
                    result.always_used_files.push(format!("app/{stripped}"));
                } else {
                    result.always_used_files.push(stripped.to_string());
                }
            } else if let Some(stripped) = entry.strip_prefix("~~/") {
                // ~~ = rootDir: always relative to project root
                result.always_used_files.push(stripped.to_string());
            } else if entry.starts_with('.') || entry.starts_with('/') {
                // Relative or absolute local path
                result.always_used_files.push(entry.clone());
            } else {
                // npm package CSS (e.g., `@unocss/reset/tailwind.css`, `floating-vue/dist/style.css`)
                let dep = crate::resolve::extract_package_name(entry);
                result.referenced_dependencies.push(dep);
            }
        }

        // postcss.plugins → referenced dependencies (object keys)
        let postcss_plugins =
            config_parser::extract_config_object_keys(source, config_path, &["postcss", "plugins"]);
        for plugin in &postcss_plugins {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(plugin));
        }

        // plugins: [...] → entry patterns
        let plugins = config_parser::extract_config_string_array(source, config_path, &["plugins"]);
        result.entry_patterns.extend(plugins);

        // extends: [...] → referenced dependencies
        let extends = config_parser::extract_config_string_array(source, config_path, &["extends"]);
        for ext in &extends {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(ext));
        }

        result
    }
}
