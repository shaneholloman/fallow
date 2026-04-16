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
- `health/` — Complexity analysis: `mod.rs` (orchestration), `scoring.rs`, `hotspots.rs`, `targets.rs`, `ownership.rs` (bus factor, drift, declared owner cross-ref for `--ownership`)
- `watch.rs` — File watcher with debounced re-analysis
- `fix/` — Auto-fix: `exports.rs`, `enum_members.rs`, `deps.rs`, `io.rs` (atomic writes)
- `codeowners.rs` — CODEOWNERS file parser, ownership lookup for `--group-by owner`
- `report/` — Output formatting: `mod.rs` (dispatch), `grouping.rs` (ownership resolver, result partitioning), `human/` (check, dupes, health, perf, traces), `json.rs`, `sarif.rs`, `compact.rs`, `markdown.rs`, `codeclimate.rs`
- `migrate/` — Config migration from knip/jscpd
- `init.rs` — Generate config files (`.fallowrc.json` or `fallow.toml`), scaffold pre-commit git hooks (`--hooks`)
- `list.rs` — Show active plugins, entry points, files, boundary zones/rules (`--boundaries`)
- `schema.rs` — `schema`, `config-schema`, `plugin-schema` commands
- `config.rs` — `config` subcommand: prints loaded config path + JSON resolved config (or `--path` only). Honors global `--config <path>`.
- `license/` — `license activate|status|refresh|deactivate` subcommands. `activate` accepts JWT via positional arg, `--from-file`, or stdin (`-`); `--trial --email <addr>` issues a 30-day trial in one step. On Unix the stored license file is written with mode `0600`. The trial response's `trialEndsAt` is surfaced on stdout after activation. `status` prints a refresh hint when the JWT's `refresh_after` claim has passed. `refresh` and `--trial` hit `api.fallow.cloud` via `ureq` with a 5s connect / 10s total timeout; failures exit `7`. Wraps `fallow-license` (offline Ed25519 verify, alg pinned, RS256/none rejected, 7/30/hard-fail grace ladder, optional `refresh_after` claim).
- `coverage/` — `coverage setup` resumable first-run state inspector for production coverage. Today: license + sidecar discovery report. Future: framework-aware recipe + auto-resume to analysis.
- `explain.rs` — Metric/rule definitions, JSON `_meta` builders, SARIF `fullDescription`/`helpUri` source, docs URLs
- `validate.rs` — Input validation (control characters, path sanitization)
- `regression/` — Regression testing: `tolerance.rs` (thresholds), `counts.rs` (baselines), `outcome.rs` (verdict), `baseline.rs` (save/load/compare)

## Environment variables
- `FALLOW_FORMAT` — default output format
- `FALLOW_QUIET` — suppress progress bars
- `FALLOW_BIN` — binary path for MCP server
- `FALLOW_COVERAGE` — path to Istanbul coverage data for accurate CRAP scores
- `FALLOW_LICENSE` — license JWT (full string). First-class storage path; intended for shared CI runners.
- `FALLOW_LICENSE_PATH` — file path containing the license JWT.
- `FALLOW_COV_BIN` — explicit override for the closed-source `fallow-cov` sidecar binary (wins over project-local `node_modules/.bin`, package-manager `bin`, `~/.fallow/bin/`, and `PATH`). When set but the path is not a file, sidecar discovery fails fast with a targeted error rather than silently falling through.

## JSON error format
Structured JSON errors on stdout when `--format json` is active: `{"error": true, "message": "...", "exit_code": 2}`
