//! Main resolution engine: creates the oxc_resolver instance and resolves individual specifiers.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use json_comments::StripComments;
use oxc_resolver::{Resolution, ResolveError, ResolveOptions, Resolver};
use serde_json::Value;

use super::fallbacks::{
    extract_package_name_from_node_modules_path, try_css_extension_fallback,
    try_path_alias_fallback, try_pnpm_workspace_fallback, try_scss_include_path_fallback,
    try_scss_node_modules_fallback, try_scss_partial_fallback, try_source_fallback,
    try_workspace_package_fallback,
};
use super::path_info::{
    extract_package_name, is_bare_specifier, is_path_alias, is_valid_package_name,
};
use super::react_native::{build_condition_names, build_extensions};
use super::types::{ResolveContext, ResolveResult};

/// Create an `oxc_resolver` instance with standard configuration.
///
/// When React Native or Expo plugins are active, platform-specific extensions
/// (e.g., `.web.tsx`, `.ios.ts`) are prepended to the extension list so that
/// Metro-style platform resolution works correctly. User-supplied
/// `extra_conditions` are prepended to the resolver's `condition_names`
/// list, giving them priority over baseline conditions during package.json
/// `exports` / `imports` matching.
pub(super) fn create_resolver(active_plugins: &[String], extra_conditions: &[String]) -> Resolver {
    let mut options = ResolveOptions {
        extensions: build_extensions(active_plugins),
        // Support TypeScript's node16/nodenext module resolution where .ts files
        // are imported with .js extensions (e.g., `import './api.js'` for `api.ts`).
        extension_alias: vec![
            (
                ".js".into(),
                vec![".ts".into(), ".tsx".into(), ".js".into()],
            ),
            (".jsx".into(), vec![".tsx".into(), ".jsx".into()]),
            (".mjs".into(), vec![".mts".into(), ".mjs".into()]),
            (".cjs".into(), vec![".cts".into(), ".cjs".into()]),
        ],
        condition_names: build_condition_names(active_plugins, extra_conditions),
        main_fields: vec!["module".into(), "main".into()],
        ..Default::default()
    };

    // Always use auto-discovery mode so oxc_resolver finds the nearest tsconfig.json
    // for each file. This is critical for monorepos where workspace packages have
    // their own tsconfig with path aliases (e.g., `~/*` → `./src/*`). Manual mode
    // with a root tsconfig only uses that single tsconfig's paths for ALL files,
    // missing workspace-specific aliases. Auto mode walks up from each file to find
    // the nearest tsconfig.json and follows `extends` chains, so workspace tsconfigs
    // that extend a root tsconfig still inherit root-level paths.
    options.tsconfig = Some(oxc_resolver::TsconfigDiscovery::Auto);

    Resolver::new(options)
}

/// Return `true` for errors raised while loading a tsconfig file (as opposed to
/// errors about the specifier itself). When `resolve_file` fails with one of these,
/// a broken sibling tsconfig is poisoning resolution for the current file — retrying
/// via `resolve(dir, specifier)` bypasses `TsconfigDiscovery::Auto` and restores
/// resolution for everything that does not need path aliases (relative, absolute,
/// bare package specifiers).
///
/// `IOError` and `Json` are included because a malformed or unreadable tsconfig
/// surfaces as one of these — the variants are shared with package.json parsing,
/// but a retry is still safe: if the error really came from the specifier's own
/// resolution, `resolve()` will fail the same way and we fall through to the
/// existing error handling.
const fn is_tsconfig_error(err: &ResolveError) -> bool {
    matches!(
        err,
        ResolveError::TsconfigNotFound(_)
            | ResolveError::TsconfigCircularExtend(_)
            | ResolveError::TsconfigSelfReference(_)
            | ResolveError::Json(_)
            | ResolveError::IOError(_)
    )
}

enum ResolveFileAttempt {
    Resolved(Resolution),
    Failed { used_tsconfig_fallback: bool },
}

