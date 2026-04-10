//! Jest test runner plugin.
//!
//! Detects Jest projects and marks test files as entry points.
//! Parses jest.config to extract setupFiles, testMatch, transform,
//! reporters, testEnvironment, preset, globalSetup/Teardown, watchPlugins,
//! resolver, snapshotSerializers, testRunner, and runner as referenced dependencies.

use std::path::Path;

use super::config_parser;
use super::{Plugin, PluginResult};

/// Built-in Jest reporter names that should not be treated as dependencies.
const BUILTIN_REPORTERS: &[&str] = &["default", "verbose", "summary"];

define_plugin!(
    struct JestPlugin => "jest",
    enablers: &["jest"],
    entry_patterns: &[
        "**/*.test.{ts,tsx,js,jsx}",
        "**/*.spec.{ts,tsx,js,jsx}",
        "**/__tests__/**/*.{ts,tsx,js,jsx}",
        "**/__mocks__/**/*.{ts,tsx,js,jsx,mjs,cjs}",
    ],
    config_patterns: &["jest.config.{ts,js,mjs,cjs}", "jest.config.json"],
    always_used: &["jest.config.{ts,js,mjs,cjs}", "jest.setup.{ts,js,tsx,jsx}"],
    tooling_dependencies: &["jest", "jest-environment-jsdom", "ts-jest", "babel-jest"],
    fixture_glob_patterns: &[
        "**/__fixtures__/**/*.{ts,tsx,js,jsx,json}",
        "**/fixtures/**/*.{ts,tsx,js,jsx,json}",
    ],
    package_json_config_key: "jest",
    resolve_config(config_path, source, root) {
        let mut result = PluginResult::default();

        // Handle JSON configs (jest.config.json)
        let is_json = config_path.extension().is_some_and(|ext| ext == "json");
        let (parse_source, parse_path_buf) = if is_json {
            (format!("({source})"), config_path.with_extension("js"))
        } else {
            (source.to_string(), config_path.to_path_buf())
        };
        let parse_path: &Path = &parse_path_buf;

        // Extract import sources as referenced dependencies
        let imports = config_parser::extract_imports(&parse_source, parse_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        extract_jest_setup_files(&parse_source, parse_path, root, &mut result);
        extract_jest_dependencies(&parse_source, parse_path, &mut result);

        result
    },
);

/// Extract setup files from Jest config (setupFiles, setupFilesAfterEnv, globalSetup, globalTeardown).
fn extract_jest_setup_files(
    parse_source: &str,
    parse_path: &Path,
    root: &Path,
    result: &mut PluginResult,
) {
    // preset → referenced dependency (e.g., "ts-jest", "react-native")
    if let Some(preset) =
        config_parser::extract_config_string(parse_source, parse_path, &["preset"])
    {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(&preset));
    }

    for key in &["setupFiles", "setupFilesAfterEnv"] {
        let files = config_parser::extract_config_string_array(parse_source, parse_path, &[key]);
        for f in &files {
            result
                .setup_files
                .push(root.join(f.trim_start_matches("./")));
        }
    }

    for key in &["globalSetup", "globalTeardown"] {
        if let Some(path) = config_parser::extract_config_string(parse_source, parse_path, &[key]) {
            result
                .setup_files
                .push(root.join(path.trim_start_matches("./")));
        }
    }

    // testMatch → entry patterns that replace defaults
    // Jest treats testMatch as a full override of its default patterns,
    // so when present the static ENTRY_PATTERNS should be dropped.
    let test_match =
        config_parser::extract_config_string_array(parse_source, parse_path, &["testMatch"]);
    if !test_match.is_empty() {
        result.replace_entry_patterns = true;
    }
    result.extend_entry_patterns(test_match);

    // testRegex → convert to best-effort glob and replace defaults
    // Jest's testRegex restricts which files are tests. Common pattern: "src/.*\\.test\\.ts$"
    // Extract a directory prefix (if any) and generate a matching glob.
    if result.entry_patterns.is_empty()
        && let Some(regex) =
            config_parser::extract_config_string(parse_source, parse_path, &["testRegex"])
        && let Some(glob) = test_regex_to_glob(&regex)
    {
        result.replace_entry_patterns = true;
        result.push_entry_pattern(glob);
    }
}

