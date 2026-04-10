//! Nx monorepo plugin.
//!
//! Detects Nx projects and marks workspace config files as always used.
//! Parses `project.json` to extract executor references as tooling dependencies
//! and `options.main` as entry points.

#[cfg(test)]
use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

define_plugin!(
    struct NxPlugin => "nx",
    enablers: &["nx"],
    config_patterns: &["**/project.json"],
    always_used: &["nx.json", "**/project.json"],
    tooling_dependencies: &[
        "nx",
        "@nx/workspace",
        "@nx/js",
        "@nx/react",
        "@nx/next",
        "@nx/node",
        "@nx/web",
        "@nx/vite",
        "@nx/jest",
        "@nx/eslint",
        "@nx/angular",
        "@nx/storybook",
        "@nx/webpack",
        "@nx/cypress",
        "@nx/playwright",
        "@nx/rollup",
        "@nx/esbuild",
        "@nx/rspack",
        "@nx/express",
        "@nx/nest",
    ],
    resolve_config(config_path, source, _root) {
        let mut result = PluginResult::default();

        // project.json: targets.*.executor → referenced dependency
        // Format: "@angular/build:application" or "@nx/vite:build"
        // Extract the package name before the ":" separator.
        let executor_strings = config_parser::extract_config_object_nested_strings(
            source,
            config_path,
            &["targets"],
            &["executor"],
        );
        for executor in &executor_strings {
            if let Some(pkg) = executor.split(':').next()
                && !pkg.is_empty()
            {
                result.referenced_dependencies.push(pkg.to_string());
            }
        }

        // project.json: targets.*.options.main → entry point
        let mains = config_parser::extract_config_object_nested_strings(
            source,
            config_path,
            &["targets"],
            &["options", "main"],
        );
        for main in &mains {
            let path = main.trim_start_matches("./");
            result.push_entry_pattern(path.to_string());
        }

        // project.json: targets.*.options.tsConfig → always used
        let tsconfigs = config_parser::extract_config_object_nested_strings(
            source,
            config_path,
            &["targets"],
            &["options", "tsConfig"],
        );
        for tsconfig in &tsconfigs {
            let path = tsconfig.trim_start_matches("./");
            result.always_used_files.push(path.to_string());
        }

        result
    },
);

#[cfg(test)]
mod tests {
    use super::*;

    fn has_entry_pattern(result: &PluginResult, pattern: &str) -> bool {
        result
            .entry_patterns
            .iter()
            .any(|entry_pattern| entry_pattern.pattern == pattern)
    }

    #[test]
    fn resolve_config_extracts_executor() {
        let source = r#"{
            "targets": {
                "build": {
                    "executor": "@angular/build:application"
                },
                "test": {
                    "executor": "@nx/vite:test"
                }
            }
        }"#;
        let plugin = NxPlugin;
        let result =
            plugin.resolve_config(Path::new("project.json"), source, Path::new("/project"));
        assert!(
            result
                .referenced_dependencies
                .contains(&"@angular/build".to_string())
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"@nx/vite".to_string())
        );
    }

    #[test]
    fn resolve_config_extracts_main() {
        let source = r#"{
            "targets": {
                "build": {
                    "executor": "@angular/build:application",
                    "options": {
                        "main": "apps/client/src/main.ts"
                    }
                }
            }
        }"#;
        let plugin = NxPlugin;
        let result =
            plugin.resolve_config(Path::new("project.json"), source, Path::new("/project"));
        assert!(has_entry_pattern(&result, "apps/client/src/main.ts"));
    }

    #[test]
    fn resolve_config_extracts_tsconfig() {
        let source = r#"{
            "targets": {
                "build": {
                    "executor": "@angular/build:application",
                    "options": {
                        "tsConfig": "apps/client/tsconfig.app.json"
                    }
                }
            }
        }"#;
        let plugin = NxPlugin;
        let result =
            plugin.resolve_config(Path::new("project.json"), source, Path::new("/project"));
        assert!(
            result
                .always_used_files
                .contains(&"apps/client/tsconfig.app.json".to_string())
        );
    }

    #[test]
    fn resolve_config_empty_targets() {
        let source = r#"{ "targets": {} }"#;
        let plugin = NxPlugin;
        let result =
            plugin.resolve_config(Path::new("project.json"), source, Path::new("/project"));
        assert!(result.referenced_dependencies.is_empty());
        assert!(result.entry_patterns.is_empty());
    }
}
