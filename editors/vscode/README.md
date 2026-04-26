# Fallow for VS Code

Find unused code, circular dependencies, and code duplication in TypeScript/JavaScript projects. Powered by [fallow](https://docs.fallow.tools), a Rust-native alternative to knip that is 5-41x faster than knip v5 (2-18x faster than knip v6).

## Features

- **Real-time diagnostics** via the fallow LSP server: unused files, exports, types, dependencies, enum/class members, unresolved imports, unlisted deps, duplicate exports, circular dependencies, and code duplication
- **Quick-fix code actions**: remove unused exports, delete unused files
- **Refactor code actions**: extract duplicate code into a shared function
- **Code Lens**: reference counts above each export declaration with click-to-navigate (opens Peek References panel)
- **Hover information**: export usage status, unused status, and duplicate block locations
- **Tree views**: browse unused code by issue type and duplicates by clone family in the sidebar
- **Status bar**: see total issue count and duplication percentage at a glance
- **Auto-fix**: remove unused exports, dependencies, and enum members with one command
- **Auto-download**: the extension downloads the `fallow-lsp` binary automatically

## Installation

### From the Marketplace

Search for "Fallow" in the VS Code extensions panel, or install from the command line:

```sh
code --install-extension fallow-rs.fallow-vscode
```

### Manual

1. Install the `fallow` npm package or the standalone `fallow` / `fallow-lsp` binaries (see [fallow installation](https://docs.fallow.tools/installation))
2. Install the extension VSIX file: `code --install-extension fallow-vscode-*.vsix`

## Commands

| Command | Description |
|---------|-------------|
| `Fallow: Run Analysis` | Run full codebase analysis and update tree views |
| `Fallow: Auto-Fix Unused Exports & Dependencies` | Remove unused exports and dependencies |
| `Fallow: Preview Fixes (Dry Run)` | Show what fixes would be applied without changing files |
| `Fallow: Restart Language Server` | Restart the fallow-lsp process |
| `Fallow: Show Output Channel` | Open the Fallow output panel for debugging |

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `fallow.lspPath` | `""` | Path to the `fallow-lsp` binary. Leave empty for auto-detection. |
| `fallow.autoDownload` | `true` | Automatically download the binary if not found. |
| `fallow.issueTypes` | all enabled | Toggle individual issue types on/off. |
| `fallow.duplication.threshold` | `5` | Minimum number of lines for a code block to be reported as a duplicate. |
| `fallow.duplication.mode` | `"mild"` | Detection mode: `strict`, `mild`, `weak`, or `semantic`. |
| `fallow.production` | `false` | Production mode: exclude test/dev files, only production scripts. |
| `fallow.changedSince` | `""` | Git ref (tag, branch, or SHA) to scope the Problems panel and sidebar to files changed since that ref, mirroring the CLI's `--changed-since`. Tag your current commit (e.g. `fallow-baseline`) and set this to the tag to enforce "no new issues going forward" while ignoring pre-existing findings. |
| `fallow.trace.server` | `"off"` | LSP trace level: `off`, `messages`, or `verbose`. |

## Binary resolution

The extension looks for the `fallow-lsp` binary in this order:

1. `fallow.lspPath` setting (if configured)
2. Local `node_modules/.bin/fallow-lsp`
3. `fallow-lsp` in `PATH`
4. Previously downloaded binary in extension storage
5. Auto-download from GitHub releases (if `fallow.autoDownload` is enabled)

## Development

```sh
cd editors/vscode
pnpm install
pnpm build           # Production build
pnpm watch           # Watch mode for development
pnpm lint            # Type check
pnpm test            # Unit + extension-host tests
pnpm package         # Package as .vsix
```