/// Try `resolve_file` first (honors per-file tsconfig discovery); on a
/// tsconfig-loading failure, retry with `resolve(dir, specifier)` which skips
/// tsconfig entirely. Emits a single `tracing::warn!` per unique error message
/// so users get one actionable hint per broken tsconfig without log spam.
fn resolve_file_with_tsconfig_fallback(
    ctx: &ResolveContext<'_>,
    from_file: &Path,
    specifier: &str,
) -> ResolveFileAttempt {
    resolve_file_with_resolver_and_tsconfig_fallback(ctx, ctx.resolver, from_file, specifier)
}

fn resolve_file_with_resolver_and_tsconfig_fallback(
    ctx: &ResolveContext<'_>,
    resolver: &Resolver,
    from_file: &Path,
    specifier: &str,
) -> ResolveFileAttempt {
    match resolver.resolve_file(from_file, specifier) {
        Ok(resolution) => ResolveFileAttempt::Resolved(resolution),
        Err(err) if is_tsconfig_error(&err) => {
            warn_once_tsconfig(ctx, &err);
            let dir = from_file.parent().unwrap_or(from_file);
            match resolver.resolve(dir, specifier) {
                Ok(resolution) => ResolveFileAttempt::Resolved(resolution),
                Err(_) => ResolveFileAttempt::Failed {
                    used_tsconfig_fallback: true,
                },
            }
        }
        Err(_) => ResolveFileAttempt::Failed {
            used_tsconfig_fallback: false,
        },
    }
}

/// Emit a `tracing::warn!` the first time a given tsconfig error message is
/// observed. The shared `Mutex<FxHashSet<String>>` in the resolver context
/// dedupes across all parallel threads for the lifetime of one analysis run.
fn warn_once_tsconfig(ctx: &ResolveContext<'_>, err: &ResolveError) {
    let message = err.to_string();
    let should_warn = {
        let Ok(mut seen) = ctx.tsconfig_warned.lock() else {
            // Mutex poisoned by a panic on another thread — stay silent rather
            // than poisoning this thread's resolution with another panic.
            return;
        };
        seen.insert(message.clone())
    };
    if should_warn {
        tracing::warn!(
            "Broken tsconfig chain: {message}. Falling back to resolver-less resolution for \
             affected files. Relative and bare imports still work, but tsconfig path aliases \
             (e.g., `@/...`) will not. Fix the extends/references chain to restore alias support."
        );
    }
}

fn nearest_tsconfig_path(root: &Path, from_file: &Path) -> Option<PathBuf> {
    let mut current = from_file.parent()?;
    loop {
        let candidate = current.join("tsconfig.json");
        if candidate.is_file() {
            return Some(candidate);
        }
        if current == root {
            return None;
        }
        current = current.parent()?;
        if !current.starts_with(root) {
            return None;
        }
    }
}

fn path_alias_pattern_matches(pattern: &str, specifier: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) if !prefix.is_empty() || !suffix.is_empty() => {
            specifier.starts_with(prefix)
                && specifier.ends_with(suffix)
                && specifier.len() >= prefix.len() + suffix.len()
        }
        Some(_) => false,
        None => specifier == pattern,
    }
}

