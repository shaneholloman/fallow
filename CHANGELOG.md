# Changelog

All notable changes to fallow are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.28.0] - 2026-04-11

### Added

- **Configurable class member allowlist** -- new top-level `usedClassMembers` config field and per-plugin `usedClassMembers` field on external plugins extend the built-in Angular/React lifecycle allowlist with framework-invoked method names. Useful for libraries that call consumer methods reflectively through an interface contract: ag-Grid's `agInit`/`refresh`, TypeORM's `MigrationInterface.up`/`down`, Web Components' `connectedCallback`/`disconnectedCallback`/`attributeChangedCallback`, and any strategy/plugin pattern. The allowlist is merged with plugin contributions during analysis and scoped to class methods and properties, so enum members with matching names are still flagged. ([#98](https://github.com/fallow-rs/fallow/issues/98))
- **Angular `stylePreprocessorOptions.includePaths`** -- the Angular plugin now reads `projects.*.architect.build.options.stylePreprocessorOptions.includePaths` from `angular.json`, and the Nx plugin reads the same key from `project.json`. Bare SCSS `@import 'variables'` / `@use 'mixins'` specifiers that fail file-local resolution retry against each configured include directory using the SCSS partial and directory-index conventions. Eliminates false unresolved-imports and false unused-files for Angular and Nx projects that share SCSS partials through `includePaths`. Parent-relative specifiers (`../shared/vars`) are left untouched so they cannot be silently redirected. ([#103](https://github.com/fallow-rs/fallow/issues/103))

### Fixed

- **Self-referencing and cross-workspace package imports in monorepo libraries** -- when a workspace library uses its own name in import specifiers (a [Node.js v12+ feature](https://nodejs.org/api/packages.html#self-referencing-a-package-using-its-name) commonly used by Angular libraries built with `ng-packagr`), secondary entry point files are no longer reported as unused. A new workspace package fallback strips the matching package name prefix from any bare specifier whose package name is in the workspace registry and resolves the remainder against the library's source tree via `oxc_resolver::resolve_file`, bypassing the `package.json` `exports` map's pointers at compiled output. The fallback also covers cross-workspace imports in monorepos that haven't run `npm install`, where bare `@org/pkg/sub` specifiers would previously fall through to `NpmPackage` classification and create false-positive unused-file and unlisted-dependency reports. ([#106](https://github.com/fallow-rs/fallow/issues/106))
- **SCSS `sass:*` built-in modules no longer reported as unlisted npm dependencies** -- the Sass compiler's [built-in modules](https://sass-lang.com/documentation/modules/) imported via `@use 'sass:string'`, `@use 'sass:math'`, etc. are now recognized alongside the existing `node:`, `bun:`, `cloudflare:`, and Deno `std` platform builtins. These are language-level Sass features, not npm packages, and cannot appear in `package.json`. Previously, every unique `sass:*` module used across a codebase produced a separate false-positive unlisted dependency report (up to 7 per Sass codebase for the full module set: `math`, `string`, `color`, `list`, `map`, `meta`, `selector`). The prefix guard is strict: `sass-loader`, `@types/sass`, and the `sass` compiler package itself remain normal npm packages. ([#104](https://github.com/fallow-rs/fallow/issues/104))
- **Entry points inside ignored output directories now fall back to source index** -- when a `package.json` `main`/`module`/`exports` field resolves to a file inside an ignored output directory (`dist`, `build`, `out`, `esm`, `cjs`) and no direct source mirror exists in `src/`, fallow now probes `src/index.*`, `src/main.*`, `index.*`, `main.*` in the package root. Prevents TypeScript libraries whose compiled entry has no one-to-one source mapping (e.g. `dist/esm2022/index.js` with only `src/index.ts`) from having their entire source tree marked unreachable. Emits a single `tracing::info!` per fallback so the heuristic is visible in logs. ([#102](https://github.com/fallow-rs/fallow/issues/102))

## [2.27.6] - 2026-04-11

### Fixed

- **Bare HTML / Vue / Svelte asset references without `./` prefix** -- HTML `<script src="app.js">`, `<link rel="stylesheet" href="styles.css">`, `<link rel="modulepreload" href="vendor.js">`, and Vue/Svelte `<script src="logic.ts">` now normalize bare filenames to `./`-prefixed relative paths before downstream resolution. Previously these were reported as unlisted npm dependencies because the resolver's specifier classifier treats anything without `./`, `/`, or `://` as a bare package. Browsers, Vite, Parcel, and the Vue/Svelte compilers all resolve these references relative to the document or component file whether or not the `./` prefix is present, so normalization matches real-world semantics. Scoped package paths (`@scope/header.html`), root-absolute paths (`/src/main.ts`), URLs, and `data:` URIs remain untouched. Same bug shape as the Angular `templateUrl` fix shipped in 2.27.5. ([#101](https://github.com/fallow-rs/fallow/pull/101))

## [2.27.5] - 2026-04-11

### Fixed

- **Import-then-reexport pattern recognized as re-export** -- the pattern `import { X } from './a'; export { X };` is now detected as a re-export (semantically equivalent to `export { X } from './a';`) instead of a local export. The visitor scans previously-seen imports when processing an export without a `from` clause and emits a `ReExportInfo` pointing back to the original source. Eliminates false duplicate-export findings (where the bottom export collided with the original) and false unused-export reports (where the re-export chain propagation never reached the misclassified entry). Handles renames on either side, default imports, and mixed local + re-export statements. Order-sensitive: relies on imports preceding exports in source order.
- **Accurate line numbers for unused re-exports** -- re-exports synthesized into `ExportSymbol` entries from `ReExportInfo` were reported with line `:1` because the synthesis used `Span::new(0, 0)` as a sentinel. The visitor's real source span is now plumbed through `ReExportInfo` and `ReExportEdge` into the synthesized `ExportSymbol`, so unused re-exports now report the actual line and column of the `export {...}` declaration. The `(re-export)` marker in human/JSON/SARIF/CodeClimate output is preserved via a semantic lookup against the module's `re_exports` list. On vue-core, 86 unused re-export entries went from `:1` to their real source positions.
- **Angular `templateUrl` / `styleUrl` without `./` prefix** -- bare filenames in Angular `@Component` decorators (e.g., `templateUrl: 'app.component.html'`) are now treated as relative paths instead of being misclassified as npm package specifiers. Angular resolves both `'app.component.html'` and `'./app.component.html'` relative to the component file, so the `./` prefix is optional. Without normalization, bare filenames were reported as unlisted dependencies. Already-relative, absolute, URL, and scoped-package paths are left untouched. ([#99](https://github.com/fallow-rs/fallow/issues/99), [#100](https://github.com/fallow-rs/fallow/pull/100))

### Changed

- **O(N) re-export detection** -- the new `is_re_export` lookup pre-builds an `FxHashSet` of re-exported names per module, replacing a per-export `iter().any()` scan. Avoids O(N²) work on barrel files with many re-exports.

## [2.27.4] - 2026-04-10

### Fixed

- **Broken tsconfig chain no longer poisons sibling resolution** -- a solution-style `tsconfig.json` with `references` to a sibling tsconfig that has a broken `extends` chain (e.g., extending a non-existent file, as commonly scaffolded by Angular CLI / Nx workspaces) no longer fails import resolution for files covered by a healthy sibling. Previously, a broken `tsconfig.spec.json` (covering only `*.spec.ts`) caused every relative import in the workspace to be flagged as unresolved, even in files covered by a valid `tsconfig.app.json`. Fallow now retries resolution without tsconfig context when it detects a tsconfig-loading error (`TsconfigNotFound`, `TsconfigCircularExtend`, `TsconfigSelfReference`, malformed JSON, IO error), and emits a single `tracing::warn!` per unique broken chain so the configuration issue is visible instead of silent. Path aliases in the broken workspace remain unresolvable (they were already broken in `tsc`). ([#97](https://github.com/fallow-rs/fallow/issues/97))
- **VS Code extension packaging** -- the root `LICENSE` file is now copied into `editors/vscode/` at package time so it is bundled in the VSIX. `vsce` only reads the extension's working directory, so the repo-root `LICENSE` was previously not included in the published extension.

### Changed

- **VS Code extension size reduced 48%** -- `.vscodeignore` now excludes `pnpm-lock.yaml`, `.fallow/`, `.test-dist/`, `test/`, `vitest.config.mts`, `tsconfig.test.json`, and `.fallowrc.json`. The packaged VSIX is 100 KB (down from 192 KB).
- **CSS path alias regression tests** -- added coverage for `@/components/Button.css` sharing the `@`-prefix with scoped packages, ensuring path aliases stay as bare specifiers for the resolver's alias fallback to handle.

## [2.27.3] - 2026-04-10

### Fixed

- **CSS scoped package imports** -- `@import` statements referencing scoped npm packages with CSS extensions (e.g., `@fontsource/monaspace-neon/400.css`, `@fallow/design-system/styles.css`) are now kept as bare specifiers for node_modules resolution. Previously, the extension-based heuristic incorrectly prepended `./`, causing all such imports to be reported as unresolved.
- **HTML root-relative paths in workspace members** -- `<script src="/src/main.tsx">` in workspace member HTML files (e.g., `site/index.html`) now resolves relative to the workspace member root, not the monorepo root. Previously, `site/index.html` referencing `/src/main.tsx` looked for `<monorepo>/src/main.tsx` instead of `site/src/main.tsx`, leaving the entry point file and all its transitively reachable modules flagged as unused. Resolution now tries the HTML file's parent directory first, with fallback to the project root for the `public/index.html` pattern.
- **SCSS directory index convention** -- `@use 'components'` now resolves to `components/_index.scss` or `components/index.scss` per SCSS directory-import convention, in addition to the existing `_components.scss` partial fallback.
- **`export default class extends Foo` inheritance** -- default-exported classes with an `extends` clause now participate in the inheritance-aware class member detection introduced in 2.27.2, preventing false unused-member reports on child class overrides.

## [2.27.2] - 2026-04-10

### Fixed

- **SCSS partial resolution** -- `@use 'variables'` and `@import '../styles/variables'` now resolve to `_variables.scss` per SCSS convention. Extensionless bare specifiers in `.scss`/`.sass` files are normalized to relative paths, and the resolver tries the underscore-prefix fallback before npm package classification. Eliminates cascading false positives across unresolved imports, unlisted dependencies, and unused files. ([#95](https://github.com/fallow-rs/fallow/issues/95))
- **Inheritance-aware class member detection** -- child class members accessed via `this.*` in parent class methods are no longer flagged as unused. Builds an inheritance map from `extends` clauses and propagates member accesses bidirectionally through the hierarchy. Reduces false positives from 7 to 1 in typical polymorphic dispatch patterns (the remaining case requires type inference). Also handles `export default class extends Foo` patterns. ([#96](https://github.com/fallow-rs/fallow/issues/96))

## [2.27.1] - 2026-04-10

### Fixed

- **Angular `@defer` block scanning** -- `@defer (when condition)` expressions are now parsed for member references, including `prefetch when` and `hydrate when` variants. Previously, members referenced only in `@defer` conditions were flagged as unused. ([#94](https://github.com/fallow-rs/fallow/issues/94))
- **Angular `@let` template variables** -- `@let name = expr;` template-local variable declarations are now parsed, extracting member references from the expression and tracking the variable as a template local. Previously, members referenced only in `@let` expressions were flagged as unused. ([#93](https://github.com/fallow-rs/fallow/issues/93))
- **Angular `@else if` condition scanning** -- `@else if (condition)` expressions are now parsed for member references. Previously, the condition was skipped and members referenced only in `@else if` were flagged as unused.
- **Angular `outputFromObservable()` recognition** -- class properties initialized with `outputFromObservable()` (from `@angular/core/rxjs-interop`) are now recognized as framework-managed, matching the existing `output()` handling. ([#92](https://github.com/fallow-rs/fallow/issues/92))

## [2.27.0] - 2026-04-10

### Added

- **Feature flag detection** -- new `fallow flags` command detects static feature flags and environment gates in JavaScript/TypeScript code. Three detection categories: boolean flags (`if (FLAG)`), environment checks (`process.env.NODE_ENV`), and config-key guards (`config.featureX`). Cross-references with dead code analysis to surface flags guarding unused code. All 6 output formats supported (human, JSON, compact, SARIF, Markdown, CodeClimate). Available as MCP tool for AI agent integration.
- **Deep Docusaurus convention coverage** -- the Docusaurus plugin now parses `docusaurus.config.*` to discover content plugin instances (docs, blog, pages), classic preset expansion, theme swizzle components, i18n/versioned docs, static directories, client modules, scripts, and stylesheets. Sidebar files are only marked as used when explicitly referenced via `sidebarPath`. Contributed by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza). ([#91](https://github.com/fallow-rs/fallow/pull/91))

### Changed

- **Plugin infrastructure** -- 9 plugins migrated from manual `impl Plugin` to the `define_plugin!` macro, reducing boilerplate. Shared config parser helpers extracted for reuse across plugins (function body extraction for async config creators, property accessors, disabled-section detection).
- **MCP server** -- shared CLI arg-building helpers extracted, reducing duplication across tool implementations.
- **Internal visibility** -- core crate modules restricted to `pub(crate)` where cross-crate access was unnecessary.
- **SFC template parser** -- deduplicated template tag parsing logic shared between Vue and Svelte extractors.

### Fixed

- **VS Code extension** -- version marker round-trip handling improved with additional test coverage.

## [2.26.1] - 2026-04-09

### Changed

- **Health: churn disk cache and parallel git log** -- churn analysis results are now cached to disk and git log calls run in parallel, significantly reducing health command runtime on repos with long histories. New `--performance` flag prints timing breakdown per analysis phase.

### Fixed

- **VS Code extension: stale LSP binary after extension update** -- the version check that detects outdated auto-downloaded binaries silently skipped re-downloading when `--version` output could not be parsed, leaving a stale binary in place. Now writes a `.fallow-version` marker file alongside downloaded binaries and treats unknown-version binaries as stale. Also warns when a PATH-resolved binary is outdated relative to the extension. ([#90](https://github.com/fallow-rs/fallow/issues/90))
- **GitHub Action: multiple JSON objects in bare invocation** -- `analyze.sh` now handles bare invocations that produce multiple JSON objects instead of failing to parse.

## [2.26.0] - 2026-04-09

### Added

- **Expo Router plugin** -- new `expo-router` plugin (85th built-in) detects Expo Router projects, reads the configured route root from `app.json` / `app.config.ts`, and marks route files plus special-file exports as framework-used. Covers `+api` (HTTP methods), `+middleware`, `+html`, `+not-found`, and `+native-intent` conventions. Contributed by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza). ([#88](https://github.com/fallow-rs/fallow/pull/88))
- **TanStack Router config parsing** -- the `tanstack-router` plugin now reads `tsr.config.json` to support custom route directories (`routesDirectory`), route file prefixes (`routeFilePrefix`), ignore patterns (`routeFileIgnorePrefix`, `routeFileIgnorePattern`), generated route tree location (`generatedRouteTree`), and lazy route files (`.lazy.tsx`). Config-driven routes replace static defaults instead of layering. Contributed by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza). ([#88](https://github.com/fallow-rs/fallow/pull/88))
- **Path rule exclusions** -- plugin entry-point and used-export rules now support glob, regex, and segment-regex exclusions for strict file matching. Prevents false negatives from overly broad patterns.

### Fixed

- **GitHub Action: fork PR CI failures** -- the `analyze.sh` script inherited `set -e` (errexit) from the composite action runner shell, causing silent failures on fork PRs where intermediate commands (git diff, grep) return non-zero as part of normal flow.
- **LSP: workspace diagnostic merging** -- `test_only_dependencies`, `boundary_violations`, and `export_usages` are now correctly merged across workspace roots instead of being dropped.

## [2.25.1] - 2026-04-09

### Fixed

- **VS Code extension: duplicate diagnostics in multi-root workspaces** -- diagnostics were shown N times (once per workspace root) when hovering over issues. Fixed by building diagnostics once from the merged results instead of per-root. ([#90](https://github.com/fallow-rs/fallow/issues/90))

## [2.25.0] - 2026-04-09

### Added

- **Health: large function drill-down and duplication penalty** -- `fallow health` now shows the largest functions contributing to unit size risk and applies a score penalty when code duplication is detected. Progressive disclosure hints guide users to relevant flags (`--top`, duplication timing).
- **SIG-aligned clippy lints and module coupling CI** -- added `too_many_lines` (150 LOC), `too_many_arguments` (7 params), and `cognitive_complexity` (25) thresholds aligned with SIG maintainability properties. New CI workflow tracks inter-module coupling metrics.

### Fixed

- **VS Code extension: raw ANSI escape codes in Output panel** -- the LSP server's tracing subscriber now disables ANSI color codes, matching the pattern already used by the MCP server. VS Code's Output panel does not render ANSI sequences. ([#89](https://github.com/fallow-rs/fallow/issues/89))

## [2.24.0] - 2026-04-09

### Added

- **Function size and parameter risk profiles** -- `fallow health` now computes risk distribution histograms for function size (1-15 / 16-30 / 31-60 / >60 LOC) and parameter count (0-2 / 3-4 / 5-6 / >=7 params). Profiles appear in JSON as `unit_size_profile` and `unit_interfacing_profile` on `vital_signs`, and in human output when approaching penalty thresholds.
- **Parameter counting on functions** -- `FunctionComplexity` now includes `param_count` (excluding TypeScript's `this` parameter, including rest parameters). Available in health findings JSON output.
- **Coupling concentration metrics** -- `p95_fan_in` and `coupling_high_pct` added to vital signs, tracking the 95th percentile of incoming dependencies and the percentage of highly-coupled files.
- **Two new health score penalties** -- `unit_size` (max 10 points, activates when >5% of functions exceed 60 LOC) and `coupling` (max 5 points, activates when p95 fan-in exceeds 30). Conservatively calibrated: zero penalty on well-maintained projects.
- **Trend schema version migration warning** -- when comparing against a snapshot from an older schema version, the trend output warns that score deltas may reflect formula changes rather than code changes.

### Changed

- **Conditional risk profile rendering** -- function size profiles only appear in human output when the very-high-risk bin reaches 3% (approaching the 5% penalty threshold). Parameter profiles only appear when high or very-high bins carry signal. Healthy projects see clean, noise-free output.
- **Risk profiles visible during trend view** -- removed suppression of risk profiles when comparing against snapshots, so users can see absolute distributions alongside trend deltas.
- **Snapshot schema v4** -- added risk profiles and coupling fields. Old v3 snapshots load seamlessly via serde defaults.

### Fixed

- **Baseline key portability** -- baseline keys now use relative paths for cross-machine consistency.
- **Zero-match baseline warning** -- improved messaging when baselines match no current issues.
- **Baseline stats in combined JSON** -- unified baseline key helpers across check/health/dupes.

## [2.23.1] - 2026-04-09

### Fixed

- **VS Code extension: explicit `lspPath` setting now takes priority over local `node_modules`** -- the `fallow.lspPath` setting represents explicit user intent and should not be silently overridden by an implicitly discovered devDependency binary. Resolution order is now: `lspPath` > `node_modules/.bin` > PATH > global storage > auto-download.

## [2.23.0] - 2026-04-09

### Added

- **VS Code extension: local devDependency binary resolution** -- the extension now checks `node_modules/.bin/` in the workspace root for locally installed binaries. Teams can pin fallow as a devDependency for consistent versions across developers. ([#86](https://github.com/fallow-rs/fallow/discussions/86))

### Fixed

- **Infinite recursion on TypeScript type annotations in destructuring patterns** -- `collect_pattern_usage` now strips type annotations before traversing, preventing stack overflow on patterns like `({ href, content }: Props)` in Vue and Svelte templates.

## [2.22.4] - 2026-04-08

### Added

- **Nuxt module authoring support** -- projects using `@nuxt/kit` with `src/runtime/` directory now have components, composables, plugins, utils, locale, types, and vue subdirectories recognized as entry points. Tested on Nuxt UI v3: false positive unused files dropped from 521 to 226.

### Fixed

- **Stack overflow on Svelte 5 typed snippet parameters** -- `{#snippet Link({ href, content }: Props)}` caused infinite recursion in `extract_pattern_binding_names` because the TypeScript type annotation prevented destructuring pattern matching. Projects like shadcn-svelte now analyze without crashing.

### Changed

- **Duplicate detection in re-export chain resolution** -- switched from linear scan to `FxHashSet` for O(1) duplicate detection during star re-export propagation.
- **Reduced allocations in plugin system and graph construction** -- reuses scratch buffers and avoids unnecessary `String`/`Vec` allocations.

## [2.22.3] - 2026-04-08

### Fixed

- **aarch64-linux-musl release build failure** -- switched from vendored OpenSSL (`native-tls`) to `rustls` for TLS, eliminating all C dependencies for cross-compilation. Vendored OpenSSL linked against glibc symbols (`__memcpy_chk`) unavailable on musl targets.

## [2.22.2] - 2026-04-08

### Added

- **Meta-framework prepare step warning** -- warns when Nuxt, Next.js, SvelteKit, or Astro need a prepare/build step before analysis can see generated types and routes.

### Fixed

- **CI dupes comment showing "No code duplication found" with `--changed-since` active** -- duplication JSON output had absolute file paths instead of project-relative paths, causing the GitHub Action's changed-file filter to silently drop all clone groups. The same issue existed in combined and audit mode for both dupes and health sections. ([#85](https://github.com/fallow-rs/fallow/issues/85))
- **Star re-exports not propagated through entry point chains** -- `export * from './module'` in barrel files referenced by entry points now correctly marks the re-exported module's exports as used.
- **Wildcard subpath exports missing CSS/asset entry points** -- package.json `exports` maps with wildcards (`"./*": "./dist/*.js"`) now expand to discover CSS, SCSS, and asset entry points.
- **Astro `getStaticPaths` not recognized for TypeScript page files** -- `.ts` files in Astro `src/pages/` now have `getStaticPaths` marked as framework-used.
- **`.js` resolved before `.d.ts` when both exist side-by-side** -- prevents false unresolved imports when a `.js` implementation file coexists with a `.d.ts` declaration.
- **Test framework `include`/`testMatch`/`testRegex` patterns not used as entry point overrides** -- Vitest `test.include` and Jest `testMatch`/`testRegex` config patterns now correctly override the default test file discovery.
- **Case-insensitive HTML/SFC regex matching on non-ASCII** -- added `unicode-case` feature to regex crate for correct case-insensitive matching in HTML and Vue/Svelte template parsing.
- **TLS provider not configured for URL extends resolution** -- explicitly configured native-tls provider for ureq HTTP client, fixing `extends: "https://..."` config resolution.
- **OpenSSL missing on Linux musl/cross-compilation** -- added vendored OpenSSL for static linking on musl targets.

### Changed

- **Binary size reduced by 16.6%** (-1.38MB) via release profile optimizations.
- **Oxc parser upgraded from 0.123 to 0.124.**
- **Sub-packages discovered from package.json exports map** -- nested packages referenced via `exports` field subpath patterns are now resolved.

## [2.22.1] - 2026-04-08

### Fixed

- **Astro frontmatter extraction on Windows** -- the regex for extracting code between `---` delimiters required LF line endings, silently failing on Windows where git checks out CRLF. All `.astro` file exports were invisible on Windows. Now handles both LF and CRLF.

## [2.22.0] - 2026-04-08

### Added

- **Expanded Astro convention coverage** -- actions (`src/actions/index`), content config (`src/content/config`, `src/content.config`), nested middleware (`src/middleware/index`), page exports (`getStaticPaths`, `prerender`, `partial`), and endpoint HTTP method handlers (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `ALL`) are now recognized as framework-used. ([#84](https://github.com/fallow-rs/fallow/pull/84))
- **React Router v7 route config and root module exports** -- `app/routes.ts` default export, root `Layout`, `middleware`, `clientMiddleware`, and `shouldRevalidate` are now kept alive. ([#84](https://github.com/fallow-rs/fallow/pull/84))
- **Remix root and client data exports** -- `clientLoader`, `clientAction`, `shouldRevalidate`, `Layout`, and `HydrateFallback` on root and route modules are now recognized. ([#84](https://github.com/fallow-rs/fallow/pull/84))
- **Gatsby `config` page export and `src/api` function conventions** -- page-level `config` export (Gatsby 5 Slice API) and `src/api/**` serverless function exports (`default`, `config`) are now covered. ([#84](https://github.com/fallow-rs/fallow/pull/84))
- **VitePress `docs/` scaffold discovery** -- `.vitepress` added to hidden directory allowlist, and `docs/.vitepress/config` and `docs/.vitepress/theme/index` patterns added for the common docs-subdirectory layout. Theme entry `default` export is kept alive. ([#84](https://github.com/fallow-rs/fallow/pull/84))

## [2.21.0] - 2026-04-08

### Added

- **Istanbul match-rate stats** -- `istanbul_matched` and `istanbul_total` fields in JSON `summary` when using Istanbul coverage model. Human output footer shows function match count. Helps detect path mismatches or naming gaps in coverage data.
- **Coverage model in snapshots** -- `VitalSignsSnapshot` now persists `coverage_model` (schema v3). Trend output warns with a yellow note when comparing snapshots with different CRAP models, preventing misattribution of score changes.
- **CI auto-detection note** -- when `CI=true` is set and coverage data is auto-detected (not explicit `--coverage`), emits a note on stderr suggesting explicit `--coverage` for deterministic scores.
- **`--coverage-root` flag** -- rebases file paths in Istanbul coverage data by stripping a prefix and prepending the project root. Enables cross-environment coverage matching (CI runner paths vs local checkout). Also available as `coverage_root` MCP parameter.

## [2.20.0] - 2026-04-08

### Added

- **Tiered CRAP scoring with graph-based estimation** -- CRAP scores now use a per-function spectrum instead of per-file binary. Functions whose exports are directly referenced by tests get 85% estimated coverage, indirectly test-reachable functions get 40%, untested files get 0%. The real CRAP formula (CC^2 * (1-cov/100)^3 + CC) is applied with these estimates. Zero config, zero extra latency.
- **Istanbul coverage integration** -- new `--coverage <PATH>` flag (or `FALLOW_COVERAGE` env var) reads `coverage-final.json` from Jest, Vitest, c8, nyc for exact per-function CRAP scores. Auto-detects coverage data from `coverage/coverage-final.json` and `.nyc_output/coverage-final.json`.
- **`oxc_coverage_instrument` dependency** -- uses the published fallow-rs/oxc-coverage-instrument crate for parsing Istanbul-format coverage data.

### Changed

- **Default CRAP model is now `static_estimated`** -- replaces the binary `static_binary` model. The `coverage_model` field in JSON output reflects which model was used: `"static_estimated"` (default) or `"istanbul"` (when coverage data is available).
- **Updated `--explain` CRAP description** -- now documents the tiered estimation model and upgrade path to Istanbul coverage.
- **Updated human output footer** -- file scores footer now describes the estimation model and mentions `--coverage` for exact scores.

## [2.19.3] - 2026-04-08

### Fixed

- **`ignorePatterns` now correctly filters files during discovery** -- user-configured relative glob patterns (e.g. `src/api/generated/**`) were matched against absolute walker paths, so only patterns with `**/` prefix worked. Now strips the project root before matching, consistent with all other glob filters. ([#83](https://github.com/fallow-rs/fallow/issues/83))

## [2.19.2] - 2026-04-08

### Fixed

- **Angular implicit member reference detection** -- `@Component`/`@Directive` metadata extraction now covers inline `template:` strings, `host:` binding expressions, `inputs:`/`outputs:` arrays, and `queries:` metadata. Angular 17+ signal APIs (`input()`, `input.required()`, `output()`, `model()`, `viewChild()`, `viewChildren()`, `contentChild()`, `contentChildren()`) on Angular-decorated classes are treated as framework-managed, preventing false `unused-class-members` reports. ([#82](https://github.com/fallow-rs/fallow/issues/82))

### Changed

- **Module splits for maintainability** -- split `graph/build.rs` narrowing helpers into `graph/narrowing.rs` and `analyze/predicates.rs` into `predicates/{file,import,lifecycle}.rs` for better cohesion. No API changes.
- **Test coverage improvements** -- added 258 tests across 12 files, improving overall line coverage from 90.2% to 90.9%.

## [2.19.1] - 2026-04-07

### Fixed

- **Value/type declaration merging no longer flagged as `duplicate-export`** -- TypeScript allows a const and a type to share the same name (`export const Status = z.enum([...]); export type Status = z.infer<typeof Status>`). This idiomatic pattern (common with Zod, Prisma, and class+interface merging) was falsely reported as a duplicate export. Detection now tracks the type-only flag and skips groups that span both value and type namespaces. ([#81](https://github.com/fallow-rs/fallow/discussions/81))

## [2.19.0] - 2026-04-07

### Added

- **Angular external template tracking** -- `@Component({ templateUrl, styleUrl })` decorators now create graph edges to referenced template and style files, preventing false `unused-files` reports. HTML templates are scanned for Angular syntax (`{{ }}`, `[prop]`, `(event)`, `[(ngModel)]`, `*ngIf`, `*ngFor`, `@if`, `@for`, `@switch`, pipes) and referenced identifiers are bridged to the component's class members, preventing false `unused-class-members` reports. Covers both legacy structural directives and Angular 17+ control flow. ([#73](https://github.com/fallow-rs/fallow/issues/73))

### Fixed

- **Factory initializer instance tracking** -- `const [svc] = useState(() => new MyService())` and similar React hook patterns now correctly resolve the first array-destructured element as a class instance for member access tracking. Prevents false `unused-class-members` when the class is instantiated through a wrapper function. Handles arrow expression bodies, arrow block bodies, and function expressions. ([#79](https://github.com/fallow-rs/fallow/pull/79))
- **VS Code extension binary version skew** -- the auto-downloaded `fallow-lsp` binary in the extension's global storage was never refreshed after initial download, causing LSP diagnostics to diverge from the CLI version. Now checks the installed binary's version on activation and re-downloads when it doesn't match the extension version. ([#80](https://github.com/fallow-rs/fallow/discussions/80))

## [2.18.3] - 2026-04-07

### Fixed

- **`fallow fix` export specifier lists** -- `export { A, B, C }` lines are now fixed correctly by removing individual unused specifiers instead of stripping the `export` keyword (which left invalid `{ A, B, C }` syntax). Handles `export { ... }`, `export { ... } from "..."`, `export type { ... }`, and aliased specifiers. ([#74](https://github.com/fallow-rs/fallow/pull/74))
- **npm publish CI** -- restored two-step npm bootstrap (`npm@10.9.8` then `npm@latest`) for reliable OIDC trusted publishing on Node 22 runners.

## [2.18.2] - 2026-04-07

### Fixed

- **npm publish CI** -- pinned `npm@10` in the release workflow to avoid `promise-retry` module error on Node 22 runners that broke npm package publishing.

## [2.18.1] - 2026-04-07

### Changed

- **Cache serialization switched to bitcode** -- replaced bincode with bitcode for incremental cache serialization, yielding smaller cache files and faster round-trips.
- **Supply chain hardening** -- added OpenSSF Scorecard workflow, zizmor static analysis for GitHub Actions, Dependabot auto-merge for patch/minor dependency bumps, and pinned all CI action SHAs.
- **Miri moved into CI workflow** -- consolidated the standalone Miri workflow into the main CI pipeline for simpler maintenance.

### Fixed

- **Chained member access through class field assignments** -- `this.field.method()` chains where `this.field` is assigned a class instance are now correctly resolved, preventing false positive unused-export reports on the assigned class's methods. ([#63](https://github.com/fallow-rs/fallow/pull/63))
- **Workspace symlink setup idempotent** -- test workspace symlink creation no longer fails when the symlink already exists.

## [2.18.0] - 2026-04-07

### Added

- **`--score`, `--trend`, `--save-snapshot` on combined command** -- the bare `fallow` command (all analyses) now supports health score computation, snapshot saving, and trend comparison. Previously these flags were only available on the `health` subcommand. Enables the health delta header in CI PR/MR comments for combined runs.
- **Health delta header uses metric labels** -- the trend delta line now uses the metric's own label (e.g., "avg complexity" from the trend data) instead of hardcoded text, making the output more robust and descriptive.
- **First-run progressive disclosure** -- when `--score` is enabled but no snapshot exists for trend comparison, the PR comment shows a hint: "Enable `save-snapshot: true` to track score trends over time."
- **Fixture accuracy** -- test fixtures now include `crap_max` and `crap_above_threshold` fields on all `file_scores` entries, matching the actual CLI output schema.

### Fixed

- **GitLab CI `FALLOW_SCORE` and `FALLOW_TREND` wiring** -- both env vars now work for the combined (empty command) case, not just the `health` subcommand.
- **`signed(0)` formatting consistency** -- zero deltas now display as "0.0" instead of "0", consistent with non-zero values like "7.2".
- **GitLab test parity** -- dead export delta assertion (`(-3.8%)`) added to GitLab CI tests, matching the GitHub Action test coverage.
- **CI input descriptions** -- `action.yml` descriptions for `score`, `trend`, and `save-snapshot` now mention bare command support and snapshot storage location.

## [2.17.1] - 2026-04-07

### Added

- **GitHub Action `score` input** -- new `score: true` input enables the `--score` flag on the health command, adding a health delta header to PR summaries showing grade, score change, dead export drift, and complexity trends.
- **Health delta header in CI summaries** -- when `--score` is used, both GitHub Action and GitLab CI summaries display a one-line health score comparison against the previous snapshot.

### Fixed

- **Summary section links** -- all section headers (Code issues, Duplication, Complexity, Codebase health) and table metrics now link to their respective docs pages. Fixes broken markdown link in the Duplication `<summary>` tag.

## [2.17.0] - 2026-04-07

### Added

- **Nuxt runtime convention hardening** -- `plugins/`, `middleware/`, `server/middleware/`, and `components/` directories now have their default exports and named exports (`defineNuxtPlugin`, `defineNuxtRouteMiddleware`, `defineEventHandler`, `defineNuxtComponent`) treated as framework-used. Nested directory scanning for plugins, composables, and utils. Nuxt config `plugins`, `css`, and `modules` entries discovered as runtime entry points with path normalization. ([#60](https://github.com/fallow-rs/fallow/pull/60))
- **Vue SFC edge case hardening** -- `<script src="...">` external script references in Vue SFCs now generate graph edges. `<script>` blocks with `generic="T extends Foo<Bar>"` containing `>` in type parameters are parsed correctly. HTML comments in Vue templates are stripped before import extraction. ([#60](https://github.com/fallow-rs/fallow/pull/60))

### Fixed

- **HTML root-relative path resolution** -- root-relative paths in HTML files (e.g., `<script src="/src/main.tsx">`) are now resolved against the project root instead of being treated as absolute filesystem paths. This fixes false unresolved-import reports in Vite, Parcel, and similar dev server setups. ([#61](https://github.com/fallow-rs/fallow/pull/61))

## [2.16.0] - 2026-04-07

### Added

- **Untested complexity risk (CRAP) scores** -- per-function risk scoring combining cyclomatic complexity with static test reachability from the module graph. Binary model: test-reachable files score CC, untested files score CC^2+CC. Threshold at >=30 (CC>=5 untested), based on the CRAP metric (Savoia & Evans, 2007). Adds `crap_max` and `crap_above_threshold` fields to `FileHealthScore`, `coverage_model: "static_binary"` to `HealthSummary` and `TrendPoint`. Gated behind `--file-scores`. Extreme values (>999) capped in human output; JSON stays uncapped.
- **AddTestCoverage refactoring target** -- new recommendation category (Rule 7) fires when a file has 2+ functions above the CRAP threshold and complexity density > 0.3. Contributing factor `crap_max` appears on targets with high untested risk. Evidence links to the top complex functions.
- **SvelteKit `$types` route group parentheses coverage** -- route groups with parentheses in path segments are now handled correctly for `$types` imports. ([#54](https://github.com/fallow-rs/fallow/pull/54))

### Changed

- **Lazy complexity computation** -- cyclomatic/cognitive complexity is now computed on-demand rather than eagerly, reducing parse time for pipelines that don't need it.
- **Packed ModuleNode flags** -- boolean flags on `ModuleNode` packed into a bitfield for tighter memory layout.
- **Smarter BFS reachability** -- reachability analysis uses optimized graph traversal.
- **Duplicate detection improvements** -- faster clone detection with reduced memory allocation.

### Fixed

- **Windows path separator normalization** -- health coverage-gaps tests now normalize backslashes for Windows CI compatibility.
- **GitHub Action fork PR handling** -- PR comment tests are skipped on fork PRs where write permissions are unavailable.

## [2.15.0] - 2026-04-06

### Added

- **Next.js convention export tracking** -- App Router special files (`loading`, `error`, `not-found`, `template`, `default`, `global-error`, `forbidden`, `unauthorized`, `global-not-found`) and route segment config exports (`revalidate`, `dynamic`, `runtime`, `fetchCache`, `preferredRegion`, `maxDuration`, `viewport`, `generateViewport`) are now treated as framework-used. Pages Router `_app` (`reportWebVitals`), API routes (`config`), `middleware`, `proxy`, `instrumentation` (`register`, `onRequestError`), `instrumentation-client` (`onRouterTransitionStart`), and `mdx-components` (`useMDXComponents`) are also covered. Entry-point files still report genuinely unused helper exports alongside framework-used ones. ([#59](https://github.com/fallow-rs/fallow/pull/59) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Next.js `transpilePackages` support** -- packages listed in `next.config.{ts,js}` `transpilePackages` are treated as referenced dependencies, preventing false unused-dependency reports. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Config-defined alias resolution for Vite, Nuxt, and SvelteKit** -- aliases defined in `vite.config.ts` (`resolve.alias`), `nuxt.config.ts` (`alias`), and `svelte.config.ts` (`kit.alias`) are extracted at analysis time and fed into the resolver as fallbacks. Supports object form, array form, `fileURLToPath(new URL(...))`, `path.resolve(__dirname, ...)`, and `path.join(...)` patterns. Eliminates false unresolved-import, unlisted-dependency, and unused-file reports for config-aliased paths. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Nuxt `srcDir` support** -- when `srcDir` is set in `nuxt.config.ts`, fallow remaps `~/` and `@/` aliases to the configured source directory, adds entry patterns and always-used files under that directory, and resolves `imports.dirs` and `components` paths relative to srcDir. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Nuxt custom directories** -- `imports.dirs`, `components` (string arrays and `{ path: "..." }` objects), and `components.dirs` from `nuxt.config.ts` are now discovered as auto-import roots and component directories. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Nuxt `@/` and `@@/` alias recognition** -- `@/` (srcDir synonym for `~/`) and `@@/` (rootDir synonym for `~~/`) are now recognized as path aliases in both static analysis and resolver fallback. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **SvelteKit param matcher support** -- files in `src/params/**/*.{ts,js}` have their `match` export treated as framework-used. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Vue component tag tracking** -- imported components used as template tags (`<FancyCard />`, `<fancy-card />`, `<Form.Input />`) are now credited as used. Supports PascalCase, kebab-case-to-PascalCase conversion, and namespace member access. Built-in components (`Transition`, `KeepAlive`, `Teleport`, `Suspense`, `Slot`) are excluded. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Vue custom directive tracking** -- custom directives like `v-focus-trap` are mapped to their imported binding name (`vFocusTrap`) and credited as used. Built-in directives (`v-if`, `v-for`, `v-model`, etc.) are excluded. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Vue `v-on` and `v-bind` object syntax** -- `v-on="handlers"` and `v-bind="attrs"` now credit the bound expression as used. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Svelte component tag tracking** -- imported components used as Svelte markup tags (`<FancyButton />`, `<Icons.Alert />`) are credited as used, including namespace member access. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Svelte directive and attribute tracking** -- `use:tooltip`, `transition:fade`, `animate:flip`, `in:fly`, `out:slide` directives credit their imported action/transition binding as used. Attribute value expressions (`class:active={isActive}`) and shorthand attributes (`{page}`) are also tracked. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Svelte `$store` subscription tracking** -- `$page.url.pathname` in Svelte templates now credits the `page` import as used, with `url` tracked as a member access. ([#59](https://github.com/fallow-rs/fallow/pull/59))

### Fixed

- **Svelte template scanner index advancement** -- the HTML tag scanner was advancing by only 1 byte instead of jumping to the end of the scanned tag, causing every HTML tag to be re-parsed from its second byte. ([#59](https://github.com/fallow-rs/fallow/pull/59))
- **Panic on malformed Svelte brace attributes** -- `parse_markup_tag` used `.expect()` on `scan_curly_section` which could panic on unterminated brace expressions in Svelte templates. Replaced with graceful fallback. ([#59](https://github.com/fallow-rs/fallow/pull/59))

## [2.14.2] - 2026-04-06

### Added

- **Svelte/Vue template-visible import tracking** -- imports used only in SFC template markup (e.g., `{formatDate(x)}` in Svelte, `{{ utils.format() }}` in Vue) are now credited as used, preventing false unused-export/import reports. Namespace member access in templates (e.g., `utils.formatDate`) is tracked as member usage. Vue credits only `<script setup>` bindings; Svelte excludes `context="module"` scripts. ([#58](https://github.com/fallow-rs/fallow/pull/58) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Type-only circular dependency filtering** -- `import type` edges are excluded from cycle detection since they are erased at compile time and cannot cause runtime cycles. ([#54](https://github.com/fallow-rs/fallow/issues/54))
- **Duplicate export common-importer filter** -- duplicate exports are only reported when the files sharing the same export name also share a common importer. Unrelated leaf files (e.g., SvelteKit route modules in different directories) are no longer flagged. ([#54](https://github.com/fallow-rs/fallow/issues/54))

### Fixed

- **Workspace plugin merge: generated imports and path aliases** -- `generated_import_patterns` (e.g., SvelteKit `$types`) and `path_aliases` (e.g., SvelteKit `$lib/`) from workspace-level plugins were not propagated to the root analysis, causing false-positive unresolved imports and resolution failures in monorepo setups. ([#54](https://github.com/fallow-rs/fallow/issues/54))
- **Windows path prefix** -- replaced `std::fs::canonicalize()` with `dunce::canonicalize()` to avoid `\\?\` extended-length path prefix on Windows, which broke `oxc_resolver` tsconfig discovery and caused all path-aliased imports to be reported as unresolved. ([#55](https://github.com/fallow-rs/fallow/pull/55) by [@KamilDev](https://github.com/KamilDev))

## [2.14.1] - 2026-04-06

### Added

- **HTML entry file parsing** -- when an HTML file is reachable (e.g., Vite's `index.html` entry point), fallow now parses `<script src>`, `<link rel="stylesheet" href>`, and `<link rel="modulepreload" href>` to create graph edges to referenced local assets. This prevents false-positive dead-code reports for JS/CSS files and their transitive imports in Vite/Parcel-style apps. HTML files are exempt from unused-file detection. ([#57](https://github.com/fallow-rs/fallow/issues/57))
- **Parcel `index.html` entry pattern** -- the Parcel plugin now auto-detects `index.html` as a runtime entry point, matching the Vite plugin's behavior.

### Fixed

- **Coverage gaps inline suppression** -- `coverage-gaps` issue kind can now be suppressed with `// fallow-ignore-next-line coverage-gaps` comments.

## [2.14.0] - 2026-04-06

### Added

- **Static test coverage gaps** (`fallow health --coverage-gaps`) -- reports runtime files and exports that no test dependency path reaches through the module graph. Splits entry points into runtime, test, and support roles across 84+ framework plugins. All 6 output formats supported. ([#53](https://github.com/fallow-rs/fallow/pull/53) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Coverage gaps severity control** -- `coverage-gaps` rule in config (`error`/`warn`/`off`, default `off`). When set to `error`, non-zero exit on any gap. When `warn`, reports gaps without failing CI.
- **Coverage gap actions** -- JSON output includes `add-tests` and `add-test-import` actions for AI agents on untested files and exports.
- **Entry point roles for external plugins** -- `entryPointRole` field (`runtime`/`test`/`support`) on external plugin definitions, allowing custom frameworks to declare how their entry points affect coverage reachability.

### Fixed

- **Workspace entry point fallback scoping** -- `discover_workspace_entry_points` fallback now correctly scopes to the workspace root instead of searching the entire project.
- **External plugin default role** -- external plugin `entryPointRole` now defaults to `support` (matching unknown builtins) instead of `runtime`.

## [2.13.4] - 2026-04-06

### Fixed

- **False positive unused exports from namespace exports** -- `export namespace Foo { export function bar() {} }` no longer reports inner declarations (`bar`) as unused top-level exports. Inner exports are now tracked as namespace members. Runtime namespaces (no `declare`) are correctly classified as non-type-only. ([#52](https://github.com/fallow-rs/fallow/issues/52))

## [2.13.3] - 2026-04-05

### Changed

- **Human output readability** -- all abbreviations spelled out in user-facing output: "deps" → "dependencies", "MI" → "maintainability", "dep" → "dependency". Affects health vital signs, dead-code summary footer, combined orientation header, markdown tables, and score deductions.
- **Section headers in dead-code output** -- human format now groups findings under `── Unused Code ──`, `── Dependencies ──`, and `── Structure ──` headers for faster scanning.
- **Labeled metrics and deductions** -- health score deductions line now prefixed with "Deductions:", metrics lines prefixed with "Metrics:" across health, combined, and audit commands.

### Fixed

- **Boundaries excluded from default `fallow list`** -- `fallow list` no longer shows "Boundaries: not configured" noise; use `--boundaries` to inspect zones and rules. ([#49](https://github.com/fallow-rs/fallow/pull/49) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Ecosystem runner error handling** -- `install_deps()` no longer swallows failures via `|| true`, and stderr is separated from JSON output files. Also uses canonical `dead-code` command with `--quiet`. ([#48](https://github.com/fallow-rs/fallow/pull/48) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Audit vital signs labeled** -- the audit command's vital signs line now includes a "Metrics:" prefix, consistent with health and combined commands.
- **Stale `fallow check` in scripts** -- replaced legacy command name in `bench-ci.sh` and `conformance/run.sh`.

## [2.13.2] - 2026-04-05

### Fixed

- **Plugin entry points included in `--entry-points` mode** -- `fallow list --entry-points` now includes plugin-discovered entry points, matching the behavior of the default `fallow list` output. Previously, plugin detection was skipped when `--entry-points` was used without `--plugins`. ([#45](https://github.com/fallow-rs/fallow/pull/45) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **`Failed` summary line uses canonical command name** -- the summary line printed on analysis failure now says `fallow dead-code` instead of the legacy `fallow check`.
- **Documentation `cargo run` examples** -- all `cargo run` examples in CONTRIBUTING.md and CLAUDE.md now include `--bin fallow` (required for multi-binary workspaces) and use `dead-code` instead of the legacy `check` command. ([#44](https://github.com/fallow-rs/fallow/pull/44) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))

## [2.13.1] - 2026-04-05

### Fixed

- **Init hook uses canonical command name** -- `fallow init --hooks` now generates hooks with `fallow dead-code` instead of the legacy `fallow check` alias, and the internal `--base` field is renamed to `--branch` to match the CLI flag. ([#43](https://github.com/fallow-rs/fallow/pull/43) by [@M-Hassan-Raza](https://github.com/M-Hassan-Raza))
- **Legacy command name cleanup** -- replaced `fallow check` with `fallow dead-code` in 4 user-facing messages: combined output suggestion, GitHub Action PR review body, VS Code extension diagram, and conformance test script.
- **Documentation consistency** -- fixed stale `--base` references to `--branch` in `AGENTS.md`, `docs/backwards-compatibility.md`, and companion repos.

## [2.13.0] - 2026-04-04

### Added

- **Bun built-in module support** -- `bun:sqlite`, `bun:test`, `bun:ffi`, and other `bun:` prefixed imports are now recognized as platform builtins and never flagged as unlisted dependencies. ([#40](https://github.com/fallow-rs/fallow/issues/40))
- **`ignoreDependencies` suppresses unlisted warnings** -- dependencies listed in `ignoreDependencies` are now excluded from both unused dependency AND unlisted dependency detection. Useful for runtime-provided packages like `bun:sqlite` or globally available dependencies. ([#40](https://github.com/fallow-rs/fallow/issues/40))
- **MCP server distributed via npm** -- `fallow-mcp` binary is now included in the npm package. After `npm install fallow`, the `fallow-mcp` command is available for MCP server integration with Claude, OpenCode, and other AI agents. ([#42](https://github.com/fallow-rs/fallow/issues/42))

### Fixed

- **`$schema` accepted in `.fallowrc.json`** -- the JSON schema now includes `$schema` as a valid property, so JSON editors no longer show "unknown key" warnings when using the schema reference. ([#39](https://github.com/fallow-rs/fallow/issues/39))
- **VS Code extension LSP download on Windows** -- the release workflow now names LSP binaries with the correct platform identifier (e.g., `fallow-lsp-win32-x64-msvc.exe`), matching what the VS Code extension expects. Previously, Windows users saw "no LSP binary found" errors. ([#38](https://github.com/fallow-rs/fallow/issues/38))

## [2.12.1] - 2026-04-04

### Fixed

- **Tab indentation preserved in export auto-fix** -- `fallow fix` no longer silently converts tab indentation to spaces when removing the `export` keyword. The original whitespace prefix is now preserved exactly. ([#36](https://github.com/fallow-rs/fallow/issues/36), [#37](https://github.com/fallow-rs/fallow/pull/37) by [@swalha1999](https://github.com/swalha1999))

## [2.12.0] - 2026-04-03

### Added

- **Vital signs percentage referents** -- the orientation header now shows denominators: `dead files 0.6% (1 of 173)` instead of just `0.6%`. Raw counts are also exposed in the health JSON `vital_signs.counts` object for CI dashboards.
- **Entry-point detection inline** -- combined and standalone check modes display `130 entry points detected (124 plugin, 6 package.json)` on stderr, with a yellow warning when zero entry points are found. Entry-point summary is also in the check JSON `entry_points` object.
- **Baseline-aware deltas** -- when `--baseline` is active, the `Failed:` summary line shows `+N since baseline` or `-N since baseline`. A `baseline_deltas` object with per-category deltas is added to the check JSON output.
- **`--summary` flag** -- global flag that shows only category counts without individual items. Works across check (severity-colored counts), dupes (families, groups, lines, rate), and health (functions analyzed, MI, score). JSON output always includes the full `summary` counts object regardless of this flag.
- **`--effort` filter** -- `fallow health --effort low|medium|high` filters refactoring targets by estimated effort level.
- **`--group-by package`** -- groups dead-code findings by workspace package in monorepos. Discovers workspaces automatically from `package.json`/`pnpm-workspace.yaml`.
- **`publicPackages` config** -- workspace package name patterns (exact or glob) whose exports are treated as public API and excluded from unused-export detection.
- **`dynamicallyLoaded` config** -- glob patterns for files loaded at runtime (plugin directories, locale files) that are treated as always-used entry points.
- **`fixture_glob_patterns()` Plugin trait method** -- plugins can now declare test fixture patterns that are implicitly used. Added for Jest, Vitest, and Playwright.
- **Cross-package circular dependency flag** -- `CircularDependency.is_cross_package` indicates cycles crossing workspace boundaries. Shown as `(cross-package)` in human output, present in JSON/SARIF/compact/markdown/CodeClimate.
- **Mirrored directories in JSON** -- `DuplicationReport.mirrored_directories` array with `dir_a`, `dir_b`, `shared_files`, and `total_lines` for CI consumption.
- **Smarter `fallow init`** -- detects project structure (TypeScript, monorepo tool, test framework, UI framework, Storybook) and generates a tailored `.fallowrc.json` with workspace patterns, ignore rules, and entry points.
- **Undeclared workspace diagnostic** -- warns when directories with `package.json` exist but aren't declared in workspace patterns.
- **Second-level directory rollup** -- when one directory holds >80% of unused files, the rollup automatically shows per-subdirectory breakdown (e.g., `packages/svelte/ 4463 files` instead of just `packages/ 4463`).
- **Test-only dependencies in CI summaries** -- `test_only_dependencies` category now appears in GitHub Action and GitLab CI PR summary comments.

### Changed

- **MI legend gated** -- the `MI scale: good ≥85, moderate ≥65, low <65 (0–100)` legend only appears when the average MI is below 85 (moderate or low). Projects with good health scores no longer see it.
- **Config quality note threshold** -- raised from 50% to 80%, reducing noise for projects with moderate test directory concentration.
- **Plugin discovery hint** -- unresolved imports footer now mentions framework plugins: "Framework-specific imports may need a plugin".
- **Scale-aware start-here nudge** -- when total issues exceed 500, the nudge suggests `--workspace <name>` instead of pointing to a specific file.
- **Summary footer filtered counts** -- the `✗ N files · N exports` summary line now reflects visible counts after export suppression, not raw totals.
- **Advisory note stream consistency** -- all advisory notes now consistently use stderr.
- **Rollup suppress hint** -- directory rollup sections suggest `ignorePatterns` in config instead of inline `fallow-ignore-next-line` comments.

### Fixed

- **Unlisted deps false positives** -- shell variables (`$DIR`), pure numbers (`1`), and bundler-internal specifiers (`__barrel_optimize__?...`) are no longer classified as npm package names. Reduces next.js false positives from 761 to 753.
- **Start-here noise filter** -- the "start with X" recommendation no longer points to test fixtures, playground files, or generated files (e.g., `a0.js`). When all targets are noise, the nudge is omitted entirely.
- **Nested node_modules exclusion** -- workspace glob expansion now skips directories inside `node_modules`, preventing third-party package.json files from being analyzed as workspace packages.
- **Init config serialization** -- changed `unwrap_or_else` with silent `{}` fallback to `expect()` for infallible JSON serialization.

## [2.11.0] - 2026-04-03

### Added

- **`--group-by owner|directory`** -- new global flag that groups all dead-code analysis output by team ownership (CODEOWNERS) or by first directory component. CODEOWNERS parser with auto-probe (`.github/CODEOWNERS`, `.gitlab/CODEOWNERS`, `docs/CODEOWNERS`), last-match-wins pattern matching, first-owner-on-line selection. All 6 output formats supported: human (colored group headers with summary line, per-type breakdown, matching rule annotations), JSON (grouped envelope with `groups` array), compact (group prefix per line), markdown (section headers per group), SARIF (`properties.owner`), CodeClimate (`owner` field).
- **`codeowners` config field** -- optional path to a non-standard CODEOWNERS file in `.fallowrc.json` / `fallow.toml`.
- **MCP `group_by` parameter** -- the `analyze` tool now accepts `group_by: "owner" | "directory"` to produce grouped JSON output.

### Changed

- **GitHub Action review comments** -- now filtered to PR diff hunks using `--slurpfile` for large PRs, preventing `ARG_MAX` crashes on PRs with 50+ changed files.
- **GitHub Action review UX** -- improved hunk filtering to tolerate whitespace-only and context lines, with better fallback behavior when no hunk matches.

## [2.10.1] - 2026-04-03

### Changed

- **PR comment vital signs clarity** -- the metrics table in GitHub Action and GitLab CI PR comments now has a "Codebase health" section header, making it clear that metrics are project-wide rather than PR-scoped. The `dead_export_pct` metric is removed from PR comments (still available in CLI JSON output) since it's a graph-level property not actionable from a single PR.
- **Scoped maintainability row** -- when `--changed-since` is active, PR comments now show a "Maintainability (changed files)" row alongside the codebase-wide score, so developers can see how their changes compare to the project baseline.
- **Clickable commit hash** -- the "Scoped to files changed since" footer now links the commit hash to the commit on GitHub/GitLab instead of showing plain text.
- **Disambiguated scoping footer** -- the footer now reads "Issue counts scoped to files changed since ... · health metrics reflect the full codebase" to eliminate ambiguity about what is PR-scoped vs codebase-wide.

## [2.10.0] - 2026-04-03

### Added

- **HTTPS URL extends** -- the `extends` field in `.fallowrc.json` now supports `https://` URLs alongside relative paths and `npm:` packages. Fetch remote shared configs without publishing an npm package. HTTPS-only, 5s default timeout (configurable via `FALLOW_EXTENDS_TIMEOUT_SECS`), 1 MB body limit, URL normalization for cycle detection. URL-sourced configs may extend other URLs or `npm:` packages but not relative paths.
- **GitHub Action shallow clone resilience** -- PR comment, review, annotation, and summary scripts now detect shallow clones and fall back gracefully instead of failing.

## [2.9.3] - 2026-04-03

### Fixed

- **GitHub Action Marketplace** -- shortened action description to meet the 125-character limit required for Marketplace publication.

## [2.9.2] - 2026-04-02

### Added

- **npm package resolution for `extends`** -- config files can now extend shared configs from npm packages using the `npm:` prefix (e.g., `"extends": "npm:@company/fallow-config"`). Resolution walks up `node_modules/`, checks `package.json` `exports`/`main`, and falls back to standard config file names. Subpaths supported (e.g., `npm:@co/config/strict.json`).
- **MCP server hardening** -- improved parameter validation, tool descriptions, and error messages for better AI agent integration.

### Internal

- Path confinement (`resolve_confined`) prevents traversal attacks via npm subpaths or malicious `package.json` exports/main fields.
- Package name validation rejects `..`/`.` components and bare `@scope` without name.

## [2.9.1] - 2026-04-02

### Fixed

- **Deterministic output ordering** -- all result arrays are now sorted after parallel collection, ensuring identical JSON/SARIF/CodeClimate output across runs. Previously, rayon parallelism caused item ordering to vary between invocations.
- **Symlink canonical index warning** -- downgraded noisy symlink resolution warning to debug level.
- **GitHub Action scoping** -- PR comments and annotations now correctly scope to changed files when `--changed-since` is active.

### Changed

- **Performance** -- `OutputFormat`, `MemberKind`, and `DupesMode` enums now derive `Copy`, eliminating unnecessary clones and reference indirection across 20 CLI files.

### Internal

- Split 5 large modules into focused submodules: `regression.rs` (5 modules), `diagnostics.rs` (4 modules), `health_types.rs` (5 modules), plus 3 monolith test files into 30 modules.
- Extracted helper functions from 4 long CLI command functions (1,001 → 365 lines).
- Added 103 unit tests to the config crate (287 → 390 tests).
- Added module-level documentation to all `graph/resolve/` submodules.

## [2.9.0] - 2026-04-01

### Added

- **Architecture boundary violations** -- define zones (glob patterns mapping directories to architecture layers) and rules (which zones may import from which). Violations are detected at the import site using the resolved target's zone. Reported in all 6 output formats, with inline and file-level suppression via `// fallow-ignore-next-line boundary-violation`. LSP diagnostics, code actions, GitHub Action annotations, GitLab CI review comments, and MCP server integration all included.
- **Built-in boundary presets** -- `"preset": "layered"` (4 zones: presentation/application/domain/infrastructure), `"preset": "hexagonal"` (3 zones: adapters/ports/domain), `"preset": "feature-sliced"` (6 zones: app/pages/widgets/features/entities/shared), `"preset": "bulletproof"` (4 zones: app/features/shared/server). Presets auto-detect `rootDir` from `tsconfig.json` for pattern prefixes. User zones and rules merge on top of presets.
- **`fallow list --boundaries`** -- inspect expanded boundary zones, rules, and per-zone file counts. Supports human and JSON output formats.
- **`--boundary-violations` filter** -- show only boundary violations in `fallow dead-code` output.

### Fixed

- **Rest patterns in destructured exports** -- `export const { a, ...rest } = obj` no longer causes a parser error.

### Changed

- **oxc dependency upgrade** -- bumped all 7 oxc crates to latest versions.

## [2.8.1] - 2026-04-01

### Fixed

- **`fallow init` no longer panics** -- the global `--base` alias for `--changed-since` collided with Init's own `--base` flag, causing a runtime panic on every `fallow init` invocation. Init's flag is now `--branch`.

### Added

- **CLI integration test infrastructure** -- shared test harness for all CLI commands with 58 new tests covering `check`, `health`, `dupes`, `init`, exit codes, baselines, and MCP end-to-end. 7 new test fixtures (astro, mdx, complexity, config-file, config-toml, hidden-dir-allowlist, error-no-package-json). Human output snapshots for check, health, and dupes commands.

## [2.8.0] - 2026-04-01

### Added

- **`fallow audit` command** -- combined dead-code + complexity + duplication analysis scoped to changed files, returning a verdict (pass/warn/fail). Purpose-built for reviewing AI-generated code and PR quality gates. Auto-detects the base branch if `--base` is not specified. JSON output includes `verdict`, per-category `summary`, and full sub-results with `actions` arrays. All 6 output formats supported. MCP `audit` tool wraps the CLI.
- **`--base` global alias** -- `--base` is now a global alias for `--changed-since` on all commands. More intuitive for PR review workflows.
- **`.fallow/` added to `.gitignore` during `fallow init`** -- prevents snapshot and cache directories from being committed.

### Changed

- **Release binary uses `panic=abort`** -- smaller binary size by removing unwind tables.
- **Rust 2024 formatting style** -- `.rustfmt.toml` with `style_edition = "2024"`.
- **Stricter lint discipline** -- 6 restriction lints added, `#[expect]` required for all lint suppressions, `tail_expr_drop_order` compiler lint enabled.

## [2.7.3] - 2026-03-31

### Fixed

- **`--format badge` now auto-enables `--score`** -- previously `fallow health --complexity --format badge` would error because score computation wasn't triggered when explicit section flags were passed. Badge format now implies `--score`, matching the behavior of `--min-score`, `--trend`, and `--save-snapshot`.

## [2.7.2] - 2026-03-31

### Added

- **Badge generation** (`--format badge`) -- generates a self-contained shields.io-compatible flat SVG badge showing the health score and letter grade. Grade-first message format (`B (76)`) matches quality dashboard conventions. Color-coded by grade: A=brightgreen, B=green, C=yellow, D=orange, F=red. Unique SVG element IDs prevent collisions when multiple badges are inlined on one page. Also available via `FALLOW_FORMAT=badge` environment variable.

## [2.7.1] - 2026-03-30

### Fixed

- **SvelteKit `$app` and `$env` no longer reported as unlisted dependencies** -- virtual module prefix matching failed when the extracted package name (e.g., `$app`) was compared against a prefix with a trailing slash (`$app/`). Also fixes the same latent bug for Docusaurus virtual prefixes (`@theme/`, `@docusaurus/`, etc.)
- **SvelteKit `./$types` imports no longer reported as unresolved** -- added `generated_import_patterns()` to the Plugin trait so frameworks can declare build-time generated import suffixes. SvelteKit uses this to suppress `./$types`, `./$types.js`, and `./$types.ts` route type imports that are generated at build time and don't exist on disk during static analysis.

## [2.7.0] - 2026-03-30

### Added

- **Structured fix suggestions in JSON output** -- every issue in `--format json` output now includes an `actions` array with machine-actionable fix and suppress hints. 14 fix action types (kebab-case), `auto_fixable` flag on every action, optional `note` for non-auto-fixable items. Dependency issues use `add-to-config` suppress with concrete package name. Re-export findings include a warning note about public API surface. `duplicate_exports` suppress includes `scope: "per-location"`. No schema version bump (additive change).
- **Pre-commit hook setup** (`fallow init --hooks`) -- scaffolds a git pre-commit hook that runs `fallow check --changed-since` on changed files. Auto-detects base branch via git. Supports `--base <ref>` override. Detects husky, lefthook, and bare `.git/hooks`. Includes binary guard and helpful success message with `--no-verify` bypass. Input validated to prevent shell injection.

## [2.6.0] - 2026-03-30

### Added

- **Trend reporting** (`fallow health --trend`) -- compare current health metrics against the most recent saved snapshot and show per-metric deltas with directional indicators (improving/declining/stable). Tracks 8 metrics: health score, dead files, dead exports, avg cyclomatic complexity, maintainability, unused deps, circular deps, and hotspots. Implies `--score`. Reads from `.fallow/snapshots/` (saved via `--save-snapshot`). Output in all formats: human (colored arrows with all-stable collapse), JSON (structured `health_trend` object with raw counts), compact (grep-friendly `trend:overall:direction=` + per-metric lines), and markdown (table for PR comments). Warns when combined with `--changed-since`. Wired through MCP server and GitHub Action.

## [2.5.5] - 2026-03-28

### Fixed

- **Stale cache caused type-level enum detection to not take effect** -- bumped cache version to invalidate entries from v2.5.3/v2.5.4 that were missing the new type-level `whole_object_uses` extraction data

## [2.5.4] - 2026-03-28

### Fixed

- **`Record<Enum, T>` now marks all enum members as used** -- `Record<Status, string>` and nested variants like `Partial<Record<Status, number>>` are now recognized as whole-object enum usage, preventing false unused member reports
- **`keyof typeof Enum` in mapped types now marks all members as used** -- `{ [K in keyof typeof Direction]: string }` is now detected as a whole-object use pattern
- **Relaxed `@types/` unlisted dependency check** -- the type-only import check for `@types/<package>` now works regardless of the import's `type` modifier style

## [2.5.3] - 2026-03-28

### Fixed

- **Type-only imports with `@types/` no longer flagged as unlisted** -- `import type { Feature } from 'geojson'` is no longer reported as an unlisted dependency when `@types/geojson` is in devDependencies. TypeScript erases type-only imports at compile time, so the bare package doesn't need to be installed. Scoped packages are handled via the DefinitelyTyped convention (`@scope/pkg` → `@types/scope__pkg`). Value imports still require the real package.
- **Unused enum member false positives in type-level usage** -- enum members used via TypeScript qualified names in type position (e.g., `type X = Status.Active`) are now detected as used. Enums used as mapped type constraints (e.g., `{ [K in Direction]: string }`) mark all members as used.

## [2.5.2] - 2026-03-28

### Fixed

- **Vitest reporter false positives** -- packages referenced as strings in `test.reporters` (e.g., `vitest-sonar-reporter`) are now detected as used dependencies
- **Vitest coverage/typecheck/browser false positives** -- `test.coverage.provider`, `test.typecheck.checker`, and `test.browser.provider` values are now resolved to their corresponding packages
- **ESLint import resolver false positives** -- packages referenced via `settings["import/resolver"]` (e.g., `eslint-import-resolver-typescript`) are now detected in string, array, and object key formats
- **CI pipeline dependency false positives** -- binaries invoked via `npx` in `.gitlab-ci.yml` and `.github/workflows/*.yml` are now detected as used dependencies

## [2.5.1] - 2026-03-28

### Fixed

- **Absolute paths in duplication suggestions** -- refactoring suggestions (e.g. "Extract ... into src/hooks") were printing absolute filesystem paths instead of project-relative paths

## [2.5.0] - 2026-03-28

### Added

- **Project health score** (`--score`, `--min-score`) -- a single 0-100 number with letter grade (A/B/C/D/F) aggregated from dead code, complexity, maintainability, hotspots, unused dependencies, and circular dependencies. Penalty breakdown in JSON is reproducible: `100 - sum(penalties) == score`. The score is shown by default in `fallow health` output and automatically included in vital signs snapshots (schema v2).
- **CI quality gate** (`--min-score N`) -- exit code 1 when the health score drops below a threshold. Pairs with `--score` for a simple "is this codebase healthy enough?" CI check.
- **Regression detection** (`--fail-on-regression`, `--tolerance`) -- CI gate that compares current issue counts against a previously saved baseline. Supports both absolute (`"5"`) and percentage (`"2%"`) tolerance. Baselines can be saved to a file (`--save-regression-baseline`) or embedded in config.

### Changed

- Health score is included by default when running `fallow health` (no section flags). Use `--score` as a section filter to show only the score.
- `--save-snapshot` now automatically includes the health score and grade in the snapshot (snapshot schema v2). Old v1 snapshots remain readable.

## [2.4.0] - 2026-03-27

### Added

- **Test-only production dependency detection** -- new issue type (14th) flags production dependencies that are only imported by test files, suggesting they be moved to `devDependencies`. Full pipeline: all 6 output formats, LSP diagnostics, baseline support, inline suppression, severity rules (default: `warn`).
- **`tap` test runner support** -- `tap` and `@tapjs/*` packages recognized as tooling dependencies
- **`type-only-dependency` inline suppression** -- `// fallow-ignore-next-line type-only-dependency` now works (was a pre-existing gap where the suppression comment was silently ignored)

### Fixed

- **Tooling over-exemption** -- removed blanket `@babel/`, `babel-`, and `@rollup/` prefix exemptions from unused dependency detection. These packages are now verified against actual config files via plugin parsing, catching ~8 previously missed unused dev dependencies.
- **ESLint shared config following** -- ESLint plugin now reads imported config packages one level deep to discover peer dependencies referenced by shared configs
- **Prettier config parsing** -- new PrettierPlugin extracts plugin references from `.prettierrc` and `prettier.config.*` files
- **Storybook addon object form** -- Storybook plugin now handles `{ name: '@storybook/addon-essentials' }` object entries alongside string entries in the `addons` array

## [2.3.1] - 2026-03-27

### Added

- **Comprehensive test coverage** — 1,200+ new tests across all crates bringing total to 4,700+ unit/integration tests, 101 snapshot tests, 30 property-based tests, 14 doc tests, and 7 conformance fixtures, achieving 91% line coverage
- **CJS `module.exports.foo` detection** — individual property assignments like `module.exports.foo = fn` are now extracted as named exports, closing ~60 missed findings on CJS-heavy projects
- **Conformance test harness** — `tests/conformance/verify-fixtures.sh` and `verify-expected.py` provide automated expected-output verification for 7 analysis scenarios (barrel resolution, circular deps, suppression, type-only imports, and more)

### Fixed

- **Unreachable module export blindspot** — modules not reachable from entry points but containing a mix of used/unused exports were previously skipped entirely; now each export is evaluated individually
- **Dead test file removed** — `unused_exports_tests.rs` (924 lines) was never compiled due to a missing module directive and contained a type mismatch; inline tests already covered all cases

## [2.3.0] - 2026-03-27

### Added

- **GitLab CI rich MR comments** — new `FALLOW_COMMENT` and `FALLOW_REVIEW` variables enable rich MR summary comments with collapsible sections and inline review discussions with suggestion blocks, matching the GitHub Action's review quality
- **GitLab inline review discussions** — posts findings as positioned `DiffNote` discussions on MR diffs with "Why this matters" sections, actionable fix steps, docs links, and one-click suppress instructions
- **GitLab suggestion blocks** — unused export findings include `suggestion:-0+0` blocks for one-click `export` keyword removal directly in the MR diff
- **Comment merging pipeline** — groups unused exports per file into single comments, deduplicates clone group findings, drops redundant refactoring targets, and merges same-line findings with numbered headers
- **Auto `--changed-since` in GitLab MR context** — automatically scopes analysis to changed files using `CI_MERGE_REQUEST_DIFF_BASE_SHA` when running in merge request pipelines
- **Package manager detection** — review comments and annotations now show correct install/uninstall commands (`npm uninstall`, `pnpm remove`, or `yarn remove`) based on lock file detection
- **GitLab comment cleanup** — automatically removes previous fallow comments and discussions on re-runs to prevent comment spam

### Changed

- **GitLab CI template modularized** — inline jq scripts extracted to separate files in `ci/jq/` and `ci/scripts/`, downloaded at runtime for maintainability
- **`diff_refs` from MR API** — GitLab inline discussions now fetch `base_sha`, `start_sha`, `head_sha` from the MR API instead of CI environment variables, matching the ictu-mcp pattern and improving positioning accuracy after rebases
- **Suggestion block ANSI-C quoting** — GitHub Action suggestion blocks fixed to use `$'...'` quoting for correct newline rendering

### Fixed

- **Subshell variable loss** — review comment counters (`POSTED`/`SKIPPED`) now use process substitution instead of pipe subshell to correctly track posting results
- **Comment body assignment** — `comment.sh` separates jq execution from string concatenation to correctly detect jq failures
- **Combined mode null arrays** — `jq -s 'add'` replaced with explicit `jq -n --argjson` to prevent null output when all comment arrays are empty

## [2.2.3] - 2026-03-27

### Added

- **Auto-changed-since for PRs** — GitHub Action automatically scopes analysis to changed files in pull requests using `--changed-since` with the merge base, eliminating the need for manual configuration
- **Enriched PR annotations** — inline annotations now include actionable context (export names, dependency names, file paths) with improved formatting for duplication annotations
- **VS Code status bar and tree views** — expanded extension UX with project health status bar, issue tree view, and pnpm workspace support
- **`analyze_with_parse_result` API** — new public function in `fallow-core` that accepts pre-parsed modules, enabling callers to skip the parsing stage when modules are already available

### Changed

- **Health pipeline optimization** — `fallow health --file-scores` no longer runs the analysis pipeline twice; pre-parsed modules are reused via the new `analyze_with_parse_result` API
- **O(1) tooling dependency lookups** — `GENERAL_TOOLING_EXACT` (76 entries) converted from linear slice scan to `OnceLock<FxHashSet>` for constant-time lookups
- **O(1) unused import binding lookups** — `ResolvedModule.unused_import_bindings` converted from `Vec<String>` to `FxHashSet<String>` in hot-path reference population
- **Optimized member export referencing** — `mark_member_exports_referenced` now uses `FxHashSet<&str>` and avoids per-export `to_string()` allocation
- **Report dispatcher unified** — new `ReportContext` struct replaces individual parameters across all 3 report dispatch functions for consistent signatures
- **`define_plugin!` macro extended** — supports `resolve_config: imports_only` variant; Cypress, Commitlint, Remark plugins migrated
- **Comprehensive code deduplication** — `emit_json()`, `plural()`, `build_json_envelope()`, shared `sample_results` test helper, fix module helpers, config parser shared traversal

### Fixed

- **Watch mode reload stability** — hardened debounce behavior and related cleanup
- **Windows CI path normalization** — discovery tests now normalize path separators for cross-platform compatibility
- **GitHub Action `pull_request_target` handling** — correctly detects and handles `pull_request_target` events in auto-changed-since logic

### Removed

- **1,986 lines of dead code** — removed orphaned `crates/graph/src/graph/build/` directory that was never compiled (abandoned refactoring artifact)

## [2.2.2] - 2026-03-27

### Added

- **CodeClimate output format** — `--format codeclimate` for GitLab Code Quality integration, with deterministic FNV-1a fingerprints and proper severity mapping
- **GitHub Actions inline annotations** — `--format annotations` emits `::warning` / `::error` workflow commands for inline PR annotations without any Action dependency
- **Real-world conformance benchmarks** — CI now validates against 8 real-world projects (zod, preact, vite, next.js, angular, nuxt, svelte, vue-core)
- **~283 new tests** — comprehensive coverage for complexity metrics, JSDoc @public tags, config extends/merge, re-export chain propagation, dynamic import patterns, declaration extraction, visitor helpers, analysis predicates, cycle detection, and file discovery

### Fixed

- **CodeClimate fingerprint stability** — use FNV-1a instead of `DefaultHasher` for deterministic cross-run fingerprints; include group index in duplication fingerprints
- **Circular dependency annotations** — sanitize chain strings and guard against empty files in annotation output
- **npm/pnpm install stdout leak** — suppress package manager install stdout that leaked into JSON report output
- **Duplicate exports comparison** — handle dict locations correctly in `duplicate_exports` comparison

## [2.2.1] - 2026-03-26

### Changed

- **Parallel workspace processing** — workspace entry point discovery and plugin runs now execute in parallel using rayon, with sequential merge for deterministic results. Up to 21% faster on monorepos (vite: 507ms → 399ms, next.js: 1532ms → 1371ms)
- **Lazy canonicalize** — skips upfront bulk `canonicalize()` of all source files when the project root is already canonical (common case). A `OnceLock`-based fallback handles the rare intra-project symlink edge case on demand. Saves up to 148ms on 20k-file projects
- **O(1) plugin dedup** — workspace plugin name and virtual module prefix deduplication uses `FxHashSet` instead of `Vec::contains` (O(n²) → O(n))

### Fixed

- **Benchmark accuracy** — benchmark script now correctly excludes knip runs that crash (exit code 2) instead of counting crash timings as valid results. Also guards against null status from timeouts
- **Updated benchmark numbers** — rebenchmarked all projects with honest error handling. Speed claims updated: 5-41x vs knip v5 (was 3-36x), 2-18x vs knip v6 (was 2-14x), 8-26x vs jscpd (was 20-33x)

## [2.2.0] - 2026-03-26

### Added

- **Efficiency score** — refactoring targets now include an `efficiency` field (`priority / effort`) and are sorted by efficiency descending, surfacing quick wins first
- **Confidence levels** — each target includes a `confidence` field (`high`/`medium`/`low`) based on data source reliability: `high` for graph/AST analysis, `medium` for heuristic thresholds, `low` for git-dependent recommendations
- **Adaptive thresholds** — fan-in/fan-out normalization uses percentile-based thresholds (p95/p90/p75/p25) from the project's distribution instead of hardcoded constants, with floors to prevent degenerate values in small projects
- **Target thresholds in JSON** — `target_thresholds` object in health JSON output exposes the computed adaptive thresholds for programmatic consumers
- **Effort summary** — human output shows effort breakdown after the targets header (e.g., `16 low effort · 34 medium · 43 high`)
- **Machine-parseable compact categories** — compact output uses underscore-delimited category labels (`circular_dep`, `dead_code`) instead of space-separated labels

### Changed

- **Human output: efficiency as primary score** — the hero number is now efficiency (sort key), with priority shown as a dimmed secondary value
- **Human output: labeled metadata** — effort and confidence on line 2 are now prefixed (`effort:low · confidence:high`) for self-documenting output
- **Markdown table: 5 columns** — reduced from 7 to 5 columns by merging effort/confidence and dropping the separate priority column
- **SARIF messages** — now include priority, efficiency, and confidence values

### Fixed

- **Cycle path deduplication** — `evidence.cycle_path` no longer contains duplicate entries when a file participates in multiple cycles
- **GitLab CI template** — uses Alpine image and detects package manager correctly
- **Benchmark alert threshold** — corrected for `customBiggerIsBetter` benchmarks
- **SARIF version redaction** in test fixtures
- **MCP analyze tool description** — corrected to match `dead-code` command

## [2.1.0] - 2026-03-25

### Added

- **GitLab CI template** (`ci/gitlab-ci.yml`) — includable template with full feature parity to the GitHub Action: Code Quality reports (CodeClimate format) for inline MR annotations, MR comment summaries, incremental caching, and all fallow commands/options via `FALLOW_*` variables
- **GitHub Action: test workflow** — CI validation for SARIF, JSON, dupes, fix, zero-issues, and PR comment scenarios

### Fixed

- **`list --no-cache`** — the `--no-cache` flag now works correctly with the `list` command
- **GitHub Action: `check` → `dead-code` rename** — completed the rename across all case statements, SARIF fallback, and job summary dispatch
- **`dead-code` subcommand** in backwards-compatibility stable interface list

## [2.0.1] - 2026-03-25

### Added

- **MCP server: all global CLI flags exposed** — `--baseline`, `--save-baseline`, `--no-cache`, `--threads` now available on all MCP tools; `--config` gap-filled on `find_dupes`/`check_health`; `--workspace` gap-filled on `find_dupes`/`fix_preview`/`fix_apply`
- **GitHub Action: 13 new inputs** — `no-cache`, `threads`, `only`, `skip`, `cross-language`, `file-scores`, `hotspots`, `targets`, `complexity`, `since`, `min-commits`, `save-snapshot`, `issue-types`
- **GitHub Action: `dead-code` alias support** — all case statements now handle both `dead-code` and legacy `check` command names
- **GitHub Action: bare invocation support** — combined issue count extraction, job summary, and PR comments work when no command is specified

### Fixed

- **GitHub Action: `fix` without `--dry-run` now adds `--yes`** — previously would hang in CI waiting for TTY input

## [2.0.0] - 2026-03-25

### Breaking

- **Bare `fallow` now runs all analyses** (check + dupes + health). Previously it ran only dead code analysis. Use `fallow dead-code` or `fallow --only check` for the old behavior.
- **`check` renamed to `dead-code`**: `fallow dead-code` is now the canonical name for dead code analysis. `fallow check` remains as a hidden alias for backwards compatibility.
- **`--ci`, `--fail-on-issues`, `--sarif-file` are now global flags**: work with all commands (check, dupes, health, bare). Previously check-only.
- **Combined JSON output**: bare `fallow --format json` produces `{ "check": {...}, "dupes": {...}, "health": {...} }` instead of a flat check-only object.
- **Combined SARIF output**: bare `fallow --format sarif` produces a multi-run SARIF document with one run per analysis.
- **MCP server tools** now use `dead-code` subcommand internally.

### Added

- **Combined analysis command**: bare `fallow` runs dead code + duplication + complexity in a single invocation with combined output across all 5 formats
- **`--only` / `--skip` flags**: select which analyses to run when using bare `fallow` (e.g., `fallow --only check,dupes`, `fallow --skip health`)
- **`dead-code` command alias**: `fallow dead-code` as the canonical name for dead code analysis (replaces `check`)
- **`--ci` on all commands**: `fallow dupes --ci` and `fallow health --ci` now work (SARIF + quiet + fail-on-issues)
- **Vital signs snapshots** (`fallow health --save-snapshot`): save codebase health metrics for trend tracking
- **Execute/run split**: internal refactor enabling combined mode — `execute_check`, `execute_dupes`, `execute_health` return results without printing

### Changed

- Codebase refactored: 10 large files split into focused submodules for maintainability

## [1.9.0] - 2026-03-25

### Added
- **Refactoring targets** (`fallow health --targets`): ranked, actionable recommendations that synthesize complexity, coupling, churn, and dead code signals into a prioritized list of files to refactor
  - Seven recommendation rules evaluated in priority order: urgent churn+complexity, break circular dependency, split high-impact file, remove dead code, extract complex functions, reduce coupling
  - Priority formula: `min(density,1)×30 + hotspot×25 + dead_code×20 + fan_in_norm×15 + fan_out_norm×10`
  - Contributing factors with raw `value` and `threshold` for programmatic use
  - **Effort estimation** (`low`/`medium`/`high`) based on file size, function count, and fan-in — shown in all output formats
  - **Evidence linking**: structured data for AI agents — unused export names, complex function names with line numbers, cycle member paths
  - **Baseline support**: `--save-baseline` / `--baseline` now includes refactoring targets for tracking progress over time
  - All five output formats: human (category · effort labels), JSON (with evidence), compact, markdown (Effort column), SARIF (warning-level findings)
  - MCP server: `targets` parameter on `check_health` tool
- VS Code extension overhaul: LSP notifications, walkthrough, diagnostics improvements, tree view enhancements
- Clickable documentation links in all diagnostics + JSON schema for config
- ~100 unit tests across 6 crates

### Fixed
- 11 bug fixes across parser, analysis, baseline, LSP, and performance
- Empty tree views now show "no issues found" after analysis
- `hasAnalyzed` context set from CLI analysis to clear welcome views
- Removed buggy extract duplicate code action
- Qualified `Span` in benchmark to resolve ambiguous import

## [1.8.1] - 2026-03-25

### Added
- LSP pull-model diagnostics (`textDocument/diagnostic`) for Cursor/VSCodium compatibility
- Multi-root workspace support in LSP: discovers monorepo workspaces via `package.json` workspaces, pnpm-workspace.yaml, and tsconfig references
- Precise diagnostic ranges for unresolved imports: squiggly underline now covers only the module specifier string literal, not the entire import line
- `source_span` field on `ImportInfo` for precise source string literal positions

### Fixed
- LSP `textDocument/diagnostic` errors in Cursor (Method not found -32601)
- Unused dependency diagnostics now appear on the correct line in package.json (was always line 1)
- Circular dependency diagnostics now appear on the import statement that starts the cycle
- File-level diagnostics (unused files, unlisted deps) highlight the entire first line instead of a zero-width marker
- Stale cached diagnostics cleared when issues are resolved between analysis runs
- Extract cache version bumped to 16 for new `ImportInfo.source_span` field

### Changed
- Internal refactoring: split large modules (human.rs, visitor.rs, resolve.rs, registry.rs, etc.) into focused submodules
- Improved test coverage across graph, extract, core, and LSP crates

## [1.8.0] - 2026-03-24

### Added
- **Human output polish**: completely redesigned terminal output across all three commands for readability at scale
  - Per-section footers with one-line explanations and docs links (always shown, replaces `--explain` for human output)
  - Mirrored directory detection in dupes: collapses `src/ ↔ deno/lib/` patterns into a single group
  - Circular dependency hub grouping: cycles sharing the same file are grouped with path elision
  - Duplicate exports stacked vertically with path elision for long monorepo paths
  - Global truncation: all sections default to 10 items with `... and N more` overflow hints
  - Thousands separators for large numbers (e.g., `5,433` lines)
- **`--complexity` flag** for `fallow health`: select only the complexity findings section
- **Health defaults to all sections**: `fallow health` now shows complexity + file-scores + hotspots by default. Use `--complexity`, `--file-scores`, or `--hotspots` to select specific sections.
- **`--explain` flag**: adds `_meta` object with metric definitions and docs URLs to JSON output. Human output always includes explanations.
- **`--top` flag** for `fallow dupes`: limit the number of clone groups shown
- `elide_common_prefix` utility for shorter paths in dependency chains and duplicate exports

### Changed
- Summary footer uses shortened labels (`27 files · 89 exports` instead of `27 unused files · 89 unused exports`)
- Health footer compacted to single line (`✗ 22 above threshold · 3111 analyzed · MI 90.5`)
- Dupes footer uses `✗` prefix consistent with check and health
- Health complexity findings use two-line format (function name on line 1, metrics on line 2)
- Single-group clone families suppressed from default output
- Tree connectors (├─ └─) replaced with simple indentation in dupes
- All docs URLs updated to `/explanations/{dead-code,health,duplication}` with section anchors
- Config warnings use `tracing::warn!` instead of `eprintln!`
- CI workflow permissions scoped to per-job level

## [1.7.0] - 2026-03-24

### Added
- Hotspot analysis (`fallow health --hotspots`): combines git churn history with complexity data to surface the riskiest files in a codebase. Score formula: `normalized_churn × normalized_complexity × 100` (0-100 scale). Recency-weighted commit count with exponential decay (half-life 90 days). Trend detection labels files as accelerating, stable, or cooling.
  - `--since` accepts durations (`6m`, `90d`, `1y`, `2w`) and ISO dates (`2025-06-01`), default 6 months
  - `--min-commits` threshold (default 3) excludes low-activity files from ranking
  - Fan-in shown as separate "blast radius" column
  - Shallow clone detection with warning
  - Available in all output formats (human, JSON, compact, markdown)
  - MCP `check_health` tool supports `hotspots`, `since`, and `min_commits` parameters

### Changed
- Health human output uses two-line format for hotspots and file scores: score/MI on first line with path (dimmed directory, bold filename), metrics on indented second line
- Renamed "lines" to "churn" in hotspot output across all formats to avoid ambiguity with file length

### Fixed
- MCP tests on Windows: skip `/bin/sh`-dependent tests on non-Unix platforms
- Typo checker false positive and Windows path separator in list tests

## [1.6.1] - 2026-03-24

### Added
- Per-file health scores (`fallow health --file-scores`): maintainability index combining complexity density, dead code ratio, fan-in, and fan-out. Available in all output formats (human, JSON, compact, markdown, SARIF). MCP `check_health` tool supports `file_scores: true` parameter.
- Markdown and SARIF output formats for health command
- Workspace scoping (`--workspace`) and baseline support (`--baseline`/`--save-baseline`) for health command

### Fixed
- Re-export chain propagation for entry-point-only exports: exports consumed solely via re-export from an entry point barrel (e.g., `export { render } from './render'` in `src/index.js`) are no longer falsely reported as unused. Fixes false positives for the common library barrel pattern.
- Entry point star re-exports (`export * from './source'`) now correctly exclude the default export per ES spec
- Deterministic bare specifier resolution: removed the `BareSpecifierCache` that caused non-deterministic results in multi-threaded mode when per-file tsconfig path aliases resolved the same specifier to different targets. Replaced with a deterministic post-resolution pass that upgrades `NpmPackage` to `InternalModule`. Analysis results are now identical between `--threads 1` and default multi-threaded mode.
- Hard error on health baseline I/O failures instead of silent fallback
- MCP server test and review findings addressed

## [1.6.0] - 2026-03-24

### Added
- Precise line/column locations in SARIF output for all issue types that previously defaulted to line 1
  - Unlisted dependencies: SARIF now points to the actual import statement in the source file
  - Unused dependencies (all variants): SARIF now points to the dependency entry line in package.json
  - Type-only dependencies: SARIF now points to the dependency entry line in package.json
  - Circular dependencies: SARIF now points to the import statement that starts the cycle
- `ImportSite` type in JSON output for unlisted dependencies (replaces plain path strings with `{path, line, col}` objects)
- `find_import_span_start()` method on `ModuleGraph` for looking up import statement locations between modules

### Changed
- JSON output `schema_version` bumped to 3 (new fields on `UnusedDependency`, `TypeOnlyDependency`, `CircularDependency`)
- `UnlistedDependency.imported_from` changed from `string[]` to `ImportSite[]` in JSON output
- `UnusedDependency` now includes `line` field (1-based line in package.json)
- `TypeOnlyDependency` now includes `line` field (1-based line in package.json)
- `CircularDependency` now includes `line` and `col` fields (location of the cycle-starting import)

### Fixed
- GitHub Code Scanning alerts for unlisted/unused dependencies no longer point to `package.json:1`
- GitHub Code Scanning alerts for circular dependencies no longer point to line 1 of the first file
- `output-schema.json` `DuplicateExport.locations` corrected from `string[]` to proper `DuplicateLocation[]` objects
- `output-schema.json` now includes missing `unused_optional_dependencies` field

## [1.5.0] - 2026-03-24

### Added
- `fallow health` command: per-function cyclomatic complexity (McCabe classic) and cognitive complexity (SonarSource algorithm) analysis with configurable thresholds (default: cyclomatic 20, cognitive 15), `--top N`, `--sort`, `--changed-since`, and human/JSON/compact output formats
- `[health]` config section with `maxCyclomatic`, `maxCognitive`, and `ignore` glob patterns
- `check_health` MCP tool for AI agent integration
- Function name capture from class methods, object method shorthand, variable declarators, property definitions, and `export default function()`

### Changed
- Duplicate export locations now include line and column numbers (JSON output `schema_version` bumped to 2)
- GitHub Action job summary redesigned with tables, icons, and collapsible sections
- Cache version bumped to 15 (old caches auto-invalidate)

### Fixed
- SARIF URIs now percent-encode brackets for Next.js dynamic routes (`[slug]` → `%5Bslug%5D`)
- JSON output uses relative paths for readable CI summaries
- GHAS availability check before SARIF upload in GitHub Action

## [1.4.0] - 2026-03-23

### Added
- Class instance member tracking: `const svc = new MyService(); svc.greet()` correctly tracks `greet` as a used class member, reducing false positives for class-based code (NestJS services, Angular components, etc.)
- JSDoc `@public` tag support: exports annotated with `/** @public */` (or `/** @api public */`) are never reported as unused, designed for library authors whose exports are consumed by external projects
- Type-only dependencies rule: production dependencies only imported via `import type` are reported as `type-only-dependency` (they should be devDependencies since types are erased at runtime)
- Progress spinners showing analysis pipeline stages (discovery, parsing, resolution, analysis) for better UX on larger projects
- VS Code extension published on Open VSX for Cursor and VSCodium support
- Historical performance metric tracking with GitHub Pages dashboard
- Package.json `#subpath` imports integration test

### Changed
- VS Code extension bundler migrated from esbuild to Rolldown
- Large modules split into focused submodules for maintainability: `discover.rs`, `config.rs`, `resolve.rs`, `detect.rs`, `cache.rs`, `visitor.rs`, `check.rs`, and integration tests
- Watch mode now shows changed file paths and clears screen between runs

### Fixed
- SARIF output now includes `tool.driver.version` and `$schema` fields
- Conformance workflow JSON output handling

## [1.3.1] - 2026-03-23

### Added
- `dupes --changed-since`: duplication detection now supports `--changed-since` to only report clone groups involving changed files
- Angular plugin `resolve_config()`: parses `angular.json` to extract `styles`, `scripts`, `main`, `browser`, and `polyfills` from build targets as entry points; adds Angular peer dependencies (`rxjs`, `@angular/common`, `@angular/platform-browser`, `@angular/build`) to tooling deps; widens entry patterns for Nx monorepo layouts
- Nx plugin `resolve_config()`: parses `project.json` to extract executor references as dependencies and `options.main` as entry points; adds `@nx/angular`, `@nx/storybook`, `@nx/webpack`, and other `@nx/*` packages to tooling deps
- JSON config file filesystem fallback: plugins with JSON config patterns (e.g., `angular.json`, `**/project.json`) are now discovered via filesystem scan when they're not in the JS/TS discovered file set; workspace-level plugins also check the project root for config files
- File-based plugin activation for ESLint and Vitest: plugins now activate when their config files exist in a workspace, not just when the package is in `dependencies`. Fixes false positives in monorepos where `eslint`/`vitest` are only in the root `package.json`
- Vitest plugin marks `setupTests.{ts,tsx,js,jsx}` and `test-setup.{ts,tsx,js,jsx}` as always-used when active, fixing false positives for test setup files referenced via imported/spread base configs
- Nested package entry discovery now searches `services/`, `tools/`, and `utils/` directories in addition to `packages/`, `apps/`, `libs/`, `modules/`, `plugins/`
- Infrastructure entry point detection: Dockerfiles, Procfiles, and `fly.toml` are scanned for source file references

### Fixed
- Bare specifier cache poisoning: the resolver cache for bare specifiers (e.g., `@scope/pkg`) now only caches results from successful `oxc_resolver` resolution; previously, when resolution failed for a tsconfig path alias that looked like an npm package, the `NpmPackage` fallback was cached and prevented correct resolution for all subsequent files
- Production dependency false positives: `plugin_tooling` dependencies (e.g., `zone.js`, `@angular/compiler`) are now excluded from unused production dependency detection, matching the existing dev dependency behavior
- Workspace entry pattern double-prefixing: entry patterns from `resolve_config()` that are already project-root-relative (e.g., `apps/client/src/styles.css` from `angular.json`) are no longer double-prefixed with the workspace path
- `check --changed-since` now filters all issue types: unlisted dependencies, duplicate exports, and circular dependencies were previously unfiltered
- Duplication stats recomputation: `recompute_stats` now deduplicates overlapping line ranges per file (matching the original `compute_stats` logic), fixing inflated `duplicated_lines` counts after baseline or `--changed-since` filtering
- Plugin entry point attribution: entry points discovered by plugins now show the correct plugin name (e.g., `Plugin { name: "nextjs" }`) instead of the generic `Plugin { name: "plugin" }`

## [1.3.0] - 2026-03-22

### Added
- Unused optionalDependencies detection: packages in `optionalDependencies` never imported or used as script binaries are now reported
- Function overload deduplication: multiple overload signatures for the same function name are deduplicated to avoid false positive unused-export reports

### Changed
- Workspace discovery performance: canonicalize optimization reduces syscall overhead during workspace resolution
- Re-export chain performance: `matches_str` optimization for faster re-export chain propagation

### Fixed
- Ecosystem test script: `--root` flag and install commands corrected for cross-project validation

## [1.2.0] - 2026-03-22

### Added
- Unused import binding detection via `oxc_semantic`: imports where the bound name is never read in the importing file no longer count as references to the exported symbol, improving unused-export detection precision
- Namespace destructuring detection: `const { foo, bar } = ns` after `import * as ns` (and dynamic import / require namespaces) now correctly tracks accessed members for narrowing

### Fixed
- Namespace imports with whole-object consumption patterns (`Object.values(ns)`, `{...ns}`, `for..in ns`, `const { a, ...rest } = ns`) now correctly mark all exports as referenced instead of being skipped by the entry-point guard

## [1.1.0] - 2026-03-21

### Added
- Markdown output format (`--format markdown`)
- Oxc-inspired code quality: workspace-level clippy configuration with `all`, `pedantic`, `nursery`, `cargo` lint groups and 13 restriction lints
- `#[expect]` over `#[allow]` for all clippy suppressions (warns when suppression becomes unnecessary)
- Const size assertions on hot-path types (`ModuleNode`, `ModuleInfo`, `ExportInfo`, `ImportInfo`, `Edge`) to prevent accidental struct bloat
- VS Code extension icon and marketplace metadata

### Changed
- Module graph split into focused submodules (`types.rs`, `build.rs`, `reachability.rs`, `re_exports.rs`, `cycles.rs`)
- Dev profile optimized with `debug = false` and selective `opt-level` for proc-macro crates

### Fixed
- Windows build: restrict `ModuleNode` size assertion to Unix (`PathBuf` layout differs on Windows MSVC)

## [1.0.4] - 2026-03-21

### Fixed
- CI failures across typos, cargo-deny, docs, MSRV, and clippy
- Windows symlink support for workspace integration test

## [1.0.3] - 2026-03-20

### Fixed
- npm publish: switch to OIDC trusted publishing (no NPM_TOKEN secret needed)

## [1.0.2] - 2026-03-20

### Fixed
- npm publish: add registry authentication to release workflow

## [1.0.1] - 2026-03-20

### Fixed
- Windows build: restrict `DiscoveredFile` size assertion to unix (different `PathBuf` layout on Windows MSVC)

## [1.0.0] - 2026-03-20

### Added
- TypeScript project references: workspace discovery from `tsconfig.json` `references` field (additive with npm/pnpm workspaces, deduplicated by canonical path)
- Elementary cycle enumeration for circular dependencies (individual cycles within SCCs, max 20 per SCC, deterministic output)
- CSS Modules support (`.module.css`/`.module.scss`) with class name export tracking
- `fallow migrate` command to convert knip and jscpd configs
- CSS/SCSS file discovery with `@import`, `@use`, `@forward`, and `@apply` extraction
- Cross-workspace `exports` field subpath resolution with output→source fallback
- Pnpm content-addressable store detection for injected dependencies
- Cache-aware incremental parsing with `--performance` cache hit/miss stats
- Code Lens with export reference counts and click-to-navigate in LSP
- Duplication diagnostics and "Extract duplicate into function" code action in LSP
- VS Code extension CI, LSP binary builds, and marketplace publishing
- Nuxt `resolve_config()` for deep config parsing (modules, css, plugins, extends, postcss, path aliases)
- Circular dependency benchmarks vs madge and dpdm
- Inline suppression for circular dependencies (`fallow-ignore-file circular-dependency`)
- Backwards compatibility policy (`docs/backwards-compatibility.md`)
- JSON output schema documentation (`docs/output-schema.json`)

### Fixed
- UTF-8 boundary handling in duplication detection (multi-byte character safety)
- Exports map resolution robustness for cross-workspace imports
- Nested output subdirectory mapping (e.g., `dist/esm/utils.mjs` → `src/utils.ts`)
- Trace path matching for monorepo compatibility (canonicalized vs user-provided paths)

## [0.3.0] - 2026-03-18

### Added
- Production mode (`--production`) — excludes test/dev files, limits to production scripts, reports type-only imports
- Clone families with refactoring suggestions (extract function/module)
- Config schema generation (`fallow config-schema`) with `$schema` support for IDE autocomplete
- Duplication baselines (`--save-baseline` / `--baseline`) for incremental CI adoption
- Deep `resolve_config()` for all top 10 framework plugins (ESLint, Vite, Jest, Storybook, Tailwind, Webpack, TypeScript, Babel, Rollup, PostCSS)
- JSONC as default config format (with JSON and TOML still supported)
- Pre-commit hooks for fmt and clippy checks

### Changed
- Migrated repository to fallow-rs organization

### Fixed
- Detection accuracy: star re-export tracking, workspace plugins, JSX fallback
- GitHub Action YAML parse error and benchmark workflow

### Performance
- Optimized resolve, discovery, and duplication detection hotpaths

## [0.2.0] - 2026-03-18

### Added
- Code duplication detection (`fallow dupes`) with suffix array + LCP algorithm
- 4 detection modes: strict, mild, weak, semantic
- Duplication benchmarks comparing fallow dupes vs jscpd (4-75x faster)
- Plugin system with 40 built-in framework plugins as single source of truth
- Dynamic import resolution (template literals, `import.meta.glob`, `require.context`)
- Vue/Svelte SFC parsing with `<script>` block extraction
- Dynamic member heuristics (Object.values/keys/entries, for..in, spread)
- GitHub Action with job summaries, PR comments, and `--sarif-file` flag
- LSP diagnostics for all 10 issue types

### Fixed
- CJS require analysis and namespace narrowing for detection accuracy

### Performance
- Clone detection pipeline optimized up to 177x faster vs initial implementation

## [0.1.7] - 2026-03-17

### Added
- Roadmap and project documentation

### Changed
- Improved human report formatting

## [0.1.6] - 2026-03-17

### Fixed
- Reduced false positives based on knip comparison across 6 public projects

## [0.1.5] - 2026-03-17

### Added
- Plugin system, baseline comparison, and `fallow fix` command
- Comprehensive test coverage for high-priority gaps
- CI regression tracking benchmarks

### Fixed
- Reduced false positives based on knip comparison across 31 projects
- Byte column counting, strip_prefix, and HashSet reuse
- Deduplicated imported_from paths in unlisted dependency reports

## [0.1.4] - 2026-03-17

### Fixed
- Reduced false positives and improved re-export chain detection
- Repository URL casing for GitHub provenance

## [0.1.3] - 2026-03-17

### Fixed
- npm publish path resolution

### Changed
- Updated GitHub Actions to Node.js 24, added npm READMEs

## [0.1.2] - 2026-03-17

### Fixed
- Repository URLs and npm publish error surfacing

## [0.1.1] - 2026-03-17

### Added
- Initial public release
- Dead code analysis with 10 issue types
- npm publishing pipeline with platform-specific binaries (macOS, Linux, Windows)
- LSP server with diagnostics and code actions
- CLI commands: check, watch, fix, init, list, schema
- 4 output formats: human, JSON, SARIF, compact
- Rules system with per-issue-type severity
- Inline suppression comments
- `--changed-since` and `--fail-on-issues` for CI
- Cross-workspace resolution for npm/yarn/pnpm workspaces

[Unreleased]: https://github.com/fallow-rs/fallow/compare/v2.28.0...HEAD
[2.28.0]: https://github.com/fallow-rs/fallow/compare/v2.27.6...v2.28.0
[2.27.6]: https://github.com/fallow-rs/fallow/compare/v2.27.5...v2.27.6
[2.27.5]: https://github.com/fallow-rs/fallow/compare/v2.27.4...v2.27.5
[2.27.4]: https://github.com/fallow-rs/fallow/compare/v2.27.3...v2.27.4
[2.27.3]: https://github.com/fallow-rs/fallow/compare/v2.27.2...v2.27.3
[2.27.2]: https://github.com/fallow-rs/fallow/compare/v2.27.1...v2.27.2
[2.27.1]: https://github.com/fallow-rs/fallow/compare/v2.27.0...v2.27.1
[2.27.0]: https://github.com/fallow-rs/fallow/compare/v2.26.1...v2.27.0
[2.26.1]: https://github.com/fallow-rs/fallow/compare/v2.26.0...v2.26.1
[2.26.0]: https://github.com/fallow-rs/fallow/compare/v2.25.1...v2.26.0
[2.25.1]: https://github.com/fallow-rs/fallow/compare/v2.25.0...v2.25.1
[2.25.0]: https://github.com/fallow-rs/fallow/compare/v2.24.0...v2.25.0
[2.24.0]: https://github.com/fallow-rs/fallow/compare/v2.23.1...v2.24.0
[2.23.1]: https://github.com/fallow-rs/fallow/compare/v2.23.0...v2.23.1
[2.23.0]: https://github.com/fallow-rs/fallow/compare/v2.22.4...v2.23.0
[2.22.4]: https://github.com/fallow-rs/fallow/compare/v2.22.3...v2.22.4
[2.22.3]: https://github.com/fallow-rs/fallow/compare/v2.22.2...v2.22.3
[2.22.2]: https://github.com/fallow-rs/fallow/compare/v2.22.1...v2.22.2
[2.22.1]: https://github.com/fallow-rs/fallow/compare/v2.22.0...v2.22.1
[2.22.0]: https://github.com/fallow-rs/fallow/compare/v2.21.0...v2.22.0
[2.21.0]: https://github.com/fallow-rs/fallow/compare/v2.20.0...v2.21.0
[2.20.0]: https://github.com/fallow-rs/fallow/compare/v2.19.3...v2.20.0
[2.19.3]: https://github.com/fallow-rs/fallow/compare/v2.19.2...v2.19.3
[2.19.2]: https://github.com/fallow-rs/fallow/compare/v2.19.1...v2.19.2
[2.19.1]: https://github.com/fallow-rs/fallow/compare/v2.19.0...v2.19.1
[2.19.0]: https://github.com/fallow-rs/fallow/compare/v2.18.3...v2.19.0
[2.18.3]: https://github.com/fallow-rs/fallow/compare/v2.18.2...v2.18.3
[2.18.2]: https://github.com/fallow-rs/fallow/compare/v2.18.1...v2.18.2
[2.18.1]: https://github.com/fallow-rs/fallow/compare/v2.18.0...v2.18.1
[2.18.0]: https://github.com/fallow-rs/fallow/compare/v2.17.1...v2.18.0
[2.17.1]: https://github.com/fallow-rs/fallow/compare/v2.17.0...v2.17.1
[2.17.0]: https://github.com/fallow-rs/fallow/compare/v2.16.0...v2.17.0
[2.16.0]: https://github.com/fallow-rs/fallow/compare/v2.15.0...v2.16.0
[2.15.0]: https://github.com/fallow-rs/fallow/compare/v2.14.2...v2.15.0
[2.14.2]: https://github.com/fallow-rs/fallow/compare/v2.14.1...v2.14.2
[2.14.1]: https://github.com/fallow-rs/fallow/compare/v2.14.0...v2.14.1
[2.14.0]: https://github.com/fallow-rs/fallow/compare/v2.13.4...v2.14.0
[2.13.4]: https://github.com/fallow-rs/fallow/compare/v2.13.3...v2.13.4
[2.13.3]: https://github.com/fallow-rs/fallow/compare/v2.13.2...v2.13.3
[2.13.2]: https://github.com/fallow-rs/fallow/compare/v2.13.1...v2.13.2
[2.13.1]: https://github.com/fallow-rs/fallow/compare/v2.13.0...v2.13.1
[2.13.0]: https://github.com/fallow-rs/fallow/compare/v2.12.1...v2.13.0
[2.12.1]: https://github.com/fallow-rs/fallow/compare/v2.12.0...v2.12.1
[2.12.0]: https://github.com/fallow-rs/fallow/compare/v2.11.0...v2.12.0
[2.11.0]: https://github.com/fallow-rs/fallow/compare/v2.10.1...v2.11.0
[2.10.1]: https://github.com/fallow-rs/fallow/compare/v2.10.0...v2.10.1
[2.10.0]: https://github.com/fallow-rs/fallow/compare/v2.9.3...v2.10.0
[2.9.3]: https://github.com/fallow-rs/fallow/compare/v2.9.2...v2.9.3
[2.9.2]: https://github.com/fallow-rs/fallow/compare/v2.9.1...v2.9.2
[2.9.1]: https://github.com/fallow-rs/fallow/compare/v2.9.0...v2.9.1
[2.9.0]: https://github.com/fallow-rs/fallow/compare/v2.8.1...v2.9.0
[2.8.1]: https://github.com/fallow-rs/fallow/compare/v2.8.0...v2.8.1
[2.8.0]: https://github.com/fallow-rs/fallow/compare/v2.7.3...v2.8.0
[2.7.3]: https://github.com/fallow-rs/fallow/compare/v2.7.2...v2.7.3
[2.7.2]: https://github.com/fallow-rs/fallow/compare/v2.7.1...v2.7.2
[2.7.1]: https://github.com/fallow-rs/fallow/compare/v2.7.0...v2.7.1
[2.7.0]: https://github.com/fallow-rs/fallow/compare/v2.6.0...v2.7.0
[2.6.0]: https://github.com/fallow-rs/fallow/compare/v2.5.5...v2.6.0
[2.5.5]: https://github.com/fallow-rs/fallow/compare/v2.5.4...v2.5.5
[2.5.4]: https://github.com/fallow-rs/fallow/compare/v2.5.3...v2.5.4
[2.5.3]: https://github.com/fallow-rs/fallow/compare/v2.5.2...v2.5.3
[2.5.2]: https://github.com/fallow-rs/fallow/compare/v2.5.1...v2.5.2
[2.5.1]: https://github.com/fallow-rs/fallow/compare/v2.5.0...v2.5.1
[2.5.0]: https://github.com/fallow-rs/fallow/compare/v2.4.0...v2.5.0
[2.4.0]: https://github.com/fallow-rs/fallow/compare/v2.3.1...v2.4.0
[2.3.1]: https://github.com/fallow-rs/fallow/compare/v2.3.0...v2.3.1
[2.3.0]: https://github.com/fallow-rs/fallow/compare/v2.2.3...v2.3.0
[2.2.3]: https://github.com/fallow-rs/fallow/compare/v2.2.1...v2.2.3
[2.2.1]: https://github.com/fallow-rs/fallow/compare/v2.2.0...v2.2.1
[2.2.0]: https://github.com/fallow-rs/fallow/compare/v2.1.0...v2.2.0
[2.1.0]: https://github.com/fallow-rs/fallow/compare/v2.0.1...v2.1.0
[2.0.1]: https://github.com/fallow-rs/fallow/compare/v2.0.0...v2.0.1
[2.0.0]: https://github.com/fallow-rs/fallow/compare/v1.9.0...v2.0.0
[1.9.0]: https://github.com/fallow-rs/fallow/compare/v1.8.1...v1.9.0
[1.8.1]: https://github.com/fallow-rs/fallow/compare/v1.8.0...v1.8.1
[1.8.0]: https://github.com/fallow-rs/fallow/compare/v1.7.0...v1.8.0
[1.7.0]: https://github.com/fallow-rs/fallow/compare/v1.6.1...v1.7.0
[1.6.1]: https://github.com/fallow-rs/fallow/compare/v1.6.0...v1.6.1
[1.6.0]: https://github.com/fallow-rs/fallow/compare/v1.5.0...v1.6.0
[2.1.0]: https://github.com/fallow-rs/fallow/compare/v2.0.1...v2.1.0
[1.5.0]: https://github.com/fallow-rs/fallow/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/fallow-rs/fallow/compare/v1.3.1...v1.4.0
[1.3.1]: https://github.com/fallow-rs/fallow/compare/v1.3.0...v1.3.1
[1.3.0]: https://github.com/fallow-rs/fallow/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/fallow-rs/fallow/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/fallow-rs/fallow/compare/v1.0.4...v1.1.0
[1.0.4]: https://github.com/fallow-rs/fallow/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/fallow-rs/fallow/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/fallow-rs/fallow/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/fallow-rs/fallow/compare/v1.0.0...v1.0.1
[2.3.0]: https://github.com/fallow-rs/fallow/compare/v2.2.3...v2.3.0
[2.2.3]: https://github.com/fallow-rs/fallow/compare/v2.2.2...v2.2.3
[2.2.2]: https://github.com/fallow-rs/fallow/compare/v2.2.1...v2.2.2
[1.0.0]: https://github.com/fallow-rs/fallow/compare/v0.3.0...v1.0.0
[0.3.0]: https://github.com/fallow-rs/fallow/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/fallow-rs/fallow/compare/v0.1.7...v0.2.0
[0.1.7]: https://github.com/fallow-rs/fallow/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/fallow-rs/fallow/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/fallow-rs/fallow/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/fallow-rs/fallow/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/fallow-rs/fallow/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/fallow-rs/fallow/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/fallow-rs/fallow/releases/tag/v0.1.1
