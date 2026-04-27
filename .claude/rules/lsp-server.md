---
paths:
  - "crates/lsp/**"
---

# fallow-lsp crate

Key modules:
- `main.rs` — LSP server setup, `LanguageServer` trait impl, event handling
- `diagnostics/` — Diagnostic generation: `mod.rs` (dispatch), `unused.rs`, `structural.rs`, `quality.rs`
- `code_actions.rs` — Quick-fix and refactor code actions
- `code_lens.rs` — Reference count Code Lens above export declarations
- `hover.rs` — Hover information showing export usage, unused status, and duplicate block locations

## initializationOptions
- `issueTypes` (object): per-issue-type toggles using kebab-case keys, mapped to diagnostic codes by `ISSUE_TYPE_TO_DIAGNOSTIC_CODE`. Disabled types are filtered out before publishing.
- `changedSince` (string): git ref. When non-empty, results and duplication reports are filtered to files changed since the ref via `fallow_core::changed_files::{try_get_changed_files_with_toplevel, filter_results_by_changed_files, filter_duplication_by_changed_files}`. Mirrors the CLI's `--changed-since`. Resolution runs inside `spawn_blocking`; success/failure is surfaced via `log_message`. Path joins use the canonical git toplevel resolved via `git rev-parse --show-toplevel` (cached on the server), so the filter works correctly when the LSP workspace is a subdirectory of the git repo (issue #190).

## Diagnostic.data
When the `changedSince` filter is active, every published `Diagnostic` carries `data: { "changedSince": "<git_ref>" }` (standard LSP `Diagnostic.data` slot). Set centrally via `attach_changed_since_data` after `build_diagnostics`. AI agents reading via `vscode.languages.getDiagnostics()` can use this payload to verify the filter is on and avoid acting on baseline-excluded findings. No `data` is set when the filter is unset.

## Server capabilities
`build_server_capabilities()` (in `main.rs`) is the single source of truth for the `ServerCapabilities` returned from `initialize`. It advertises `text_document_sync`, `code_action_provider`, `code_lens_provider`, `hover_provider`, and `diagnostic_provider`. The `diagnostic_provider` (LSP 3.17 pull-model) advertisement is required for strict 3.17 clients (Helix, Zed) to call `textDocument/diagnostic`; without it the handler registered via `.custom_method(...)` and the `cached_diagnostics` cache are dead code for those clients. `inter_file_dependencies = true` (cross-file findings like unused exports / unused dependencies); `workspace_diagnostics = false` (no `workspace/diagnostic` handler).

## Cross-root dedup
`find_project_roots` returns the workspace root plus each sub-package; `merge_results` `.extend()`s without dedup, so overlapping roots (e.g. workspace root + `apps/web` sub-package both walking `apps/web/src/foo.ts`) accumulate the same finding N times. `dedup_results` runs after the project-root loop and before the `changedSince` filter; identity keys are per-type (path + line + col for file-scoped issues, package_name + path for deps, sorted-files for cycles). `UnlistedDependency` is the one type that gets a real merge instead of plain dedup: `imported_from` site lists are unioned across roots so the user sees a single entry per package with the combined import view.
