use crate::tests::parse_ts as parse_source;

// -- Dynamic import pattern extraction --

#[test]
fn extracts_template_literal_dynamic_import_pattern() {
    let info = parse_source("const m = import(`./locales/${lang}.json`);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./locales/");
    assert_eq!(
        info.dynamic_import_patterns[0].suffix,
        Some(".json".to_string())
    );
}

#[test]
fn extracts_concat_dynamic_import_pattern() {
    let info = parse_source("const m = import('./pages/' + name);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./pages/");
    assert!(info.dynamic_import_patterns[0].suffix.is_none());
}

#[test]
fn extracts_concat_with_suffix() {
    let info = parse_source("const m = import('./pages/' + name + '.tsx');");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./pages/");
    assert_eq!(
        info.dynamic_import_patterns[0].suffix,
        Some(".tsx".to_string())
    );
}

#[test]
fn no_substitution_template_treated_as_exact() {
    let info = parse_source("const m = import(`./exact-module`);");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./exact-module");
    assert!(info.dynamic_import_patterns.is_empty());
}

#[test]
fn fully_dynamic_import_still_ignored() {
    let info = parse_source("const m = import(variable);");
    assert!(info.dynamic_imports.is_empty());
    assert!(info.dynamic_import_patterns.is_empty());
}

#[test]
fn non_relative_template_ignored() {
    let info = parse_source("const m = import(`lodash/${fn}`);");
    assert!(info.dynamic_import_patterns.is_empty());
}

#[test]
fn multi_expression_template_uses_globstar() {
    let info = parse_source("const m = import(`./plugins/${cat}/${name}.js`);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./plugins/**/");
    assert_eq!(
        info.dynamic_import_patterns[0].suffix,
        Some(".js".to_string())
    );
}

// -- import.meta.glob / require.context --

#[test]
fn extracts_import_meta_glob_pattern() {
    let info = parse_source("const mods = import.meta.glob('./components/*.tsx');");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./components/*.tsx");
}

#[test]
fn extracts_import_meta_glob_array() {
    let info = parse_source("const mods = import.meta.glob(['./pages/*.ts', './layouts/*.ts']);");
    assert_eq!(info.dynamic_import_patterns.len(), 2);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./pages/*.ts");
    assert_eq!(info.dynamic_import_patterns[1].prefix, "./layouts/*.ts");
}

#[test]
fn extracts_require_context_pattern() {
    let info = parse_source("const ctx = require.context('./icons', false);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./icons/");
}

#[test]
fn extracts_require_context_recursive() {
    let info = parse_source("const ctx = require.context('./icons', true);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./icons/**/");
}

#[test]
fn vitest_mock_records_auto_mock_sibling() {
    let info = parse_source("vi.mock('./services/api');");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./services/__mocks__/api");
    assert_eq!(info.dynamic_imports[0].local_name, Some(String::new()));
}

#[test]
fn vitest_mock_records_auto_mock_sibling_from_import_argument() {
    let info = parse_source("vi.mock(import('./services/api'));");
    let auto_mock = info
        .dynamic_imports
        .iter()
        .find(|imp| imp.source == "./services/__mocks__/api")
        .expect("vi.mock(import(...)) should record the auto-mock sibling");
    assert_eq!(auto_mock.local_name, Some(String::new()));
}

// -- Dynamic import namespace tracking --

#[test]
fn dynamic_import_await_captures_local_name() {
    let info = parse_source(
        "async function f() { const mod = await import('./service'); mod.doStuff(); }",
    );
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./service");
    assert_eq!(info.dynamic_imports[0].local_name, Some("mod".to_string()));
    assert!(info.dynamic_imports[0].destructured_names.is_empty());
}

#[test]
fn dynamic_import_without_await_captures_local_name() {
    let info = parse_source("const mod = import('./service');");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./service");
    assert_eq!(info.dynamic_imports[0].local_name, Some("mod".to_string()));
}

