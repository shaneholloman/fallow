<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/fallow-rs/fallow/main/assets/logo-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/fallow-rs/fallow/main/assets/logo.svg">
    <img src="https://raw.githubusercontent.com/fallow-rs/fallow/main/assets/logo.svg" alt="fallow" width="290">
  </picture>
</p>

<p align="center">
  <strong>Codebase analyzer for TypeScript & JavaScript — unused code, duplication, complexity, and architecture.</strong><br>
  <strong>Rust-native. Zero config. Sub-second.</strong>
</p>

<p align="center">
  <a href="https://github.com/fallow-rs/fallow/actions/workflows/ci.yml"><img src="https://github.com/fallow-rs/fallow/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/fallow-rs/fallow/actions/workflows/coverage.yml"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/fallow-rs/fallow/badges/coverage.json" alt="Coverage"></a>
  <a href="https://crates.io/crates/fallow-cli"><img src="https://img.shields.io/crates/v/fallow-cli.svg" alt="crates.io"></a>
  <a href="https://www.npmjs.com/package/fallow"><img src="https://img.shields.io/npm/v/fallow.svg" alt="npm"></a>
  <a href="https://github.com/fallow-rs/fallow/blob/main/LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License"></a>
  <a href="https://docs.fallow.tools"><img src="https://img.shields.io/badge/docs-docs.fallow.tools-blue.svg" alt="Documentation"></a>
</p>

---

```bash
npx fallow
```

```
 Dead code   3 unused files, 12 unused exports, 2 unused deps       18ms
 Duplication 4 clone groups (2.1% of codebase)                      31ms
 Complexity  7 functions exceed thresholds                           4ms
 Total       26 issues across 847 files                             53ms
```

90 framework plugins. No Node.js runtime. No config file needed.

## Install

```bash
npx fallow                  # Run without installing
npm install -g fallow       # Or install globally (macOS, Linux, Windows)
cargo install fallow-cli    # Or via Cargo
```

## Commands

```bash
fallow                      # Run all three analyses
fallow dead-code            # Dead code only
fallow dupes                # Duplication only
fallow health               # Complexity only
fallow health --production-coverage ./coverage   # Merge runtime coverage into health
fallow license activate --trial --email you@company.com
fallow coverage setup       # Guided first-run setup for production coverage
fallow audit                # Audit changed files (verdict: pass/warn/fail)
fallow fix --dry-run        # Preview auto-removal of dead exports and deps
fallow watch                # Re-analyze on file changes
```

## Dead code

Finds unused files, exports, dependencies, types, enum members, class members, unresolved imports, unlisted dependencies, duplicate exports, circular dependencies (including cross-package cycles in monorepos), boundary violations, type-only dependencies, test-only production dependencies, and stale suppression comments. Entry points are auto-detected from package.json fields, framework conventions, and plugin patterns. Arrow-wrapped dynamic imports (`React.lazy`, `loadable`, `defineAsyncComponent`) are tracked as references. Script multiplexers (`concurrently`, `npm-run-all`) are analyzed to discover transitive script dependencies. JSDoc tags (`@public`, `@internal`, `@beta`, `@alpha`, `@expected-unused`) control export visibility.

```bash
fallow dead-code                          # All dead code issues
fallow dead-code --unused-exports         # Only unused exports
fallow dead-code --circular-deps          # Only circular dependencies
fallow dead-code --boundary-violations    # Only boundary violations
fallow dead-code --stale-suppressions     # Only stale suppression comments
fallow dead-code --production             # Exclude test/dev files
fallow dead-code --changed-since main     # Only changed files (for PRs)
fallow dead-code --file src/utils.ts       # Single file (lint-staged integration)
fallow dead-code --include-entry-exports  # Also check exports from entry files
fallow dead-code --group-by owner         # Group by CODEOWNERS for team triage
fallow dead-code --group-by directory     # Group by first directory component
fallow dead-code --group-by package       # Group by workspace package (monorepo)
```

## Duplication

Finds copy-pasted code blocks across your codebase. Suffix-array algorithm -- no quadratic pairwise comparison.

```bash
fallow dupes                              # Default (mild mode)
fallow dupes --mode semantic              # Catch clones with renamed variables
fallow dupes --skip-local                 # Only cross-directory duplicates
fallow dupes --trace src/utils.ts:42      # Show all clones of code at this location
```

Four detection modes: **strict** (exact tokens), **mild** (default, AST-based), **weak** (different string literals), **semantic** (renamed variables and literals).

## Complexity

Surfaces the most complex functions in your codebase and identifies where to spend refactoring effort.

