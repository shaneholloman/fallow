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
- `changedSince` (string): git ref. When non-empty, results and duplication reports are filtered to files changed since the ref via `fallow_core::changed_files::{try_get_changed_files, filter_results_by_changed_files, filter_duplication_by_changed_files}`. Mirrors the CLI's `--changed-since`. Resolution runs inside `spawn_blocking`; success/failure is surfaced via `log_message`.