#[test]
fn dynamic_import_destructured_captures_names() {
    let info =
        parse_source("async function f() { const { foo, bar } = await import('./module'); }");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./module");
    assert!(info.dynamic_imports[0].local_name.is_none());
    assert_eq!(
        info.dynamic_imports[0].destructured_names,
        vec!["foo", "bar"]
    );
}

#[test]
fn dynamic_import_destructured_with_rest_is_namespace() {
    let info =
        parse_source("async function f() { const { foo, ...rest } = await import('./module'); }");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./module");
    assert!(info.dynamic_imports[0].local_name.is_none());
    assert!(info.dynamic_imports[0].destructured_names.is_empty());
}

#[test]
fn dynamic_import_side_effect_only() {
    let info = parse_source("async function f() { await import('./side-effect'); }");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./side-effect");
    assert!(info.dynamic_imports[0].local_name.is_none());
    assert!(info.dynamic_imports[0].destructured_names.is_empty());
}

#[test]
fn dynamic_import_no_duplicate_entries() {
    let info = parse_source("async function f() { const mod = await import('./service'); }");
    assert_eq!(info.dynamic_imports.len(), 1);
}

// ── require.context with regex pattern ──────────────────────

#[test]
fn require_context_with_json_regex() {
    let info = parse_source(r"const ctx = require.context('./locale', false, /\.json$/);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./locale/");
    assert_eq!(
        info.dynamic_import_patterns[0].suffix,
        Some(".json".to_string())
    );
}

// ── Dynamic import string concatenation patterns ────────────

#[test]
fn dynamic_import_concat_prefix_only() {
    let info = parse_source("const m = import('./pages/' + name);");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./pages/");
    assert!(
        info.dynamic_import_patterns[0].suffix.is_none(),
        "Concat with only prefix and variable should have no suffix"
    );
}

#[test]
fn dynamic_import_concat_prefix_and_suffix() {
    let info = parse_source("const m = import('./views/' + name + '.vue');");
    assert_eq!(info.dynamic_import_patterns.len(), 1);
    assert_eq!(info.dynamic_import_patterns[0].prefix, "./views/");
    assert_eq!(
        info.dynamic_import_patterns[0].suffix,
        Some(".vue".to_string())
    );
}

// ── Arrow-wrapped dynamic imports ────────────────────────────