```bash
fallow health                             # Functions exceeding thresholds
fallow health --score                     # Project health score (0-100) with letter grade
fallow health --min-score 70              # CI gate: fail if score drops below 70
fallow health --top 20                    # 20 most complex functions
fallow health --file-scores               # Per-file maintainability index (0-100)
fallow health --hotspots                  # Riskiest files (git churn x complexity)
fallow health --hotspots --ownership      # Add bus factor, owner, drift signals
fallow health --targets                   # Ranked refactoring recommendations
fallow health --targets --effort low      # Only quick-win refactoring targets
fallow health --coverage-gaps             # Static test coverage gaps
fallow health --coverage coverage/coverage-final.json
fallow health --coverage artifacts/coverage.json --coverage-root /home/runner/work/myapp
fallow health --production-coverage ./coverage
fallow health --production-coverage ./coverage --min-invocations-hot 250
fallow health --trend                     # Compare against saved snapshot
fallow health --changed-since main        # Only changed files
```

### Paid production coverage

Production coverage answers a different question than static reachability: which functions actually execute in production. Fallow can merge V8 coverage dumps (`NODE_V8_COVERAGE=...`) and Istanbul `coverage-final.json` files into the `health` report, classify cold functions, and surface hot paths.

Static `coverage_gaps` and paid `production_coverage` are separate layers in the same `health` surface:
- `coverage_gaps` is graph-based and answers which runtime files or exports have no test dependency path
- `production_coverage` is runtime-based and answers which functions actually executed in production-like coverage input
- `coverage_gaps` can appear either because you passed `--coverage-gaps`, or because top-level `health` enabled it from config severity when no narrower section flags were selected

| Surface | Flag | Input | Answers | License |
|:--|:--|:--|:--|:--|
| Static test reachability | `--coverage-gaps` | none | which runtime files/exports have no test dependency path | no |
| Exact CRAP scoring | `--coverage` | Istanbul JSON file or `coverage-final.json` directory | how covered each function is for CRAP computation | no |
| Runtime production coverage | `--production-coverage` | V8 directory, V8 JSON file, or Istanbul JSON file | which functions actually executed, which stayed cold, which are hot | yes |

```bash
fallow license activate --trial --email you@company.com
fallow coverage setup
fallow health --production-coverage ./coverage
```

- `fallow license activate --trial --email ...` starts a trial and stores the signed license locally
- `fallow license refresh` refreshes the stored license before the hard-fail window
- `fallow coverage setup` detects your framework and package manager, installs the sidecar if needed, writes a collection recipe, and resumes from the current setup state on re-run
- The sidecar can be installed globally or as a project devDependency; fallow resolves `FALLOW_COV_BIN`, project-local shims, package-manager bin lookups, `~/.fallow/bin/fallow-cov`, and `PATH`
- `fallow health --production-coverage <path>` accepts a V8 directory, a single V8 JSON file, or a single Istanbul coverage map JSON file (commonly `coverage-final.json`)
- `fallow health --coverage <path>` accepts a single Istanbul coverage map JSON file or a directory containing `coverage-final.json`
- `--coverage-root <path>` rebases Istanbul file paths before CRAP matching. Use it when coverage was generated in CI or Docker with a different checkout root, for example `fallow health --coverage artifacts/coverage-final.json --coverage-root /home/runner/work/myapp`
- V8 dumps that include Node's `source-map-cache` are remapped through supported source-map paths before analysis, including file paths, relative paths, `webpack://...`, and `vite://...`; unsupported virtual schemes safely fall back to raw V8 handling
- `fallow health --changed-since <ref> --production-coverage <path>` promotes touched hot paths to a `hot-path-changes-needed` verdict during change review

Production coverage is merged into the same human, JSON, SARIF, compact, markdown, and CodeClimate outputs as the rest of the health report.

## Audit

Quality gate for AI-generated code and PRs. Combines dead code + complexity + duplication scoped to changed files.

```bash
fallow audit                              # Auto-detects base branch
fallow audit --base main                  # Explicit base ref
fallow audit --base HEAD~3               # Audit last 3 commits
fallow audit --format json                # Structured output with verdict
```

Returns a verdict: **pass** (exit 0), **warn** (exit 0, warn-severity only), or **fail** (exit 1). JSON output includes a `verdict` field for CI and agent integration.

## CI integration

```yaml
# GitHub Action
- uses: fallow-rs/fallow@v2

# GitLab CI — include the template and extend
include:
  - remote: 'https://raw.githubusercontent.com/fallow-rs/fallow/main/ci/gitlab-ci.yml'
fallow:
  extends: .fallow

# Or run directly on any CI
- run: npx fallow --ci
```

`--ci` enables SARIF output, quiet mode, and non-zero exit on issues. Also supports:

