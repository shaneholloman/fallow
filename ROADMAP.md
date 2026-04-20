# Fallow Roadmap

> Last updated: 2026-04-20

Fallow is the codebase intelligence layer for TypeScript and JavaScript. Static analysis shows how the codebase is wired: unused code, duplication, complexity, architecture boundaries, and feature flags. Runtime intelligence shows what actually executes: hot paths, cold paths, and the risks inside every change. Together they let humans and agents clean up and refactor with confidence.

AI agents ship code faster than teams can review it. The problem is not only dead code, it is structural drift and execution drift. Fallow addresses both.

---

## Where we are (v2.42.0)

Two layers ship today: the static layer (free and open source) covers how the code is wired, and the runtime layer (paid) covers what actually executes in production.

**Dead code analysis** -- 14 issue types: unused files, exports, types, dependencies, enum/class members, unresolved imports, unlisted deps, duplicate exports, circular dependencies, type-only dependencies, and test-only production dependencies. 90 framework plugins with auto-detection. Auto-fix for safe removals. Inline suppression. Severity rules (`error` / `warn` / `off`).

**Code duplication** -- 4 detection modes (strict, mild, weak, semantic) with cross-language TS/JS matching and cross-directory filtering.

**Health analysis** -- function complexity (cyclomatic + cognitive), per-file maintainability scores, git-churn hotspot analysis, ranked refactoring targets with effort estimation and adaptive thresholds. Vital signs snapshots with trend reporting (`--trend` compares against saved snapshots with directional indicators). Static test coverage gaps are shipped, and paid production-coverage analysis can merge V8 / Istanbul runtime data into the health report.

**Production coverage workflow** -- `fallow license {activate, status, refresh, deactivate}` is live with signed JWT verification and networked trial / refresh flows. `fallow coverage setup` is now a resumable state machine: license bootstrap, sidecar install, framework-specific coverage recipe generation, and automatic handoff back into `fallow health --production-coverage`.

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

### Runtime intelligence

The core production-coverage path is live. Next up: better hot-path change review (`HotPathChangesNeeded` once fallow can correlate modified code with hot functions), deeper framework heuristics, and smoother packaging for the companion sidecar install experience. Runtime intelligence is the paid team layer, and it is where heavier workflows (alerts, runtime-backed review, stale-flag evidence) will land.

### Pre-commit hooks

Catch unused exports and unresolved imports before they reach CI. Scoped to changed files for sub-second feedback.

---

## Ongoing

- **Incremental analysis** -- finer-grained caching for faster watch mode and CI on large monorepos
- **Plugin ecosystem** -- more framework coverage, better external plugin authoring, community-contributed plugins
- **Health intelligence** -- trend reporting, regression detection, audit, static coverage gaps, and production coverage are shipped; next up: structured fix suggestions and HTML report cards
- **Agent integration** -- richer MCP tool responses, Claude Code hooks, Cursor integration, agent skill packages

---

## Known limitations

- **Syntactic analysis only** -- no TypeScript type information. Projects using `isolatedModules: true` (the modern default) are well-served; legacy tsc-only patterns may produce false positives.
- **Config parsing ceiling** -- AST-based extraction handles static configs. Computed values and conditionals are out of reach without JS eval.
- **Svelte export false negatives** -- props (`export let`) can't be distinguished from utility exports without Svelte compiler semantics.
- **NestJS/DI class members** -- abstract methods consumed via DI are not tracked. Use `unused_class_members = "off"` for DI-heavy projects.

---

## Competitive context

- **Knip** -- the closest alternative on dead code. Both use the Oxc parser, but fallow runs as a native Rust binary with no Node.js runtime -- 3-18x faster in benchmarks, and Knip errors out on the largest monorepos (20k+ files). Fallow also goes beyond dead code into duplication, complexity, architecture, and runtime intelligence, so the comparison is only one slice of what fallow does.
- **Biome** -- has module graph infrastructure but hasn't shipped cross-file unused export detection. If they do, they cover ~1 of fallow's 14 issue types.
- **SonarQube** -- dominates enterprise code quality but is Java-centric, slow on JS/TS, and lacks framework-aware dead code analysis.
- **AI code review tools** -- complementary. AI generates code faster than humans review it, which accelerates both dead code and structural drift. Fallow is the codebase-intelligence layer that AI reviewers lack: it sees the full module graph and the runtime execution profile, not just the diff.

---

```bash
npx fallow              # All analyses -- zero config, sub-second
npx fallow dead-code    # Unused code only
npx fallow dupes        # Find copy-paste clones
npx fallow health       # Complexity, hotspots, refactoring targets
npx fallow fix --dry-run # Preview safe auto-removals
```

[Open an issue](https://github.com/fallow-rs/fallow/issues) to request a feature or report a bug. PRs welcome -- check the [contributing guide](CONTRIBUTING.md) and the [issues labeled "good first issue"](https://github.com/fallow-rs/fallow/issues?q=label%3A%22good+first+issue%22).
