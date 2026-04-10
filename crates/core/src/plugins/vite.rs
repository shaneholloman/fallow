//! Vite bundler plugin.
//!
//! Detects Vite projects and marks conventional entry points and config files.
//! Parses vite config to extract entry points, dependency references, and SSR externals.

use super::config_parser;
use super::{Plugin, PluginResult};

define_plugin!(
    struct VitePlugin => "vite",
    enablers: &["vite", "rolldown-vite"],
    entry_patterns: &[
        "src/main.{ts,tsx,js,jsx}",
        "src/index.{ts,tsx,js,jsx}",
        "index.html",
    ],
    config_patterns: &["vite.config.{ts,js,mts,mjs}"],
    always_used: &["vite.config.{ts,js,mts,mjs}"],
    tooling_dependencies: &["vite", "@vitejs/plugin-react", "@vitejs/plugin-vue"],
    // Vite plugins create virtual modules with `virtual:` prefix
    // (e.g., `virtual:pwa-register`, `virtual:emoji-mart-lang-importer`)
    virtual_module_prefixes: &["virtual:"],
    resolve_config(config_path, source, root) {
        let mut result = PluginResult::default();

        let imports = config_parser::extract_imports(source, config_path);
        for imp in &imports {
            let dep = crate::resolve::extract_package_name(imp);
            result.referenced_dependencies.push(dep);
        }

        for (find, replacement) in
            config_parser::extract_config_aliases(source, config_path, &["resolve", "alias"])
        {
            if let Some(normalized) =
                config_parser::normalize_config_path(&replacement, config_path, root)
            {
                result.path_aliases.push((find, normalized));
            }
        }

        // build.rollupOptions.input → entry points (string, array, or object)
        let rollup_input = config_parser::extract_config_string_or_array(
            source,
            config_path,
            &["build", "rollupOptions", "input"],
        );
        result.extend_entry_patterns(rollup_input);

        // build.lib.entry → entry points (string or array)
        let lib_entry = config_parser::extract_config_string_or_array(
            source,
            config_path,
            &["build", "lib", "entry"],
        );
        result.extend_entry_patterns(lib_entry);

        // optimizeDeps.include → referenced dependencies
        let optimize_include = config_parser::extract_config_string_array(
            source,
            config_path,
            &["optimizeDeps", "include"],
        );
        for dep in &optimize_include {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(dep));
        }

        // optimizeDeps.exclude → referenced dependencies
        let optimize_exclude = config_parser::extract_config_string_array(
            source,
            config_path,
            &["optimizeDeps", "exclude"],
        );
        for dep in &optimize_exclude {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(dep));
        }

        // ssr.external → referenced dependencies
        let ssr_external =
            config_parser::extract_config_string_array(source, config_path, &["ssr", "external"]);
        for dep in &ssr_external {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(dep));
        }

        // ssr.noExternal → referenced dependencies
        let ssr_no_external =
            config_parser::extract_config_string_array(source, config_path, &["ssr", "noExternal"]);
        for dep in &ssr_no_external {
            result
                .referenced_dependencies
                .push(crate::resolve::extract_package_name(dep));
        }

        result
    },
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_config_ssr_external() {
        let source = r#"
            export default {
                ssr: {
                    external: ["lodash", "express"],
                    noExternal: ["my-ui-lib"]
                }
            };
        "#;
        let plugin = VitePlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("vite.config.ts"),
            source,
            std::path::Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"lodash".to_string()));
        assert!(deps.contains(&"express".to_string()));
        assert!(deps.contains(&"my-ui-lib".to_string()));
    }

    #[test]
    fn resolve_config_optimize_deps_exclude() {
        let source = r#"
            export default {
                optimizeDeps: {
                    include: ["react"],
                    exclude: ["@my/heavy-dep"]
                }
            };
        "#;
        let plugin = VitePlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("vite.config.ts"),
            source,
            std::path::Path::new("/project"),
        );
        let deps = &result.referenced_dependencies;
        assert!(deps.contains(&"react".to_string()));
        assert!(deps.contains(&"@my/heavy-dep".to_string()));
    }

    #[test]
    fn resolve_config_extracts_aliases() {
        let source = r#"
            import { defineConfig } from 'vite';
            import { fileURLToPath, URL } from 'node:url';

            export default defineConfig({
                resolve: {
                    alias: {
                        "@": fileURLToPath(new URL("./src", import.meta.url))
                    }
                }
            });
        "#;
        let plugin = VitePlugin;
        let result = plugin.resolve_config(
            std::path::Path::new("/project/vite.config.ts"),
            source,
            std::path::Path::new("/project"),
        );

        assert_eq!(
            result.path_aliases,
            vec![("@".to_string(), "src".to_string())]
        );
    }
}
