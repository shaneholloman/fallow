# Changelog

All notable changes to fallow are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/fallow-rs/fallow/compare/v2.14.1...HEAD
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
