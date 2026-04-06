---
paths:
  - "crates/core/src/analyze/**"
  - "crates/extract/src/visitor/**"
  - "crates/graph/src/graph/**"
  - "crates/graph/src/resolve/**"
---

# Detection capabilities

Non-obvious implementation details for each detection feature. These are NOT discoverable from reading the code alone.

## Parser-level
- **HTML entry files**: `<script src="...">` (module and classic) and `<link rel="stylesheet" href="...">` / `<link rel="modulepreload" href="...">` create graph edges from HTML to referenced assets. Remote URLs skipped. HTML comments stripped before matching. HTML files exempt from unused-file detection.
- **Vue/Svelte SFC**: handles `>` in quoted attributes like `generic="T extends Foo<Bar>"`, `<script src="...">` external script support, HTML comment filtering
- **Namespace destructuring**: `const { a, b } = ns` → member accesses. Rest patterns (`const { foo, ...rest } = ns`) → conservative whole-object use. Works with static/dynamic imports and require.
- **Unused import bindings**: via `oxc_semantic` scope-aware symbol analysis. Dead imports don't count as references, improving unused-export precision.
- **TypeScript overload dedup**: `export function foo(): void; export function foo(x: string): string;` treated as single export
- **Class instance members**: `const svc = new MyService(); svc.greet()` tracks `greet` as used. Scope-unaware — false matches produce false negatives, not false positives.
- **Type-level member access**: `TSQualifiedName` (e.g., `type X = Status.Active`) tracked as member access. Mapped type constraints (`{ [K in Enum]: ... }`, `{ [K in keyof typeof Enum]: ... }`) and `Record<Enum, T>` mark all enum members as used via whole-object use.
- **TypeScript namespace exports**: `export namespace Foo { export function bar() {} }` extracts `Foo` as a single export with inner declarations as `NamespaceMember` entries, not as separate top-level exports. Runtime namespaces (no `declare`) are NOT type-only. `declare namespace`/`declare module` remain type-only. Nested namespaces flatten members into the outermost namespace.

## Resolution-level
- **Package.json `exports` subpath**: output dirs (dist/build/out/esm/cjs) mapped back to src/ with source extension fallback, including nested subdirs
- **Pnpm virtual store**: `.pnpm` paths mapped back to workspace source files. Handles injected deps, scoped/unscoped packages, peer dependency suffixes.
- **Package.json `imports` (`#subpath`)**: simple mappings, wildcard patterns, conditional exports. Per-package scoping.
- **React Native platform extensions**: `.web.ts`, `.ios.ts`, `.android.ts`, `.native.ts` resolved alongside standard extensions
- **Tsconfig path aliases**: per-file discovery resolves `@/utils` by finding nearest tsconfig.json per file

## Graph-level
- **`export *` chain propagation**: multi-level barrel file chains fully resolved for transitive usage tracking
- **CSS Modules**: default imports (`import styles from '...'`) resolve `styles.className` to named exports via graph-level narrowing. Spread/`Object.values` handled conservatively.

## Analysis-level
- **Decorated class members**: members with decorators (NestJS `@Get()`, Angular `@Input()`, TypeORM `@Column()`) not reported unused
- **JSDoc `@public` tag**: exports annotated with `/** @public */` never reported unused. Only `/** */` block comments recognized.
- **Infrastructure entry points**: Dockerfiles, Procfiles, fly.toml scanned for source file refs. Searches root and config/docker/deploy subdirs.
- **TypeScript project references**: tsconfig.json `references` field discovered as workspaces (additive with npm/pnpm workspaces)
- **CI file scanning**: `.gitlab-ci.yml` and `.github/workflows/*.yml` scanned for binary invocations (npx, direct binaries). Maps binaries to package names to prevent false "unused dependency" reports for CI-only packages.
- **Production mode**: excludes test/dev files, only start/build scripts, skips unused devDeps, detects type-only deps
- **Platform built-in modules**: `node:`, `bun:`, `cloudflare:` prefixed imports and Deno `std` are recognized as builtins — never flagged as unlisted dependencies.
- **`ignoreDependencies` dual suppression**: listed packages are excluded from both unused dependency AND unlisted dependency detection, making it useful for runtime-provided packages (e.g., `bun:sqlite`).
- **@types/ unlisted dep suppression**: imports from `pkg` not flagged as unlisted when `@types/pkg` is in deps (TypeScript resolves types from `@types/` and erases the import regardless of `import type` syntax). Scoped packages use DefinitelyTyped convention (`@scope/pkg` → `@types/scope__pkg`).
- **Architecture boundary violations**: user-defined zones (glob patterns) with directional import rules. Fires at direct import site using resolved target's zone. Unzoned files are unrestricted in both directions. First-match zone classification. Self-imports always allowed. Inline and file-level suppression supported. Built-in presets: `layered` (presentation/application/domain/infrastructure), `hexagonal` (adapters/ports/domain), `feature-sliced` (app/pages/widgets/features/entities/shared), `bulletproof` (app/features/shared/server — shared zone covers components, hooks, lib, utils, utilities, providers, shared, types, styles, i18n). Presets auto-detect `rootDir` from tsconfig.json for pattern prefix. User zones/rules merge on top of presets (same-name replaces). Zones with zero file matches emit warnings.
- **Static test coverage gaps**: module graph reachability from test entry points to runtime files/exports. Not line-level coverage. Test files are identified by plugin patterns and production-mode exclusion rules. Reports runtime files with no test dependency path and runtime exports with no test-reachable reference chain. Gated by `--coverage-gaps` flag on the health command and `coverage-gaps` rule severity.