/// Best-effort conversion of a Jest `testRegex` to a glob pattern.
///
/// Handles common patterns like:
/// - `"src/.*\\.test\\.ts$"` → `"src/**/*.test.ts"`
/// - `".*\\.(test|spec)\\.tsx?$"` → stays as defaults (no fixed prefix)
fn test_regex_to_glob(regex: &str) -> Option<String> {
    // Extract a fixed directory prefix before the first regex metachar
    let meta_chars = ['.', '*', '+', '?', '(', '[', '|', '^', '$', '{', '\\'];
    let prefix_end = regex
        .find(|c: char| meta_chars.contains(&c))
        .unwrap_or(regex.len());
    let prefix = &regex[..prefix_end];

    // Must have a non-empty directory prefix to be useful (otherwise same as defaults)
    if prefix.is_empty() || !prefix.contains('/') {
        return None;
    }

    // Detect file extension from the regex suffix
    let ext = if regex.contains("tsx?") {
        "{ts,tsx}"
    } else if regex.contains("jsx?") {
        "{js,jsx}"
    } else if regex.contains("\\.ts") {
        "ts"
    } else if regex.contains("\\.js") {
        "js"
    } else {
        "{ts,tsx,js,jsx}"
    };

    // Detect test naming convention
    let name_pattern = if regex.contains("(test|spec)") || regex.contains("(spec|test)") {
        "*.{test,spec}"
    } else if regex.contains("\\.spec\\.") {
        "*.spec"
    } else {
        "*.test"
    };

    Some(format!("{prefix}**/{name_pattern}.{ext}"))
}

