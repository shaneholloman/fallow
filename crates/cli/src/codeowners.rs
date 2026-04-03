//! CODEOWNERS file parser and ownership lookup.
//!
//! Parses GitHub/GitLab-style CODEOWNERS files and matches file paths
//! to their owners. Used by `--group-by owner` to group analysis output
//! by team ownership.
//!
//! # Pattern semantics
//!
//! CODEOWNERS patterns follow gitignore-like rules:
//! - `*.js` matches any `.js` file in any directory
//! - `/docs/*` matches files directly in `docs/` (root-anchored)
//! - `docs/` matches everything under `docs/`
//! - Last matching rule wins
//! - First owner on a multi-owner line is the primary owner

use std::path::Path;

use globset::{Glob, GlobSet, GlobSetBuilder};

/// Parsed CODEOWNERS file for ownership lookup.
#[derive(Debug)]
pub struct CodeOwners {
    /// Primary owner per rule, indexed by glob position in the `GlobSet`.
    owners: Vec<String>,
    /// Compiled glob patterns for matching.
    globs: GlobSet,
}

/// Standard locations to probe for a CODEOWNERS file, in priority order.
///
/// Order: root catch-all → GitHub → GitLab → GitHub legacy (`docs/`).
const PROBE_PATHS: &[&str] = &[
    "CODEOWNERS",
    ".github/CODEOWNERS",
    ".gitlab/CODEOWNERS",
    "docs/CODEOWNERS",
];

/// Label for files that match no CODEOWNERS rule.
pub const UNOWNED_LABEL: &str = "(unowned)";

impl CodeOwners {
    /// Load and parse a CODEOWNERS file from the given path.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Self::parse(&content)
    }

    /// Auto-probe standard CODEOWNERS locations relative to the project root.
    ///
    /// Tries `CODEOWNERS`, `.github/CODEOWNERS`, `.gitlab/CODEOWNERS`, `docs/CODEOWNERS`.
    pub fn discover(root: &Path) -> Result<Self, String> {
        for probe in PROBE_PATHS {
            let path = root.join(probe);
            if path.is_file() {
                return Self::from_file(&path);
            }
        }
        Err(format!(
            "no CODEOWNERS file found (looked for: {}). \
             Create one of these files or use --group-by directory instead",
            PROBE_PATHS.join(", ")
        ))
    }

    /// Load from a config-specified path, or auto-discover.
    pub fn load(root: &Path, config_path: Option<&str>) -> Result<Self, String> {
        if let Some(p) = config_path {
            let path = root.join(p);
            Self::from_file(&path)
        } else {
            Self::discover(root)
        }
    }

    /// Parse CODEOWNERS content into a lookup structure.
    fn parse(content: &str) -> Result<Self, String> {
        let mut builder = GlobSetBuilder::new();
        let mut owners = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut parts = line.split_whitespace();
            let Some(pattern) = parts.next() else {
                continue;
            };
            let Some(owner) = parts.next() else {
                continue; // Pattern without owners — skip
            };

            let glob_pattern = translate_pattern(pattern);
            let glob = Glob::new(&glob_pattern)
                .map_err(|e| format!("invalid CODEOWNERS pattern '{pattern}': {e}"))?;

            builder.add(glob);
            owners.push(owner.to_string());
        }

        let globs = builder
            .build()
            .map_err(|e| format!("failed to compile CODEOWNERS patterns: {e}"))?;

        Ok(Self { owners, globs })
    }

    /// Look up the primary owner of a file path (relative to project root).
    ///
    /// Returns the first owner from the last matching CODEOWNERS rule,
    /// or `None` if no rule matches.
    pub fn owner_of(&self, relative_path: &Path) -> Option<&str> {
        let matches = self.globs.matches(relative_path);
        // Last match wins: highest index = last rule in file order
        matches.iter().max().map(|&idx| self.owners[idx].as_str())
    }
}

/// Translate a CODEOWNERS pattern to a `globset`-compatible glob pattern.
///
/// CODEOWNERS uses gitignore-like semantics:
/// - Leading `/` anchors to root (stripped for globset)
/// - Trailing `/` means directory contents (`dir/` → `dir/**`)
/// - No `/` in pattern: matches in any directory (`*.js` → `**/*.js`)
/// - Contains `/` (non-trailing): root-relative as-is
fn translate_pattern(pattern: &str) -> String {
    // Strip leading `/` — globset matches from root by default
    let (anchored, rest) = if let Some(p) = pattern.strip_prefix('/') {
        (true, p)
    } else {
        (false, pattern)
    };

    // Trailing `/` means directory contents
    let expanded = if let Some(p) = rest.strip_suffix('/') {
        format!("{p}/**")
    } else {
        rest.to_string()
    };

    // If not anchored and no directory separator, match in any directory
    if !anchored && !expanded.contains('/') {
        format!("**/{expanded}")
    } else {
        expanded
    }
}

