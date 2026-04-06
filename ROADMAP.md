# Fallow Roadmap

> Last updated: 2026-04-06

AI agents write more code than ever. They rarely clean up after themselves. Every generated file, every scaffolded export, every copied utility accumulates until your codebase is half dead weight.

Fallow is the counterbalance: fast, framework-aware dead code detection that works at the speed AI generates code. One binary, sub-second analysis, 14 issue types, 84 framework plugins, auto-fix, and native integration with the tools agents already use.

---

## Where we are (v2.14.1)

**Dead code analysis** -- 14 issue types: unused files, exports, types, dependencies, enum/class members, unresolved imports, unlisted deps, duplicate exports, circular dependencies, type-only dependencies, and test-only production dependencies. 84 framework plugins with auto-detection. Auto-fix for safe removals. Inline suppression. Severity rules (`error` / `warn` / `off`).

**Code duplication** -- 4 detection modes (strict, mild, weak, semantic) with cross-language TS/JS matching and cross-directory filtering.

**Health analysis** -- function complexity (cyclomatic + cognitive), per-file maintainability scores, git-churn hotspot analysis, ranked refactoring targets with effort estimation and adaptive thresholds. Vital signs snapshots with trend reporting (`--trend` compares against saved snapshots with directional indicators).

**CI/CD integration** -- GitHub Action with SARIF upload, inline PR annotations, review comments with suggestion blocks, and auto-changed-since for PR scoping. GitLab CI template with Code Quality reports, MR comments, and inline discussions. Baseline support for incremental adoption.

**Agent and editor tooling** -- MCP server so AI agents can query fallow directly. LSP server with multi-root workspace support. VS Code extension with diagnostics, tree views, and status bar. The detect-analyze-fix loop works whether a human or an agent drives it.

**6 output formats** -- human, JSON, SARIF, compact, markdown, CodeClimate.

---

## Where we're going

### The agent-driven cleanup loop

Fallow already auto-fixes safe removals (unused exports, enum members, dependencies). The next step: AI agents handle the judgment calls. Fallow provides structured analysis via MCP, the agent decides whether to delete a file, restructure a module, or consolidate duplicates. The human reviews the PR. This is the workflow: detect, delegate, review.

Coming next: unused class member removal, automatic formatter integration, and richer MCP responses that give agents enough context to make confident cleanup decisions.

### Codebase health grade

A single letter grade (A-F) for your project, computed from dead code ratio, duplication percentage, complexity density, and dependency hygiene. Visible in CI, in your README via badge, and tracked over time with vital signs snapshots. Managers understand it. Developers trust it. Agents optimize for it.

### Dependency risk scoring

Cross-reference unused dependencies with vulnerability data. "These 3 unused deps have known CVEs -- remove them for a free security win." Only fallow can surface this because only fallow knows which deps are actually unused.

### Visualization

`fallow viz` -- a self-contained interactive HTML report. Treemap with dead code highlighted, dependency graph, cycle visualization, duplication heatmaps. No server required, opens in any browser.

### Architecture boundaries

Define import rules between directory-based layers (`src/ui/` cannot import from `src/db/`). Validated against the module graph -- like dependency-cruiser but faster and integrated with dead code analysis.

### Static test coverage gaps

Identify exports and files with no test file dependency -- without running tests. Uses the module graph to find untested code. The CI use case: "your PR adds 3 untested exports."

### Pre-commit hooks

Catch unused exports and unresolved imports before they reach CI. Scoped to changed files for sub-second feedback.

---

## Ongoing

- **Incremental analysis** -- finer-grained caching for faster watch mode and CI on large monorepos
- **Plugin ecosystem** -- more framework coverage, better external plugin authoring, community-contributed plugins
- **Health intelligence** -- trend reporting and regression detection are shipped; next up: structured fix suggestions, audit command, HTML report cards
- **Agent integration** -- richer MCP tool responses, Claude Code hooks, Cursor integration, agent skill packages

---

## Known limitations

- **Syntactic analysis only** -- no TypeScript type information. Projects using `isolatedModules: true` (the modern default) are well-served; legacy tsc-only patterns may produce false positives.
- **Config parsing ceiling** -- AST-based extraction handles static configs. Computed values and conditionals are out of reach without JS eval.
- **Svelte export false negatives** -- props (`export let`) can't be distinguished from utility exports without Svelte compiler semantics.
- **NestJS/DI class members** -- abstract methods consumed via DI are not tracked. Use `unused_class_members = "off"` for DI-heavy projects.

---

## Competitive context

- **Knip** -- the closest alternative. Both use the Oxc parser, but fallow runs as a native Rust binary with no Node.js runtime -- 3-18x faster in benchmarks. Knip errors out on the largest monorepos (20k+ files).
- **Biome** -- has module graph infrastructure but hasn't shipped cross-file unused export detection. If they do, they cover ~1 of fallow's 14 issue types.
- **SonarQube** -- dominates enterprise code quality but is Java-centric, slow on JS/TS, and lacks framework-aware dead code analysis.
- **AI code review tools** -- complementary. AI generates code faster than humans review it, accelerating the dead code problem. Fallow is the structural analysis layer that AI reviewers lack: it sees the full module graph, not just the diff.

---

```bash
npx fallow              # All analyses -- zero config, sub-second
npx fallow dead-code    # Unused code only
npx fallow dupes        # Find copy-paste clones
npx fallow health       # Complexity, hotspots, refactoring targets
npx fallow fix --dry-run # Preview safe auto-removals
```

[Open an issue](https://github.com/fallow-rs/fallow/issues) to request a feature or report a bug. PRs welcome -- check the [contributing guide](CONTRIBUTING.md) and the [issues labeled "good first issue"](https://github.com/fallow-rs/fallow/issues?q=label%3A%22good+first+issue%22).