fn matches_nearest_tsconfig_path_alias(root: &Path, from_file: &Path, specifier: &str) -> bool {
    let Some(tsconfig_path) = nearest_tsconfig_path(root, from_file) else {
        return false;
    };
    let Ok(file) = File::open(tsconfig_path) else {
        return false;
    };
    let reader = StripComments::new(BufReader::new(file));
    let Ok(json) = serde_json::from_reader::<_, Value>(reader) else {
        return false;
    };
    let Some(paths) = json
        .get("compilerOptions")
        .and_then(|compiler_options| compiler_options.get("paths"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    paths
        .keys()
        .any(|pattern| path_alias_pattern_matches(pattern, specifier))
}

/// Try the SCSS-specific resolution fallbacks in order: local partial,
/// framework-supplied include paths, and `node_modules/`.
///
/// Applies when the importer is a `.scss` / `.sass` file OR the import
/// originated from an SFC `<style lang="scss">` block (`from_style = true`).
/// SFC importers carry the `.vue` / `.svelte` extension at the file system
/// level but still emit SCSS-shape specifiers from style blocks; the
/// `from_style` flag is the authoritative signal that the import is a
/// CSS-context reference rather than a JS-context import from the same file.
/// Returns `None` when none of the fallbacks produce a hit, so the outer error
/// path continues to the generic alias / bare / workspace fallbacks.
fn try_scss_fallbacks(
    ctx: &ResolveContext<'_>,
    from_file: &Path,
    specifier: &str,
    from_style: bool,
) -> Option<ResolveResult> {
    let is_scss_importer = from_file
        .extension()
        .is_some_and(|e| e == "scss" || e == "sass");
    if !is_scss_importer && !from_style {
        return None;
    }
    // 0. CSS-extension probe: `./Foo` -> `./Foo.scss` / `.sass` / `.css`. The
    //    standard resolver's extension list contains both `.vue` / `.svelte` /
    //    `.astro` AND CSS extensions; for SFC importers (`from_style = true`)
    //    `./Foo` would otherwise resolve to the SFC itself instead of the
    //    sibling `Foo.scss`. SCSS importers also benefit (defensive against
    //    future extension list changes).
    if let Some(result) = try_css_extension_fallback(ctx, from_file, specifier) {
        return Some(result);
    }
    // 1. Local partial convention: `@use 'variables'` → `_variables.scss`.
    if let Some(result) = try_scss_partial_fallback(ctx, from_file, specifier) {
        return Some(result);
    }
    // 2. Framework-supplied SCSS include paths (Angular's
    //    `stylePreprocessorOptions.includePaths`, Nx equivalent). See #103.
    if let Some(result) = try_scss_include_path_fallback(ctx, from_file, specifier, from_style) {
        return Some(result);
    }
    // 3. `node_modules/` search (Sass's own resolution algorithm):
    //    `@import 'bootstrap/scss/functions'` →
    //    `node_modules/bootstrap/scss/_functions.scss`. Returns
    //    `ResolveResult::NpmPackage` so unused-/unlisted-dependency detection
    //    stays accurate. See #125.
    try_scss_node_modules_fallback(ctx, from_file, specifier, from_style)
}

fn is_style_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "css" | "scss" | "sass"))
}

/// Return `true` when the path's extension is a JS/TS-family runtime extension.
///
/// Used to reject standard-resolver hits when the importer is a stylesheet:
/// Sass's resolution algorithm only ever considers `.css` / `.scss` / `.sass`
/// files, so a sibling `.tsx` / `.ts` / `.js` cannot legally satisfy a Sass
/// `@use` / `@import`. The resolver's extension list mixes JS/TS and CSS,
/// so without this guard `@use 'Widget'` from a `.scss` importer would
/// resolve to a sibling `Widget.tsx` whenever both files exist. See #245.
fn is_js_ts_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext,
                "ts" | "tsx" | "mts" | "cts" | "js" | "jsx" | "mjs" | "cjs"
            )
        })
}

fn is_plain_css_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "css")
}

fn is_bare_style_subpath(specifier: &str) -> bool {
    is_bare_specifier(specifier)
        && specifier.contains('/')
        && Path::new(specifier)
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| {
                ext.eq_ignore_ascii_case("css")
                    || ext.eq_ignore_ascii_case("scss")
                    || ext.eq_ignore_ascii_case("sass")
                    || ext.eq_ignore_ascii_case("less")
            })
}

fn try_css_relative_subpath_fallback(
    ctx: &ResolveContext<'_>,
    from_file: &Path,
    specifier: &str,
    from_style: bool,
) -> Option<ResolveResult> {
    if !is_plain_css_file(from_file) || !is_bare_style_subpath(specifier) {
        return None;
    }

    let relative = format!("./{specifier}");
    match resolve_specifier(ctx, from_file, &relative, from_style) {
        ResolveResult::Unresolvable(_) => None,
        result => Some(result),
    }
}