#[test]
fn arrow_wrapped_import_expression_body() {
    let info = parse_source("const Foo = React.lazy(() => import('./Foo'));");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_import_block_body() {
    let info = parse_source("const Foo = lazy(() => { return import('./Foo'); });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_import_function_expression() {
    let info = parse_source("const Foo = loadable(function() { return import('./Foo'); });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_import_vue_define_async() {
    let info = parse_source("const Comp = defineAsyncComponent(() => import('./MyComp'));");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./MyComp");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_import_no_duplicate() {
    // Should NOT produce a duplicate side-effect import alongside the arrow-wrapped one
    let info = parse_source("React.lazy(() => import('./Foo'));");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn non_import_arrow_not_extracted() {
    // Arrow that doesn't contain an import() should not produce a dynamic import
    let info = parse_source("const result = someFunc(() => doSomething());");
    assert_eq!(info.dynamic_imports.len(), 0);
}

#[test]
fn arrow_wrapped_import_second_argument() {
    // Import callback is the second argument, not the first
    let info = parse_source("const Foo = createLazy(config, () => import('./Foo'));");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_import_async_arrow() {
    let info = parse_source("const Foo = lazy(async () => import('./Foo'));");
    // Async arrow's body is still an expression containing import()
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
}

#[test]
fn arrow_wrapped_import_with_non_import_first_arg() {
    // First arg is not an import arrow, second arg IS
    let info = parse_source("const Foo = wrapper('options', () => import('./Foo'));");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./Foo");
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["default"]);
}

#[test]
fn arrow_wrapped_template_literal_source() {
    // Template literal source in arrow — should NOT produce a DynamicImportInfo
    // (it's a dynamic pattern, not a static import)
    let info = parse_source("const Foo = lazy(() => import(`./pages/${name}`));");
    assert_eq!(info.dynamic_imports.len(), 0);
    // But it should produce a dynamic_import_pattern
    assert_eq!(info.dynamic_import_patterns.len(), 1);
}

// ── Dynamic import .then() callback patterns ────────────────

#[test]
fn then_callback_expression_body_member_access() {
    // Angular lazy loading: `import('./x').then(m => m.Component)`
    let info = parse_source("import('./dashboard').then(m => m.DashboardComponent);");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./dashboard");
    assert_eq!(
        info.dynamic_imports[0].destructured_names,
        vec!["DashboardComponent"]
    );
    assert!(info.dynamic_imports[0].local_name.is_none());
}

#[test]
fn then_callback_destructured_param() {
    // Destructured: `import('./x').then(({ foo, bar }) => { ... })`
    let info = parse_source("import('./lib').then(({ foo, bar }) => { console.log(foo, bar); });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./lib");
    assert_eq!(
        info.dynamic_imports[0].destructured_names,
        vec!["foo", "bar"]
    );
    assert!(info.dynamic_imports[0].local_name.is_none());
}

#[test]
fn then_callback_namespace_block_body() {
    // Block body with identifier param falls back to namespace binding
    let info = parse_source("import('./service').then(m => { m.doStuff(); m.doMore(); });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./service");
    assert!(info.dynamic_imports[0].destructured_names.is_empty());
    assert_eq!(info.dynamic_imports[0].local_name, Some("m".to_string()));
}

#[test]
fn then_callback_angular_routes_pattern() {
    // Real-world Angular routing pattern
    let info = parse_source(
        r"
        const routes = [
            {
                path: 'dashboard',
                loadComponent: () => import('./dashboard.component').then(m => m.DashboardComponent),
            },
            {
                path: 'settings',
                loadComponent: () => import('./settings.component').then(m => m.SettingsComponent),
            },
        ];
        ",
    );
    assert_eq!(info.dynamic_imports.len(), 2);
    assert_eq!(info.dynamic_imports[0].source, "./dashboard.component");
    assert_eq!(
        info.dynamic_imports[0].destructured_names,
        vec!["DashboardComponent"]
    );
    assert_eq!(info.dynamic_imports[1].source, "./settings.component");
    assert_eq!(
        info.dynamic_imports[1].destructured_names,
        vec!["SettingsComponent"]
    );
}

#[test]
fn then_callback_object_literal_body() {
    // React.lazy .then pattern: `import('./x').then(m => ({ default: m.Foo }))`
    let info = parse_source(
        "const Comp = React.lazy(() => import('./Foo').then(m => ({ default: m.FooComponent })));",
    );
    // The outer React.lazy would normally capture this, but the .then() should also fire
    assert!(
        info.dynamic_imports
            .iter()
            .any(|d| d.source == "./Foo"
                && d.destructured_names.contains(&"FooComponent".to_string()))
    );
}

#[test]
fn then_callback_no_duplicate_side_effect() {
    // Should NOT produce a duplicate side-effect import alongside the .then() one
    let info = parse_source("import('./lib').then(m => m.foo);");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].destructured_names, vec!["foo"]);
}

#[test]
fn then_callback_function_expression() {
    // Function expression callback
    let info = parse_source("import('./lib').then(function(m) { return m.foo; });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert_eq!(info.dynamic_imports[0].source, "./lib");
    assert_eq!(info.dynamic_imports[0].local_name, Some("m".to_string()));
}

#[test]
fn then_callback_destructured_with_rest_is_namespace() {
    // Rest element means we can't know all accessed names
    let info = parse_source("import('./lib').then(({ foo, ...rest }) => { });");
    assert_eq!(info.dynamic_imports.len(), 1);
    assert!(info.dynamic_imports[0].destructured_names.is_empty());
    assert!(info.dynamic_imports[0].local_name.is_none());
}

#[test]
fn then_callback_non_import_callee_ignored() {
    // `.then()` on a non-import should not produce a dynamic import
    let info = parse_source("fetch('/api').then(r => r.json());");
    assert!(info.dynamic_imports.is_empty());
}
