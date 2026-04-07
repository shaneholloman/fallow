---
paths:
  - "crates/cli/**"
---

# fallow-cli crate

Key modules:
- `main.rs` — CLI definition (clap) + command dispatch
- `error.rs` — Structured error output (`emit_error`): JSON on stdout when `--format json`, stderr otherwise
- `audit.rs` — Audit command: combined dead-code + complexity + duplication for changed files, verdict (pass/warn/fail)
- `check.rs` — Analysis pipeline, tracing, filtering, output
- `dupes.rs` — Duplication detection, baseline, cross-reference
- `health/` — Complexity analysis: `mod.rs` (orchestration), `scoring.rs`, `hotspots.rs`, `targets.rs`
- `watch.rs` — File watcher with debounced re-analysis
- `fix/` — Auto-fix: `exports.rs`, `enum_members.rs`, `deps.rs`, `io.rs` (atomic writes)
- `codeowners.rs` — CODEOWNERS file parser, ownership lookup for `--group-by owner`
- `report/` — Output formatting: `mod.rs` (dispatch), `grouping.rs` (ownership resolver, result partitioning), `human/` (check, dupes, health, perf, traces), `json.rs`, `sarif.rs`, `compact.rs`, `markdown.rs`, `codeclimate.rs`
- `migrate/` — Config migration from knip/jscpd
- `init.rs` — Generate config files (`.fallowrc.json` or `fallow.toml`), scaffold pre-commit git hooks (`--hooks`)
- `list.rs` — Show active plugins, entry points, files, boundary zones/rules (`--boundaries`)
- `schema.rs` — `schema`, `config-schema`, `plugin-schema` commands
- `explain.rs` — Metric/rule definitions, JSON `_meta` builders, SARIF `fullDescription`/`helpUri` source, docs URLs
- `validate.rs` — Input validation (control characters, path sanitization)
- `regression/` — Regression testing: `tolerance.rs` (thresholds), `counts.rs` (baselines), `outcome.rs` (verdict), `baseline.rs` (save/load/compare)

## Environment variables
- `FALLOW_FORMAT` — default output format
- `FALLOW_QUIET` — suppress progress bars
- `FALLOW_BIN` — binary path for MCP server

## JSON error format
Structured JSON errors on stdout when `--format json` is active: `{"error": true, "message": "...", "exit_code": 2}`
