# Fallow Roadmap

> Last updated: 2026-04-23

This roadmap tracks planned work on Fallow. For shipped capabilities, see the [documentation](https://docs.fallow.tools) and [GitHub releases](https://github.com/fallow-rs/fallow/releases).

---

## Next

Concrete work scoped to the next one or two minor releases.

### Hot-path change review

Production coverage ships the data; the review workflow on top of it does not yet exist. `HotPathChangesNeeded` will correlate a PR's changed lines against the hot functions captured by the sidecar and flag diffs that touch runtime-critical code. Paid, runtime-backed.

### Richer MCP responses

Agents already query fallow via MCP, but the responses lack context agents need to make confident removal decisions: re-export chains, who imports this symbol, recent churn, duplicate siblings. Expand existing tool responses before adding new tools.

### Pre-commit hook install

`fallow check --changed` is fast enough to run on staged files. Ship a `fallow hooks install` command that wires it into husky, lefthook, or native `core.hooksPath`, scoped to unused exports and unresolved imports for sub-second feedback.

### Coverage sidecar ergonomics

The coverage setup state machine works end to end, but the install handoff still depends on users trusting a download. Target: reproducible sidecar pinning, smoother framework recipe generation, clearer failure messages when the sidecar cannot attach.

### Post-fix formatter integration

`fallow fix` leaves Prettier, dprint, or Biome to clean up whitespace after removals. Invoke the project's configured formatter automatically when running in-place.

---

## Vision

Broader bets, still being scoped.

### Agent-driven cleanup loop

Safe removals (unused exports, enum members, dependencies) are already auto-fixable. The open question is the judgment calls: deleting files, consolidating duplicates, restructuring modules. The bet: structured MCP output plus the right review workflow lets an agent propose those changes, a human approves the PR, and fallow verifies nothing regressed.

### Codebase health grade

One letter (A-F) per project, derived from dead code ratio, duplication, complexity density, and dependency hygiene. Visible as a badge, tracked in vital signs snapshots, trended over time. Managers understand it, developers trust it, agents optimize for it. The risk is that a single grade collapses signal the existing health score already surfaces more precisely; scoping needs to show it adds value over the current score.

### Visualization

`fallow viz`: a self-contained interactive HTML report. Treemap with dead code highlighted, dependency graph, cycle visualization, duplication heatmaps. No server, opens in any browser. Scoping depends on which view actually unblocks a user workflow rather than just looking good in screenshots.

---

## Ongoing

Continuous work across releases.

- **Incremental analysis** -- finer-grained caching for faster watch mode and CI on large monorepos
- **Plugin ecosystem** -- more framework coverage, better external plugin authoring, community-contributed plugins
- **Health intelligence** -- structured fix suggestions, HTML report cards, richer regression diffing
- **Agent integration** -- Claude Code hooks, Cursor integration, agent skill packages, expanded MCP coverage

---

## Known limitations

Acknowledged gaps. Fixes land opportunistically.

- **Syntactic analysis only** -- no TypeScript type information. Projects using `isolatedModules: true` (the modern default) are well-served; legacy tsc-only patterns may produce false positives.
- **Config parsing ceiling** -- AST-based extraction handles static configs. Computed values and conditionals are out of reach without JS eval.
- **Svelte export false negatives** -- props (`export let`) can't be distinguished from utility exports without Svelte compiler semantics.
- **NestJS/DI class members** -- abstract methods consumed via DI are not tracked. Use `unused_class_members = "off"` for DI-heavy projects.

---

[Open an issue](https://github.com/fallow-rs/fallow/issues) to request a feature or report a bug. PRs welcome: check the [contributing guide](CONTRIBUTING.md) and [issues labeled "good first issue"](https://github.com/fallow-rs/fallow/issues?q=label%3A%22good+first+issue%22).