fn is_node_modules_path(path: &Path) -> bool {
    path.components().any(|component| match component {
        std::path::Component::Normal(segment) => segment == "node_modules",
        _ => false,
    })
}

fn should_preserve_node_modules_style_file(
    specifier: &str,
    from_file: &Path,
    resolved_path: &Path,
) -> bool {
    if !is_style_file(resolved_path) || !is_node_modules_path(resolved_path) {
        return false;
    }

    let is_bare_subpath =
        is_bare_specifier(specifier) && extract_package_name(specifier).as_str() != specifier;
    if is_bare_subpath {
        return true;
    }

    is_node_modules_path(from_file) && (specifier.starts_with('.') || specifier.starts_with('/'))
}

fn try_style_condition_package_resolution(
    ctx: &ResolveContext<'_>,
    from_file: &Path,
    specifier: &str,
    from_style: bool,
) -> Option<ResolveResult> {
    if !is_bare_style_subpath(specifier) || (!from_style && !is_style_file(from_file)) {
        return None;
    }

    let ResolveFileAttempt::Resolved(resolved) = resolve_file_with_resolver_and_tsconfig_fallback(
        ctx,
        ctx.style_resolver,
        from_file,
        specifier,
    ) else {
        return None;
    };
    let resolved_path = resolved.path();

    if let Some(&file_id) = ctx.raw_path_to_id.get(resolved_path) {
        return Some(ResolveResult::InternalModule(file_id));
    }

    if let Some(pkg_name) = extract_package_name_from_node_modules_path(resolved_path)
        && !ctx.workspace_roots.contains_key(pkg_name.as_str())
    {
        return Some(ResolveResult::NpmPackage(pkg_name));
    }

    if let Ok(canonical) = dunce::canonicalize(resolved_path) {
        if let Some(&file_id) = ctx.path_to_id.get(canonical.as_path()) {
            return Some(ResolveResult::InternalModule(file_id));
        }
        if let Some(fallback) = ctx.canonical_fallback
            && let Some(file_id) = fallback.get(&canonical)
        {
            return Some(ResolveResult::InternalModule(file_id));
        }
        if let Some(file_id) = try_source_fallback(&canonical, ctx.path_to_id) {
            return Some(ResolveResult::InternalModule(file_id));
        }
        if let Some(file_id) =
            try_pnpm_workspace_fallback(&canonical, ctx.path_to_id, ctx.workspace_roots)
        {
            return Some(ResolveResult::InternalModule(file_id));
        }
        if let Some(pkg_name) = extract_package_name_from_node_modules_path(&canonical)
            && !ctx.workspace_roots.contains_key(pkg_name.as_str())
        {
            return Some(ResolveResult::NpmPackage(pkg_name));
        }
        return Some(ResolveResult::ExternalFile(canonical));
    }

    extract_package_name_from_node_modules_path(resolved_path)
        .map(ResolveResult::NpmPackage)
        .or_else(|| Some(ResolveResult::ExternalFile(resolved_path.to_path_buf())))
}