- `--group-by owner|directory|package` -- group output by CODEOWNERS ownership, directory, or workspace package for team-level triage
- `--summary` -- show only category counts (no individual issues)
- `--changed-since main` -- analyze only files touched in a PR
- `--baseline` / `--save-baseline` -- fail only on **new** issues
- `--fail-on-regression` / `--tolerance 2%` -- fail only if issues **grew** beyond tolerance
- `--format sarif` -- upload to GitHub Code Scanning
- `--format codeclimate` -- GitLab Code Quality inline MR annotations
- `--format annotations` -- GitHub Actions inline PR annotations (no Action required)
- `--format json` / `--format markdown` -- for custom workflows (JSON includes machine-actionable `actions` per issue)
- `--format badge` -- shields.io-compatible SVG health badge (`fallow health --format badge > badge.svg`)

Both the GitHub Action and GitLab CI template auto-detect your package manager (npm/pnpm/yarn) from lock files, so install/uninstall commands in review comments match your project.

Adopt incrementally -- surface issues without blocking CI, then promote when ready:

```jsonc
{ "rules": { "unused-files": "error", "unused-exports": "warn", "circular-dependencies": "off" } }
```

### GitLab CI rich MR comments

The GitLab CI template can post rich comments directly on merge requests -- summary comments with collapsible sections and inline review discussions with suggestion blocks.

| Variable | Default | Description |
|---|---|---|
| `FALLOW_COMMENT` | `"false"` | Post a summary comment on the MR with collapsible sections per analysis |
| `FALLOW_REVIEW` | `"false"` | Post inline MR discussions at the relevant lines, with `suggestion` blocks for unused exports |
| `FALLOW_MAX_COMMENTS` | `"50"` | Maximum number of inline review comments |

In MR pipelines, `--changed-since` is set automatically to scope analysis to changed files. Previous fallow comments are cleaned up on re-runs.

The comment merging pipeline groups unused exports per file and deduplicates clone reports, keeping MR threads readable.

A `GITLAB_TOKEN` (PAT with `api` scope) is recommended for full features (suggestion blocks, cleanup of previous comments). `CI_JOB_TOKEN` works for posting but cannot delete comments from prior runs.

```yaml
# .gitlab-ci.yml — full example with rich MR comments
include:
  - remote: 'https://raw.githubusercontent.com/fallow-rs/fallow/main/ci/gitlab-ci.yml'

fallow:
  extends: .fallow
  variables:
    FALLOW_COMMENT: "true"       # Summary comment with collapsible sections
    FALLOW_REVIEW: "true"        # Inline discussions with suggestion blocks
    FALLOW_MAX_COMMENTS: "30"    # Cap inline comments (default: 50)
    FALLOW_FAIL_ON_ISSUES: "true"
```

## Configuration

Works out of the box. When you need to customize, create `.fallowrc.json` or run `fallow init`:

```jsonc
// .fallowrc.json
{
  "$schema": "https://raw.githubusercontent.com/fallow-rs/fallow/main/schema.json",
  "entry": ["src/workers/*.ts", "scripts/*.ts"],
  "ignorePatterns": ["**/*.generated.ts"],
  "ignoreDependencies": ["autoprefixer"],
  "rules": {
    "unused-files": "error",
    "unused-exports": "warn",
    "unused-types": "off"
  },
  "health": {
    "maxCyclomatic": 20,
    "maxCognitive": 15
  }
}
```

Architecture boundary presets enforce import rules between layers with zero manual config:

```jsonc
{ "boundaries": { "preset": "bulletproof" } } // or: layered, hexagonal, feature-sliced
```

Run `fallow list --boundaries` to inspect the expanded rules. TOML also supported (`fallow init --toml`). The init command auto-detects your project structure (monorepo layout, frameworks, existing config) and generates a tailored config. It also adds `.fallow/` to your `.gitignore` (cache and local data). Scaffold a pre-commit hook with `fallow init --hooks`. Migrating from knip or jscpd? Run `fallow migrate`.

