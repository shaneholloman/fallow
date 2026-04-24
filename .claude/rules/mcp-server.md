---
paths:
  - "crates/mcp/**"
---

# fallow-mcp crate

MCP server exposing fallow analysis as tools for AI agents. Stdio transport, wraps `fallow` CLI via subprocess.

## Tools (11 total)
- `analyze` - full dead code analysis (`fallow dead-code --format json`), supports `boundary_violations` convenience param
- `check_changed` - incremental analysis (`fallow dead-code --changed-since`)
- `find_dupes` - code duplication (`fallow dupes --format json`), supports `changed_since`
- `check_health` - complexity metrics (`fallow health --format json`), supports `file_scores`, `hotspots`, `targets`, `since`, `min_commits`, `production_coverage` (paid, forwards `--production-coverage <path>`), `min_invocations_hot` (hot-path threshold), `min_observation_volume` (high-confidence verdict floor), `low_traffic_threshold` (active/low_traffic split), `max_crap` (per-function CRAP threshold, default 30.0; forwards `--max-crap <N>`) params
- `check_production_coverage` - focused paid production-coverage entry point (`fallow health --production-coverage <path>`). Required `coverage` param (V8 dir, V8 JSON, or Istanbul JSON). Tuning: `min_invocations_hot` (default 100), `min_observation_volume` (default 5000), `low_traffic_threshold` (default 0.001), `max_crap` (default 30.0), `group_by` (`owner`/`directory`/`package`/`section`). Raise `FALLOW_TIMEOUT_SECS` for multi-megabyte dumps. Pick this over `check_health` when you have a V8 or Istanbul coverage dump and want surfaced dead-in-production verdicts.
- `audit` - combined dead-code + complexity + duplication for changed files, returns verdict (`fallow audit --format json`). Supports `max_crap` (forwards `--max-crap <N>` to the health sub-analysis).
- `fix_preview` - dry-run auto-fix (`fallow fix --dry-run --format json`)
- `fix_apply` - apply auto-fixes (`fallow fix --yes --format json`), destructive
- `project_info` - project metadata (`fallow list --format json`), supports section params (`entry_points`, `files`, `plugins`, `boundaries`)
- `list_boundaries` - architecture boundary zones and rules (`fallow list --boundaries --format json`)
- `feature_flags` - detect feature flag patterns (`fallow flags --format json`), supports `flag_type`, `confidence`, `dead_code_only` params

## Global flags (available on all tools)
- `no_cache` (bool) — disable incremental parse cache
- `threads` (usize) — parser thread count

## Flags on analysis tools (analyze, check_changed, find_dupes, check_health)
- `baseline` (string) — compare against saved baseline
- `save_baseline` (string) — save results as baseline

## Error handling
- Subprocess timeout: 120s default, configurable via `FALLOW_TIMEOUT_SECS` env var
- Exit code 2+ errors: pass through CLI's structured JSON error from stdout when available; fall back to `{"error":true,"message":"...","exit_code":N}` from stderr
- Exit code 1: treated as success (issues found, not an error)

## Actions injection
All JSON output includes structured `actions` arrays on every finding:
- Dead-code issues: fix action + suppress action (via `inject_actions` in `report/json.rs`)
- Health findings: `refactor-function` + suppress (via `inject_health_actions`)
- Health targets: `apply-refactoring` + suppress when evidence exists
- Dupes families: `extract-shared` + suggestions + suppress (via `inject_dupes_actions`)
- Dupes groups: `extract-shared` + suppress
- Audit: inherits actions from all three sub-analyses

All params structs derive `Default` for ergonomic test construction except `CheckChangedParams` and `CheckProductionCoverageParams`, which each have one required non-default field (`since` and `coverage` respectively). Their test helpers (`check_changed("main")`, `check_production_coverage("./coverage")`) substitute for `Default::default()`.

Built with `rmcp` (official Rust MCP SDK). Thin subprocess wrapper — all analysis logic stays in the CLI.
- `FALLOW_BIN` — binary path (defaults to sibling binary or `fallow` in PATH)
- `FALLOW_TIMEOUT_SECS` — subprocess timeout in seconds (default: 120)