/// Extract the first path component for `--group-by directory` grouping.
///
/// Returns the first directory segment of a relative path.
/// For monorepo structures (`packages/auth/...`), returns `packages`.
pub fn directory_group(relative_path: &Path) -> &str {
    let s = relative_path.to_str().unwrap_or("");
    // Use forward-slash normalized path
    let s = if s.contains('\\') {
        // Windows paths: handled by caller normalizing, but be safe
        return s.split(['/', '\\']).next().unwrap_or(s);
    } else {
        s
    };

    match s.find('/') {
        Some(pos) => &s[..pos],
        None => s, // Root-level file
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── translate_pattern ──────────────────────────────────────────

    #[test]
    fn translate_bare_glob() {
        assert_eq!(translate_pattern("*.js"), "**/*.js");
    }

    #[test]
    fn translate_rooted_pattern() {
        assert_eq!(translate_pattern("/docs/*"), "docs/*");
    }

    #[test]
    fn translate_directory_pattern() {
        assert_eq!(translate_pattern("docs/"), "docs/**");
    }

    #[test]
    fn translate_rooted_directory() {
        assert_eq!(translate_pattern("/src/app/"), "src/app/**");
    }

    #[test]
    fn translate_path_with_slash() {
        assert_eq!(translate_pattern("src/utils/*.ts"), "src/utils/*.ts");
    }

    #[test]
    fn translate_double_star() {
        // Pattern already contains `/`, so it's root-relative — no extra prefix
        assert_eq!(translate_pattern("**/test_*.py"), "**/test_*.py");
    }

    #[test]
    fn translate_single_file() {
        assert_eq!(translate_pattern("Makefile"), "**/Makefile");
    }

    // ── parse ──────────────────────────────────────────────────────

    #[test]
    fn parse_simple_codeowners() {
        let content = "* @global-owner\n/src/ @frontend\n*.rs @rust-team\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owners.len(), 3);
    }

    #[test]
    fn parse_skips_comments_and_blanks() {
        let content = "# Comment\n\n* @owner\n  # Indented comment\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owners.len(), 1);
    }

    #[test]
    fn parse_multi_owner_takes_first() {
        let content = "*.ts @team-a @team-b @team-c\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owners[0], "@team-a");
    }

    #[test]
    fn parse_skips_pattern_without_owner() {
        let content = "*.ts\n*.js @owner\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owners.len(), 1);
        assert_eq!(co.owners[0], "@owner");
    }

    #[test]
    fn parse_empty_content() {
        let co = CodeOwners::parse("").unwrap();
        assert_eq!(co.owner_of(Path::new("anything.ts")), None);
    }

    // ── owner_of ───────────────────────────────────────────────────

    #[test]
    fn owner_of_last_match_wins() {
        let content = "* @default\n/src/ @frontend\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("src/app.ts")), Some("@frontend"));
    }

    #[test]
    fn owner_of_falls_back_to_catch_all() {
        let content = "* @default\n/src/ @frontend\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("README.md")), Some("@default"));
    }

    #[test]
    fn owner_of_no_match_returns_none() {
        let content = "/src/ @frontend\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("README.md")), None);
    }

    #[test]
    fn owner_of_extension_glob() {
        let content = "*.rs @rust-team\n*.ts @ts-team\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("src/lib.rs")), Some("@rust-team"));
        assert_eq!(
            co.owner_of(Path::new("packages/ui/Button.ts")),
            Some("@ts-team")
        );
    }

    #[test]
    fn owner_of_nested_directory() {
        let content = "* @default\n/packages/auth/ @auth-team\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(
            co.owner_of(Path::new("packages/auth/src/login.ts")),
            Some("@auth-team")
        );
        assert_eq!(
            co.owner_of(Path::new("packages/ui/Button.ts")),
            Some("@default")
        );
    }

    #[test]
    fn owner_of_specific_overrides_general() {
        // Later, more specific rule wins
        let content = "\
            * @default\n\
            /src/ @frontend\n\
            /src/api/ @backend\n\
        ";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(
            co.owner_of(Path::new("src/api/routes.ts")),
            Some("@backend")
        );
        assert_eq!(co.owner_of(Path::new("src/app.ts")), Some("@frontend"));
    }

    // ── directory_group ────────────────────────────────────────────

    #[test]
    fn directory_group_simple() {
        assert_eq!(directory_group(Path::new("src/utils/index.ts")), "src");
    }

    #[test]
    fn directory_group_root_file() {
        assert_eq!(directory_group(Path::new("index.ts")), "index.ts");
    }

    #[test]
    fn directory_group_monorepo() {
        assert_eq!(
            directory_group(Path::new("packages/auth/src/login.ts")),
            "packages"
        );
    }

    // ── discover ───────────────────────────────────────────────────

    #[test]
    fn discover_nonexistent_root() {
        let result = CodeOwners::discover(Path::new("/nonexistent/path"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("no CODEOWNERS file found"));
        assert!(err.contains("--group-by directory"));
    }

    // ── from_file ──────────────────────────────────────────────────

    #[test]
    fn from_file_nonexistent() {
        let result = CodeOwners::from_file(Path::new("/nonexistent/CODEOWNERS"));
        assert!(result.is_err());
    }

    #[test]
    fn from_file_real_codeowners() {
        // Use the project's own CODEOWNERS file
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let path = root.join(".github/CODEOWNERS");
        if path.exists() {
            let co = CodeOwners::from_file(&path).unwrap();
            // Our CODEOWNERS has `* @bartwaardenburg`
            assert_eq!(
                co.owner_of(Path::new("src/anything.ts")),
                Some("@bartwaardenburg")
            );
        }
    }

    // ── edge cases ─────────────────────────────────────────────────

    #[test]
    fn email_owner() {
        let content = "*.js user@example.com\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("index.js")), Some("user@example.com"));
    }

    #[test]
    fn team_owner() {
        let content = "*.ts @org/frontend-team\n";
        let co = CodeOwners::parse(content).unwrap();
        assert_eq!(co.owner_of(Path::new("app.ts")), Some("@org/frontend-team"));
    }
}