/// Extract referenced dependencies from Jest config (transform, reporters, environment, etc.).
fn extract_jest_dependencies(parse_source: &str, parse_path: &Path, result: &mut PluginResult) {
    // transform values → referenced dependencies
    let transform_values =
        config_parser::extract_config_shallow_strings(parse_source, parse_path, "transform");
    for val in &transform_values {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(val));
    }

    // reporters → referenced dependencies
    let reporters =
        config_parser::extract_config_shallow_strings(parse_source, parse_path, "reporters");
    for reporter in &reporters {
        if !BUILTIN_REPORTERS.contains(&reporter.as_str()) {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(reporter));
        }
    }

    // testEnvironment → if not built-in, it's a referenced dependency
    if let Some(env) =
        config_parser::extract_config_string(parse_source, parse_path, &["testEnvironment"])
        && !matches!(env.as_str(), "node" | "jsdom")
    {
        result
            .referenced_dependencies
            .push(format!("jest-environment-{env}"));
        result.referenced_dependencies.push(env);
    }

    // watchPlugins → referenced dependencies
    let watch_plugins =
        config_parser::extract_config_shallow_strings(parse_source, parse_path, "watchPlugins");
    for plugin in &watch_plugins {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(plugin));
    }

    // resolver → referenced dependency (only if it's a package, not a relative path)
    if let Some(resolver) =
        config_parser::extract_config_string(parse_source, parse_path, &["resolver"])
        && !resolver.starts_with('.')
        && !resolver.starts_with('/')
    {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(&resolver));
    }

    // snapshotSerializers → referenced dependencies
    let serializers = config_parser::extract_config_string_array(
        parse_source,
        parse_path,
        &["snapshotSerializers"],
    );
    for s in &serializers {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(s));
    }

    // testRunner → referenced dependency (filter built-in runners)
    if let Some(runner) =
        config_parser::extract_config_string(parse_source, parse_path, &["testRunner"])
        && !matches!(
            runner.as_str(),
            "jest-jasmine2" | "jest-circus" | "jest-circus/runner"
        )
    {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(&runner));
    }

    // runner → referenced dependency (process runner, not test runner)
    if let Some(runner) =
        config_parser::extract_config_string(parse_source, parse_path, &["runner"])
        && runner != "jest-runner"
    {
        result
            .referenced_dependencies
            .push(crate::resolve::extract_package_name(&runner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_config_preset() {
        let source = r#"module.exports = { preset: "ts-jest" };"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"ts-jest".to_string())
        );
    }

    #[test]
    fn resolve_config_global_setup_teardown() {
        let source = r#"
            module.exports = {
                globalSetup: "./test/global-setup.ts",
                globalTeardown: "./test/global-teardown.ts"
            };
        "#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .setup_files
                .contains(&std::path::PathBuf::from("/project/test/global-setup.ts"))
        );
        assert!(result.setup_files.contains(&std::path::PathBuf::from(
            "/project/test/global-teardown.ts"
        )));
    }

    #[test]
    fn resolve_config_watch_plugins() {
        let source = r#"
            module.exports = {
                watchPlugins: [
                    "jest-watch-typeahead/filename",
                    "jest-watch-typeahead/testname"
                ]
            };
        "#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"jest-watch-typeahead".to_string()));
    }

    #[test]
    fn resolve_config_resolver() {
        let source = r#"module.exports = { resolver: "jest-resolver-enhanced" };"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"jest-resolver-enhanced".to_string())
        );
    }

    #[test]
    fn resolve_config_resolver_relative_not_added() {
        let source = r#"module.exports = { resolver: "./custom-resolver.js" };"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            !result
                .referenced_dependencies
                .iter()
                .any(|d| d.contains("custom-resolver"))
        );
    }

    #[test]
    fn resolve_config_snapshot_serializers() {
        let source = r#"
            module.exports = {
                snapshotSerializers: ["enzyme-to-json/serializer"]
            };
        "#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"enzyme-to-json".to_string())
        );
    }

    #[test]
    fn resolve_config_test_runner_builtin() {
        let source = r#"module.exports = { testRunner: "jest-circus/runner" };"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            !result
                .referenced_dependencies
                .iter()
                .any(|d| d.contains("jest-circus"))
        );
    }

    #[test]
    fn resolve_config_custom_runner() {
        let source = r#"module.exports = { runner: "jest-runner-eslint" };"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.js"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"jest-runner-eslint".to_string())
        );
    }

    #[test]
    fn resolve_config_json() {
        let source = r#"{"preset": "ts-jest", "testEnvironment": "jsdom"}"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.json"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"ts-jest".to_string())
        );
    }

    #[test]
    fn test_regex_with_directory_prefix() {
        assert_eq!(
            test_regex_to_glob(r"src/.*\.test\.ts$"),
            Some("src/**/*.test.ts".to_string())
        );
    }

    #[test]
    fn test_regex_without_directory_prefix() {
        assert_eq!(
            test_regex_to_glob(r".*\.test\.ts$"),
            None,
            "regex without directory prefix should return None (same as defaults)"
        );
    }

    #[test]
    fn test_regex_tsx_extension() {
        assert_eq!(
            test_regex_to_glob(r"src/.*\.test\.tsx?$"),
            Some("src/**/*.test.{ts,tsx}".to_string())
        );
    }

    #[test]
    fn test_regex_spec_pattern() {
        assert_eq!(
            test_regex_to_glob(r"src/.*\.spec\.ts$"),
            Some("src/**/*.spec.ts".to_string())
        );
    }

    #[test]
    fn test_regex_test_or_spec() {
        assert_eq!(
            test_regex_to_glob(r"src/.*(test|spec)\.ts$"),
            Some("src/**/*.{test,spec}.ts".to_string())
        );
    }

    #[test]
    fn resolve_config_test_regex_replaces_defaults() {
        let source =
            r#"{"testRegex": "src/.*\\.test\\.ts$", "transform": {"^.+\\.tsx?$": "ts-jest"}}"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.json"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result.replace_entry_patterns,
            "testRegex with directory prefix should trigger replacement"
        );
        assert_eq!(result.entry_patterns, vec!["src/**/*.test.ts"]);
    }

    #[test]
    fn resolve_config_json_transform_object_values() {
        let source = r#"{"transform": {"^.+\\.tsx?$": "ts-jest"}}"#;
        let plugin = JestPlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("jest.config.json"),
            source,
            std::path::Path::new("/project"),
        );
        assert!(
            result
                .referenced_dependencies
                .contains(&"ts-jest".to_string()),
            "should extract transform values from object"
        );
    }
}