See the [full configuration reference](https://docs.fallow.tools/configuration/overview) for all options.

## Framework plugins

90 built-in plugins detect entry points, convention exports, config-defined aliases, and template-visible usage for your framework automatically.

| Category | Plugins |
|---|---|
| **Frameworks** | Next.js, Nuxt, Remix, Qwik, SvelteKit, Gatsby, Astro, Angular, NestJS, Expo, Expo Router, Electron, and more |
| **Bundlers** | Vite, Webpack, Rspack, Rsbuild, Rollup, Rolldown, Tsup, Tsdown, Parcel |
| **Testing** | Vitest, Jest, Playwright, Cypress, Storybook, Mocha, Ava |
| **CSS** | Tailwind, PostCSS, UnoCSS |
| **Databases & Backend** | Prisma, Drizzle, Knex, TypeORM, Kysely, Convex |
| **Blockchain** | Hardhat |
| **Monorepos** | Turborepo, Nx, Changesets, Syncpack, pnpm |

[Full plugin list](https://docs.fallow.tools/frameworks/built-in) -- missing one? Add a [custom plugin](https://docs.fallow.tools/frameworks/custom-plugins) or [open an issue](https://github.com/fallow-rs/fallow/issues).

## Editor & AI support

- **VS Code extension** -- tree views, status bar, one-click fixes, auto-download LSP binary ([Marketplace](https://github.com/fallow-rs/fallow/tree/main/editors/vscode))
- **LSP server** -- real-time diagnostics, hover info, code actions, Code Lens with reference counts
- **MCP server** -- AI agent integration for Claude Code, Cursor, Windsurf ([fallow-skills](https://github.com/fallow-rs/fallow-skills))
- **JSON `actions` array** -- every issue in `--format json` output includes fix suggestions with `auto_fixable` flag, so agents can self-correct

## Fallow vs linters

Linters enforce style. Formatters enforce consistency. Fallow enforces relevance.

ESLint, Biome, and oxlint analyze one file at a time. They catch bad patterns within a file boundary. Fallow builds a module dependency graph across the entire project and finds issues that only appear when you see the whole picture.

| What | Linter | Fallow |
|---|---|---|
| Unused variable in a function | yes | no |
| Unused export that nothing imports | no | yes |
| File that nothing imports | no | yes |
| Circular dependency across modules | no | yes |
| Duplicate code blocks across files | no | yes |
| Dependency in package.json never imported | no | yes |

They're complementary -- run your linter on every save, fallow on every commit.

[Full comparison: fallow vs ESLint, Biome, knip, ts-prune](https://docs.fallow.tools/explanations/fallow-vs-linters)

## Performance

Benchmarked on real open-source projects (median of 5 runs, Apple M5).

### Dead code: fallow vs knip

| Project | Files | fallow | knip v5 | knip v6 | vs v5 | vs v6 |
|:--------|------:|-------:|--------:|--------:|------:|------:|
| [zod](https://github.com/colinhacks/zod) | 174 | **17ms** | 577ms | 300ms | 34x | 18x |
| [fastify](https://github.com/fastify/fastify) | 286 | **19ms** | 791ms | 232ms | 41x | 12x |
| [preact](https://github.com/preactjs/preact) | 244 | **20ms** | 767ms | 2.02s | 39x | 103x |
| [TanStack/query](https://github.com/TanStack/query) | 901 | **170ms** | 2.50s | 1.28s | 15x | 8x |
| [svelte](https://github.com/sveltejs/svelte) | 3,337 | **359ms** | 1.73s | 749ms | 5x | 2x |
| [next.js](https://github.com/vercel/next.js) | 20,416 | **1.66s** | -- | -- | -- | -- |

knip errors out on next.js. fallow completes in under 2 seconds.

### Duplication: fallow vs jscpd

| Project | Files | fallow | jscpd | Speedup |
|:--------|------:|-------:|------:|--------:|
| [fastify](https://github.com/fastify/fastify) | 286 | **76ms** | 1.96s | 26x |
| [vue/core](https://github.com/vuejs/core) | 522 | **124ms** | 3.11s | 25x |
| [next.js](https://github.com/vercel/next.js) | 20,416 | **2.89s** | 24.37s | 8x |

No TypeScript compiler, no Node.js runtime. [How it works](https://docs.fallow.tools/explanations/architecture) | [Reproduce benchmarks](https://github.com/fallow-rs/fallow/tree/main/benchmarks)

## Suppressing findings

```ts
// fallow-ignore-next-line unused-export
export const keepThis = 1;

// fallow-ignore-file
// Suppress all issues in this file
```

Also supports JSDoc visibility tags (`/** @public */`, `/** @internal */`, `/** @beta */`, `/** @alpha */`) to suppress unused export reports for library APIs consumed externally.

## Limitations

fallow uses syntactic analysis -- no type information. This is what makes it fast, but type-level dead code is out of scope. Use [inline suppression comments](#suppressing-findings) or [`ignoreExports`](https://docs.fallow.tools/configuration/overview#ignoring-specific-exports) for edge cases.

## Documentation

- [Getting started](https://docs.fallow.tools)
- [Configuration reference](https://docs.fallow.tools/configuration/overview)
- [CI integration guide](https://docs.fallow.tools/integrations/ci)
- [Migrating from knip](https://docs.fallow.tools/migration/from-knip)
- [Plugin authoring guide](https://github.com/fallow-rs/fallow/blob/main/docs/plugin-authoring.md)

## Contributing

Missing a framework plugin? Found a false positive? [Open an issue](https://github.com/fallow-rs/fallow/issues).

```bash
cargo build --workspace && cargo test --workspace
```

## License

MIT
