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
- `api.rs`: shared HTTP layer for fallow-cloud backend calls. Exposes `api_agent()` / `api_agent_with_timeout()` (ureq builder, `http_status_as_error(false)`), `api_url()` (respects `FALLOW_API_URL`), `ErrorEnvelope` (typed `{code, message}`), `actionable_error_hint()`, `http_status_message()`, `ResponseBodyReader` trait, and the `NETWORK_EXIT_CODE = 7` constant. Used by `license/` (5s/10s timeouts) and `coverage/upload_inventory` (5s/30s timeouts).
- `license/` — `license activate|status|refresh|deactivate` subcommands. `activate` accepts JWT via positional arg, `--from-file`, or stdin (`-`); `--trial --email <addr>` issues a 30-day trial in one step. On Unix the stored license file is written with mode `0600`. The trial response's `trialEndsAt` is surfaced on stdout after activation. `status` prints a refresh hint when the JWT's `refresh_after` claim has passed. `refresh` and `--trial` hit `api.fallow.cloud` via the shared `api.rs` layer; failures exit `7`. Wraps `fallow-license` (offline Ed25519 verify, alg pinned, RS256/none rejected, 7/30/hard-fail grace ladder, optional `refresh_after` claim).
- `coverage/` — paid Production Coverage subtree. Two subcommands today:
  - `coverage setup`: resumable first-run state machine (license → sidecar install → framework-aware recipe → auto-handoff to `fallow health --production-coverage`).
  - `coverage upload-inventory`: POSTs a static function inventory to `/v1/coverage/{repo}/inventory` via the shared `api.rs` layer. Flags: `--api-key` (or `$FALLOW_API_KEY`), `--api-endpoint`, `--project-id` (default: `$GITHUB_REPOSITORY` → `$CI_PROJECT_PATH` → parsed origin URL), `--git-sha` (default: `git rev-parse HEAD`), `--allow-dirty` (escape hatch: proceed with a dirty working tree even though the inventory then reflects the working copy rather than a SHA-exact commit), `--exclude-paths` (repeatable glob), `--path-prefix` (prepended to emitted paths for containerized deployments where runtime reports absolute paths like `/app/src/foo.ts`), `--dry-run`, `--ignore-upload-errors` (soft-fails only transport/server errors; auth remains fatal). Walks the project with `fallow_extract::inventory::walk_source`, emitting Istanbul/`oxc-coverage-instrument`-compatible names (per-file counter, bodyless functions and `.d.ts` files skipped). The current cloud join key is only `(filePath, functionName)`, so the CLI rejects uploads when one file contains multiple distinct functions with the same emitted name. Server returns `pathOverlap` on the upload response; CLI prints a yellow warning when matched/sampled < 50%. Exit codes: 0 ok · 7 network · 10 validation · 11 payload too large · 12 auth rejected · 13 server error. The ONLY fallow subcommand that does network I/O outside of `license`; `check`/`dupes`/`health` stay offline.
- `explain.rs` — Metric/rule definitions, JSON `_meta` builders, SARIF `fullDescription`/`helpUri` source, docs URLs
- `validate.rs` — Input validation (control characters, path sanitization)
- `regression/` — Regression testing: `tolerance.rs` (thresholds), `counts.rs` (baselines), `outcome.rs` (verdict), `baseline.rs` (save/load/compare)
- `lib.rs` — Library surface for the `fallow-cli` crate. Re-exports `runtime_support::{AnalysisKind, GroupBy}` and the programmatic facade (`report::{build_json, build_health_json, build_duplication_json, build_baseline_deltas_json}`, `explain::*`, `health_types`, `regression`, `codeowners`). The binary (`main.rs`) still owns clap + dispatch; the library is consumed by `crates/napi` (Node bindings) and any out-of-tree embedders.
- `runtime_support.rs` — Shared `build_ownership_resolver` + `load_config` used by both `main.rs` and `programmatic.rs`, plus the `AnalysisKind` / `GroupBy` clap enums. Extracted out of `main.rs` so the library can reuse them without dragging in the full clap command tree.
- `programmatic.rs` — One-shot Rust API reused by the NAPI bindings. Exposes `detect_dead_code`, `detect_circular_dependencies`, `detect_boundary_violations`, `detect_duplication`, `compute_complexity`, and `compute_health`, each returning a `serde_json::Value` whose shape matches the CLI's `--format json` contract (`schema_version`, `summary`, relative paths, injected `actions`, optional `_meta` under `--explain`). Wraps `check::execute_check`, `dupes::execute_dupes`, and `health::execute_health` with a quiet-mode, no-side-effect harness. Structured `ProgrammaticError { message, exit_code, code, help, context }` is the surface errors, preserving the CLI's exit-code ladder (0 ok · 2 generic · 7 network · etc.). Used by `crates/napi` via `pub use fallow_cli::programmatic`.

## Environment variables
- `FALLOW_FORMAT` — default output format
- `FALLOW_QUIET` — suppress progress bars
- `FALLOW_BIN` — binary path for MCP server
- `FALLOW_COVERAGE` — path to Istanbul coverage data for accurate CRAP scores
- `FALLOW_LICENSE` — license JWT (full string). First-class storage path; intended for shared CI runners.
- `FALLOW_LICENSE_PATH` — file path containing the license JWT.
- `FALLOW_COV_BIN` — explicit override for the closed-source `fallow-cov` sidecar binary (wins over project-local `node_modules/.bin`, package-manager `bin`, `~/.fallow/bin/`, and `PATH`). When set but the path is not a file, sidecar discovery fails fast with a targeted error rather than silently falling through.
- `FALLOW_API_URL`: base URL for fallow cloud API calls (license refresh, trial, inventory upload). Trailing slashes are trimmed. Used for staging / local-dev overrides.
- `FALLOW_API_KEY`: fallow cloud bearer token. Consumed by `fallow coverage upload-inventory` (flag `--api-key` wins).

## JSON error format
Structured JSON errors on stdout when `--format json` is active: `{"error": true, "message": "...", "exit_code": 2}`
