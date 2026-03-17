use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

pub struct AstroPlugin;

const ENABLERS: &[&str] = &["astro"];

const ENTRY_PATTERNS: &[&str] = &["src/content/config.ts"];

const PRODUCTION_PATTERNS: &[&str] = &[
    "src/pages/**/*.{astro,ts,tsx,js,jsx,md,mdx}",
    "src/layouts/**/*.astro",
    "src/content/**/*.{ts,js,md,mdx}",
    "src/middleware.{js,ts}",
];

const CONFIG_PATTERNS: &[&str] = &["astro.config.{ts,js,mjs}"];

const ALWAYS_USED: &[&str] = &["astro.config.{ts,js,mjs}"];

const TOOLING_DEPENDENCIES: &[&str] = &["astro", "@astrojs/check", "@astrojs/ts-plugin"];

impl Plugin for AstroPlugin {
    fn name(&self) -> &'static str {
        "Astro"
    }

    fn enablers(&self) -> &'static [&'static str] {
        ENABLERS
    }

    fn entry_patterns(&self) -> &'static [&'static str] {
        ENTRY_PATTERNS
    }

    fn production_patterns(&self) -> &'static [&'static str] {
        PRODUCTION_PATTERNS
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

    fn resolve_config(&self, config_path: &Path, source: &str, _root: &Path) -> PluginResult {
        let mut result = PluginResult::default();

        let imports = config_parser::extract_imports(source, config_path);
        for import in imports {
            result.referenced_dependencies.push(import);
        }

        result
    }
}