/// Resolve a single import specifier to a target.
///
/// `from_style` is `true` for imports extracted from CSS contexts (currently
/// SFC `<style lang="scss">` blocks and `<style src>` references). It enables
/// SCSS partial / include-path / node_modules fallbacks for SFC importers
/// without applying them to JS-context imports from the same file.
#[expect(
    clippy::too_many_lines,
    reason = "central import resolver keeps fallback order visible; style-preservation logic is \
              intentionally local to the resolution decision tree"
)]
pub(super) fn resolve_specifier(
    ctx: &ResolveContext<'_>,
    from_file: &Path,
    specifier: &str,
    from_style: bool,
) -> ResolveResult {
    // URL imports (https://, http://, data:) are valid but can't be resolved locally
    if specifier.contains("://") || specifier.starts_with("data:") {
        return ResolveResult::ExternalFile(PathBuf::from(specifier));
    }

    // Root-relative paths (`/src/main.tsx`, `/static/style.css`) are a web
    // convention meaning "relative to the project/workspace root". Vite,
    // Parcel, and other dev servers resolve them this way. In monorepos, each
    // workspace member has its own Vite root, so `site/index.html` referencing
    // `/src/main.tsx` should resolve to `site/src/main.tsx`, not
    // `<monorepo-root>/src/main.tsx`. Use the source file's parent directory as
    // the base, which is correct for both workspace members and single projects.
    //
    // Applied to web-facing source files: HTML, JSX/TSX, and plain JS/TS. The
    // JSX/TSX case covers SSR frameworks like Hono where JSX templates emit
    // `<link href="/static/style.css" />`: these paths cannot be AST-resolved
    // and have the same web-root semantics as HTML. See issue #105 (till's
    // comment). Applied unconditionally to JS/TS too because the JSX visitor
    // emits `ImportInfo` with the raw attribute value, and the file extension
    // after JSX retry may not reflect the original source (`.js` files with
    // JSX still parse as JSX and get their asset refs recorded here).
    if specifier.starts_with('/')
        && from_file.extension().is_some_and(|e| {
            matches!(
                e.to_str(),
                Some("html" | "jsx" | "tsx" | "js" | "ts" | "mjs" | "cjs" | "mts" | "cts")
            )
        })
    {
        let relative = format!(".{specifier}");
        let source_dir = from_file.parent().unwrap_or(ctx.root);
        if let Ok(resolved) = ctx.resolver.resolve(source_dir, &relative) {
            let resolved_path = resolved.path();
            if let Some(&file_id) = ctx.raw_path_to_id.get(resolved_path) {
                return ResolveResult::InternalModule(file_id);
            }
            if let Ok(canonical) = dunce::canonicalize(resolved_path) {
                if let Some(&file_id) = ctx.path_to_id.get(canonical.as_path()) {
                    return ResolveResult::InternalModule(file_id);
                }
                if let Some(fallback) = ctx.canonical_fallback
                    && let Some(file_id) = fallback.get(&canonical)
                {
                    return ResolveResult::InternalModule(file_id);
                }
            }
        }
        // Fall back to project root for non-workspace setups where the source
        // file may be in a subdirectory (e.g., `public/index.html` referencing
        // `/src/main.tsx`, or a Hono JSX layout in `src/` referencing `/static/style.css`).
        if source_dir != ctx.root
            && let Ok(resolved) = ctx.resolver.resolve(ctx.root, &relative)
        {
            let resolved_path = resolved.path();
            if let Some(&file_id) = ctx.raw_path_to_id.get(resolved_path) {
                return ResolveResult::InternalModule(file_id);
            }
            if let Ok(canonical) = dunce::canonicalize(resolved_path) {
                if let Some(&file_id) = ctx.path_to_id.get(canonical.as_path()) {
                    return ResolveResult::InternalModule(file_id);
                }
                if let Some(fallback) = ctx.canonical_fallback
                    && let Some(file_id) = fallback.get(&canonical)
                {
                    return ResolveResult::InternalModule(file_id);
                }
            }
        }
        return ResolveResult::Unresolvable(specifier.to_string());
    }

    // CSS-context imports (SFC `<style>` blocks) bypass the standard resolver
    // entirely and route through SCSS-aware fallbacks first. The standard
    // resolver's extension list mixes JS / SFC / CSS extensions, so a bare
    // `./Foo` from a `Foo.vue` `<style lang="scss">` block would resolve to
    // `Foo.vue` itself instead of the sibling `Foo.scss`. The SCSS fallback
    // chain restricts probing to `.css` / `.scss` / `.sass` (plus partial /
    // include-path / node_modules conventions), which matches Sass's actual
    // resolution algorithm. See issue #195 (Case B).
    if from_style && let Some(result) = try_scss_fallbacks(ctx, from_file, specifier, true) {
        return result;
    }

    // Bare specifier classification (used for fallback logic below).
    let is_bare = is_bare_specifier(specifier);
    let is_alias = is_path_alias(specifier);
    let matches_plugin_alias = ctx
        .path_aliases
        .iter()
        .any(|(prefix, _)| specifier.starts_with(prefix));

    if let Some(result) =
        try_style_condition_package_resolution(ctx, from_file, specifier, from_style)
    {
        return result;
    }

    // Use resolve_file instead of resolve so that TsconfigDiscovery::Auto works.
    // oxc_resolver's resolve() ignores Auto tsconfig discovery — only resolve_file()
    // walks up from the importing file to find the nearest tsconfig.json and apply
    // its path aliases (e.g., @/ → src/).
    //
    // If resolve_file returns a tsconfig-related error (e.g., a solution-style
    // tsconfig.json references a sibling with a broken `extends` chain), retry with
    // the directory-only `resolve()` form so a broken sibling config does not poison
    // resolution for files covered by a healthy sibling. See issue #97.
    match resolve_file_with_tsconfig_fallback(ctx, from_file, specifier) {
        ResolveFileAttempt::Resolved(resolved) => {
            let resolved_path = resolved.path();
            // Reject JS/TS hits for stylesheet importers. The standard resolver's
            // extension list mixes JS/TS with CSS-family extensions and tries
            // `.tsx` / `.ts` before `.scss` / `.sass` / `.css`, so a `@use 'Widget'`
            // from a `.scss` file would otherwise resolve to a sibling
            // `Widget.tsx` even when `Widget.scss` exists next to it. Sass's
            // actual resolution algorithm only considers stylesheets; redirect
            // to the SCSS-aware fallback chain (CSS-extension probe, partial
            // convention, include paths, node_modules) and short-circuit with
            // `Unresolvable` if those also fail. See issue #245.
            let is_scss_importer = from_file
                .extension()
                .is_some_and(|e| e == "scss" || e == "sass");
            if is_scss_importer && is_js_ts_extension(resolved_path) {
                if let Some(result) = try_scss_fallbacks(ctx, from_file, specifier, from_style) {
                    return result;
                }
                return ResolveResult::Unresolvable(specifier.to_string());
            }
            // Try raw path lookup first (avoids canonicalize syscall in most cases)
            if let Some(&file_id) = ctx.raw_path_to_id.get(resolved_path) {
                return ResolveResult::InternalModule(file_id);
            }

            // Fast path for bare specifiers resolving to node_modules: if the resolved
            // path is in node_modules (but not pnpm's .pnpm virtual store) and the
            // package is not a workspace package, skip the expensive canonicalize()
            // syscall and go directly to NpmPackage. Workspace packages need the full
            // fallback chain (source fallback, pnpm fallback) to map dist→src.
            // Note: the byte pattern check handles Unix and Windows separators separately.
            // Paths with mixed separators fall through to canonicalize() (perf-only cost).
            if is_bare
                && !resolved_path
                    .as_os_str()
                    .as_encoded_bytes()
                    .windows(7)
                    .any(|w| w == b"/.pnpm/" || w == b"\\.pnpm\\")
                && let Some(pkg_name) = extract_package_name_from_node_modules_path(resolved_path)
                && !ctx.workspace_roots.contains_key(pkg_name.as_str())
            {
                return if should_preserve_node_modules_style_file(
                    specifier,
                    from_file,
                    resolved_path,
                ) {
                    ResolveResult::ExternalFile(resolved_path.to_path_buf())
                } else {
                    ResolveResult::NpmPackage(pkg_name)
                };
            }

            // Fall back to canonical path lookup
            match dunce::canonicalize(resolved_path) {
                Ok(canonical) => {
                    if let Some(&file_id) = ctx.path_to_id.get(canonical.as_path()) {
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(fallback) = ctx.canonical_fallback
                        && let Some(file_id) = fallback.get(&canonical)
                    {
                        // Intra-project symlink: raw path differs from canonical path.
                        // The lazy fallback resolves this without upfront bulk canonicalize.
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(file_id) = try_source_fallback(&canonical, ctx.path_to_id) {
                        // Exports map resolved to a built output (e.g., dist/utils.js)
                        // but the source file (e.g., src/utils.ts) is what we track.
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(file_id) =
                        try_pnpm_workspace_fallback(&canonical, ctx.path_to_id, ctx.workspace_roots)
                    {
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(pkg_name) =
                        extract_package_name_from_node_modules_path(&canonical)
                    {
                        // Workspace package resolved through a node_modules symlink to
                        // a built output (e.g. dist/esm/button/index.js) that has no
                        // src/ mirror. Retry against the workspace root's source tree.
                        // See issue #106.
                        if ctx.workspace_roots.contains_key(pkg_name.as_str())
                            && let Some(result) = try_workspace_package_fallback(ctx, specifier)
                        {
                            return result;
                        }
                        if should_preserve_node_modules_style_file(specifier, from_file, &canonical)
                        {
                            ResolveResult::ExternalFile(canonical)
                        } else {
                            ResolveResult::NpmPackage(pkg_name)
                        }
                    } else {
                        ResolveResult::ExternalFile(canonical)
                    }
                }
                Err(_) => {
                    // Path doesn't exist on disk — try source fallback on the raw path
                    if let Some(file_id) = try_source_fallback(resolved_path, ctx.path_to_id) {
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(file_id) = try_pnpm_workspace_fallback(
                        resolved_path,
                        ctx.path_to_id,
                        ctx.workspace_roots,
                    ) {
                        ResolveResult::InternalModule(file_id)
                    } else if let Some(pkg_name) =
                        extract_package_name_from_node_modules_path(resolved_path)
                    {
                        if ctx.workspace_roots.contains_key(pkg_name.as_str())
                            && let Some(result) = try_workspace_package_fallback(ctx, specifier)
                        {
                            return result;
                        }
                        if should_preserve_node_modules_style_file(
                            specifier,
                            from_file,
                            resolved_path,
                        ) {
                            ResolveResult::ExternalFile(resolved_path.to_path_buf())
                        } else {
                            ResolveResult::NpmPackage(pkg_name)
                        }
                    } else {
                        ResolveResult::ExternalFile(resolved_path.to_path_buf())
                    }
                }
            }
        }
        ResolveFileAttempt::Failed {
            used_tsconfig_fallback,
        } => {
            if let Some(result) = try_scss_fallbacks(ctx, from_file, specifier, from_style) {
                return result;
            }

            if used_tsconfig_fallback
                && matches_nearest_tsconfig_path_alias(ctx.root, from_file, specifier)
            {
                // The tsconfig chain was broken, so alias-aware resolution is unavailable.
                // Keep these imports unresolved instead of misclassifying them as npm packages.
                return ResolveResult::Unresolvable(specifier.to_string());
            }

            if is_alias || matches_plugin_alias {
                // Try plugin-provided path aliases before giving up.
                // This covers both built-in alias shapes (`~/`, `@/`, `#foo`) and
                // custom prefixes discovered from framework config files such as
                // `@shared/*` or `$utils/*`.
                // Path aliases that fail resolution are unresolvable, not npm packages.
                // Classifying them as NpmPackage would cause false "unlisted dependency" reports.
                try_path_alias_fallback(ctx, specifier)
                    .unwrap_or_else(|| ResolveResult::Unresolvable(specifier.to_string()))
            } else if let Some(result) =
                try_css_relative_subpath_fallback(ctx, from_file, specifier, from_style)
            {
                result
            } else if is_plain_css_file(from_file) && is_bare_style_subpath(specifier) {
                ResolveResult::Unresolvable(specifier.to_string())
            } else if is_bare && is_valid_package_name(specifier) {
                // Workspace package fallback: self-referencing and cross-workspace
                // imports without node_modules symlinks. Resolves `@org/pkg/sub`
                // against the workspace root's source tree. See issue #106.
                if let Some(result) = try_workspace_package_fallback(ctx, specifier) {
                    return result;
                }
                let pkg_name = extract_package_name(specifier);
                ResolveResult::NpmPackage(pkg_name)
            } else {
                ResolveResult::Unresolvable(specifier.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use oxc_resolver::{JSONError, ResolveError};
    use tempfile::tempdir;

    use super::{
        is_tsconfig_error, matches_nearest_tsconfig_path_alias, path_alias_pattern_matches,
    };

    #[test]
    fn tsconfig_not_found_is_tsconfig_error() {
        assert!(is_tsconfig_error(&ResolveError::TsconfigNotFound(
            PathBuf::from("/nonexistent/tsconfig.json")
        )));
    }

    #[test]
    fn tsconfig_self_reference_is_tsconfig_error() {
        assert!(is_tsconfig_error(&ResolveError::TsconfigSelfReference(
            PathBuf::from("/project/tsconfig.json")
        )));
    }

    // `TsconfigCircularExtend(CircularPathBufs)` is part of the matched set but
    // cannot be directly unit-tested: `CircularPathBufs` is not re-exported from
    // `oxc_resolver::lib`, so external crates cannot construct the variant. The
    // `matches!` arm is structural, so adding the variant to `is_tsconfig_error`
    // is guaranteed by the compiler to return `true` regardless of payload.

    #[test]
    fn io_error_is_tsconfig_error() {
        // An IO error (permission denied while reading a tsconfig) must trigger
        // the fallback. The variant is shared with non-tsconfig IO failures, but
        // the retry via `resolve()` is safe in either case.
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(is_tsconfig_error(&ResolveError::from(io_err)));
    }

    #[test]
    fn json_error_is_tsconfig_error() {
        // Malformed tsconfig JSON surfaces as ResolveError::Json. Same variant
        // covers malformed package.json; retry via `resolve()` is safe there too.
        assert!(is_tsconfig_error(&ResolveError::Json(JSONError {
            path: PathBuf::from("/project/tsconfig.json"),
            message: "unexpected token".to_string(),
            line: 1,
            column: 1,
        })));
    }

    #[test]
    fn module_not_found_is_not_tsconfig_error() {
        // Regular "module not found" must NOT trigger the fallback —
        // the tsconfig loaded fine, the specifier just doesn't exist.
        assert!(!is_tsconfig_error(&ResolveError::NotFound(
            "./missing-module".to_string()
        )));
    }

    #[test]
    fn ignored_is_not_tsconfig_error() {
        assert!(!is_tsconfig_error(&ResolveError::Ignored(PathBuf::from(
            "/ignored"
        ))));
    }

    #[test]
    fn wildcard_tsconfig_path_alias_pattern_matches() {
        assert!(path_alias_pattern_matches("@gen/*", "@gen/foo"));
        assert!(path_alias_pattern_matches("@gen/*", "@gen/nested/foo"));
        assert!(!path_alias_pattern_matches("@gen/*", "@other/foo"));
    }

    #[test]
    fn exact_tsconfig_path_alias_pattern_matches() {
        assert!(path_alias_pattern_matches("$lib", "$lib"));
        assert!(!path_alias_pattern_matches("$lib", "$lib/utils"));
    }

    #[test]
    fn wildcard_only_tsconfig_path_alias_pattern_does_not_match_everything() {
        assert!(!path_alias_pattern_matches("*", "@gen/foo"));
    }

    #[cfg_attr(miri, ignore = "tempdir is blocked by Miri isolation")]
    #[test]
    fn detects_alias_from_nearest_tsconfig_even_when_chain_is_broken() {
        let temp = tempdir().unwrap();
        let project_root = temp.path().join("app");
        let src_dir = project_root.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let source_file = src_dir.join("index.ts");
        fs::write(&source_file, "import '@gen/foo';").unwrap();
        fs::write(
            project_root.join("tsconfig.json"),
            r#"{
                "extends": "./.svelte-kit/tsconfig.json",
                "compilerOptions": {
                    "paths": {
                        "@gen/*": ["../generated/build/ts/*"]
                    }
                }
            }"#,
        )
        .unwrap();

        assert!(matches_nearest_tsconfig_path_alias(
            &project_root,
            &source_file,
            "@gen/foo"
        ));
        assert!(!matches_nearest_tsconfig_path_alias(
            &project_root,
            &source_file,
            "@other/foo"
        ));
    }
}
