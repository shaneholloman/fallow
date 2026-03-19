use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// A warning about a config field that could not be migrated.
struct MigrationWarning {
    source: &'static str,
    field: String,
    message: String,
    suggestion: Option<String>,
}

impl std::fmt::Display for MigrationWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] `{}`: {}", self.source, self.field, self.message)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " (suggestion: {suggestion})")?;
        }
        Ok(())
    }
}

/// Result of migrating one or more source configs.
struct MigrationResult {
    config: serde_json::Value,
    warnings: Vec<MigrationWarning>,
    sources: Vec<String>,
}

/// Run the migrate command.
pub(crate) fn run_migrate(
    root: &Path,
    use_toml: bool,
    dry_run: bool,
    from: Option<PathBuf>,
) -> ExitCode {
    // Check if a fallow config already exists
    let existing_names = ["fallow.jsonc", "fallow.json", "fallow.toml", ".fallow.toml"];
    if !dry_run {
        for name in &existing_names {
            let path = root.join(name);
            if path.exists() {
                eprintln!(
                    "Error: {name} already exists. Remove it first or use --dry-run to preview."
                );
                return ExitCode::from(2);
            }
        }
    }

    let result = if let Some(ref from_path) = from {
        migrate_from_file(from_path)
    } else {
        migrate_auto_detect(root)
    };

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::from(2);
        }
    };

    if result.sources.is_empty() {
        eprintln!("No knip or jscpd configuration found to migrate.");
        return ExitCode::from(2);
    }

    // Generate output
    let output_content = if use_toml {
        generate_toml(&result)
    } else {
        generate_jsonc(&result)
    };

    if dry_run {
        println!("{output_content}");
    } else {
        let filename = if use_toml {
            "fallow.toml"
        } else {
            "fallow.jsonc"
        };
        let output_path = root.join(filename);
        if let Err(e) = std::fs::write(&output_path, &output_content) {
            eprintln!("Error: failed to write {filename}: {e}");
            return ExitCode::from(2);
        }
        eprintln!("Created {filename}");
    }

    // Print source info
    for source in &result.sources {
        eprintln!("Migrated from: {source}");
    }

    // Print warnings
    if !result.warnings.is_empty() {
        eprintln!();
        eprintln!("Warnings ({} skipped fields):", result.warnings.len());
        for warning in &result.warnings {
            eprintln!("  {warning}");
        }
    }

    ExitCode::SUCCESS
}

/// Auto-detect and migrate from knip and/or jscpd configs in the given root.
fn migrate_auto_detect(root: &Path) -> Result<MigrationResult, String> {
    let mut config = serde_json::Map::new();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();

    // Try knip configs
    let knip_files = [
        "knip.json",
        "knip.jsonc",
        ".knip.json",
        ".knip.jsonc",
        "knip.ts",
        "knip.config.ts",
    ];

    for name in &knip_files {
        let path = root.join(name);
        if path.exists() {
            if name.ends_with(".ts") {
                warnings.push(MigrationWarning {
                    source: "knip",
                    field: name.to_string(),
                    message: format!(
                        "TypeScript config files ({name}) cannot be parsed. \
                         Convert to knip.json first, then re-run migrate."
                    ),
                    suggestion: None,
                });
                continue;
            }
            let knip_value = load_json_or_jsonc(&path)?;
            migrate_knip(&knip_value, &mut config, &mut warnings);
            sources.push(name.to_string());
            break; // Only use the first knip config found
        }
    }

    // Try jscpd standalone config
    let mut found_jscpd_file = false;
    let jscpd_path = root.join(".jscpd.json");
    if jscpd_path.exists() {
        let jscpd_value = load_json_or_jsonc(&jscpd_path)?;
        migrate_jscpd(&jscpd_value, &mut config, &mut warnings);
        sources.push(".jscpd.json".to_string());
        found_jscpd_file = true;
    }

    // Check package.json for embedded knip/jscpd config (single read)
    let need_pkg_knip = sources.is_empty();
    let need_pkg_jscpd = !found_jscpd_file;
    if need_pkg_knip || need_pkg_jscpd {
        let pkg_path = root.join("package.json");
        if pkg_path.exists() {
            let pkg_content = std::fs::read_to_string(&pkg_path)
                .map_err(|e| format!("failed to read package.json: {e}"))?;
            let pkg_value: serde_json::Value = serde_json::from_str(&pkg_content)
                .map_err(|e| format!("failed to parse package.json: {e}"))?;
            if need_pkg_knip && let Some(knip_config) = pkg_value.get("knip") {
                migrate_knip(knip_config, &mut config, &mut warnings);
                sources.push("package.json (knip key)".to_string());
            }
            if need_pkg_jscpd && let Some(jscpd_config) = pkg_value.get("jscpd") {
                migrate_jscpd(jscpd_config, &mut config, &mut warnings);
                sources.push("package.json (jscpd key)".to_string());
            }
        }
    }

    Ok(MigrationResult {
        config: serde_json::Value::Object(config),
        warnings,
        sources,
    })
}

/// Migrate from a specific config file.
fn migrate_from_file(path: &Path) -> Result<MigrationResult, String> {
    if !path.exists() {
        return Err(format!("config file not found: {}", path.display()));
    }

    let mut config = serde_json::Map::new();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if filename.contains("knip") {
        if filename.ends_with(".ts") {
            return Err(format!(
                "TypeScript config files ({filename}) cannot be parsed. \
                 Convert to knip.json first, then re-run migrate."
            ));
        }
        let knip_value = load_json_or_jsonc(path)?;
        migrate_knip(&knip_value, &mut config, &mut warnings);
        sources.push(path.display().to_string());
    } else if filename.contains("jscpd") {
        let jscpd_value = load_json_or_jsonc(path)?;
        migrate_jscpd(&jscpd_value, &mut config, &mut warnings);
        sources.push(path.display().to_string());
    } else if filename == "package.json" {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        let pkg_value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
        if let Some(knip_config) = pkg_value.get("knip") {
            migrate_knip(knip_config, &mut config, &mut warnings);
            sources.push(format!("{} (knip key)", path.display()));
        }
        if let Some(jscpd_config) = pkg_value.get("jscpd") {
            migrate_jscpd(jscpd_config, &mut config, &mut warnings);
            sources.push(format!("{} (jscpd key)", path.display()));
        }
        if sources.is_empty() {
            return Err(format!(
                "no knip or jscpd configuration found in {}",
                path.display()
            ));
        }
    } else {
        // Try to detect format from content
        let value = load_json_or_jsonc(path)?;
        // If it has knip-like fields, treat as knip
        if value.get("entry").is_some()
            || value.get("ignore").is_some()
            || value.get("rules").is_some()
            || value.get("project").is_some()
            || value.get("ignoreDependencies").is_some()
        {
            migrate_knip(&value, &mut config, &mut warnings);
            sources.push(path.display().to_string());
        }
        // If it has jscpd-like fields, treat as jscpd
        else if value.get("minTokens").is_some()
            || value.get("minLines").is_some()
            || value.get("threshold").is_some()
            || value.get("mode").is_some()
        {
            migrate_jscpd(&value, &mut config, &mut warnings);
            sources.push(path.display().to_string());
        } else {
            return Err(format!(
                "could not determine config format for {}",
                path.display()
            ));
        }
    }

    Ok(MigrationResult {
        config: serde_json::Value::Object(config),
        warnings,
        sources,
    })
}

/// Load a JSON or JSONC file, stripping comments if present.
fn load_json_or_jsonc(path: &Path) -> Result<serde_json::Value, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    // Try plain JSON first
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
        return Ok(value);
    }

    // Try stripping comments (JSONC)
    let mut stripped = String::new();
    json_comments::StripComments::new(content.as_bytes())
        .read_to_string(&mut stripped)
        .map_err(|e| format!("failed to strip comments from {}: {e}", path.display()))?;

    serde_json::from_str(&stripped).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

/// Extract a string-or-array field as a Vec<String>.
fn string_or_array(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

// ── Knip migration ──────────────────────────────────────────────

/// Knip rule names mapped to fallow rule names.
const KNIP_RULE_MAP: &[(&str, &str)] = &[
    ("files", "unusedFiles"),
    ("dependencies", "unusedDependencies"),
    ("devDependencies", "unusedDevDependencies"),
    ("exports", "unusedExports"),
    ("types", "unusedTypes"),
    ("enumMembers", "unusedEnumMembers"),
    ("classMembers", "unusedClassMembers"),
    ("unlisted", "unlistedDependencies"),
    ("unresolved", "unresolvedImports"),
    ("duplicates", "duplicateExports"),
];

/// Knip fields that cannot be mapped and generate warnings.
const KNIP_UNMAPPABLE_FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("project", "Fallow auto-discovers project files", None),
    (
        "paths",
        "Fallow reads path mappings from tsconfig.json automatically",
        None,
    ),
    (
        "ignoreFiles",
        "No separate concept in fallow",
        Some("use `ignore` patterns instead"),
    ),
    (
        "ignoreBinaries",
        "Binary filtering is not configurable in fallow",
        None,
    ),
    (
        "ignoreMembers",
        "Member-level ignoring is not configurable in fallow",
        Some("use inline suppression comments: // fallow-ignore-next-line"),
    ),
    (
        "ignoreUnresolved",
        "Unresolved import filtering is not configurable in fallow",
        Some("use inline suppression comments: // fallow-ignore-next-line unresolved-import"),
    ),
    ("ignoreExportsUsedInFile", "No equivalent in fallow", None),
    (
        "ignoreWorkspaces",
        "Workspace filtering is not configurable per-workspace",
        Some("use --workspace flag to scope output to a single package"),
    ),
    (
        "ignoreIssues",
        "No global issue ignoring in fallow",
        Some("use inline suppression comments: // fallow-ignore-file [issue-type]"),
    ),
    (
        "includeEntryExports",
        "Entry export inclusion is not configurable in fallow",
        None,
    ),
    (
        "tags",
        "Tag-based filtering is not supported in fallow",
        None,
    ),
    (
        "compilers",
        "Custom compilers are not supported in fallow (uses Oxc parser)",
        None,
    ),
    ("treatConfigHintsAsErrors", "No equivalent in fallow", None),
];

/// Knip issue type names that have no fallow equivalent.
const KNIP_UNMAPPABLE_ISSUE_TYPES: &[&str] = &[
    "optionalPeerDependencies",
    "binaries",
    "nsExports",
    "nsTypes",
    "catalog",
];

/// Known knip plugin config keys (framework-specific). These are auto-detected by fallow plugins.
const KNIP_PLUGIN_KEYS: &[&str] = &[
    "angular",
    "astro",
    "ava",
    "babel",
    "biome",
    "capacitor",
    "changesets",
    "commitizen",
    "commitlint",
    "cspell",
    "cucumber",
    "cypress",
    "docusaurus",
    "drizzle",
    "eleventy",
    "eslint",
    "expo",
    "gatsby",
    "github-actions",
    "graphql-codegen",
    "husky",
    "jest",
    "knex",
    "lefthook",
    "lint-staged",
    "markdownlint",
    "mocha",
    "moonrepo",
    "msw",
    "nest",
    "next",
    "node-test-runner",
    "npm-package-json-lint",
    "nuxt",
    "nx",
    "nyc",
    "oclif",
    "playwright",
    "postcss",
    "prettier",
    "prisma",
    "react-cosmos",
    "react-router",
    "release-it",
    "remark",
    "remix",
    "rollup",
    "rspack",
    "semantic-release",
    "sentry",
    "simple-git-hooks",
    "size-limit",
    "storybook",
    "stryker",
    "stylelint",
    "svelte",
    "syncpack",
    "tailwind",
    "tsup",
    "tsx",
    "typedoc",
    "typescript",
    "unbuild",
    "unocss",
    "vercel-og",
    "vite",
    "vitest",
    "vue",
    "webpack",
    "wireit",
    "wrangler",
    "xo",
    "yorkie",
];

fn migrate_knip(
    knip: &serde_json::Value,
    config: &mut serde_json::Map<String, serde_json::Value>,
    warnings: &mut Vec<MigrationWarning>,
) {
    let obj = match knip.as_object() {
        Some(o) => o,
        None => {
            warnings.push(MigrationWarning {
                source: "knip",
                field: "(root)".to_string(),
                message: "expected an object, got something else".to_string(),
                suggestion: None,
            });
            return;
        }
    };

    // entry → entry
    if let Some(entry_val) = obj.get("entry") {
        let entries = string_or_array(entry_val);
        if !entries.is_empty() {
            config.insert(
                "entry".to_string(),
                serde_json::Value::Array(
                    entries.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }
    }

    // ignore → ignore
    if let Some(ignore_val) = obj.get("ignore") {
        let ignores = string_or_array(ignore_val);
        if !ignores.is_empty() {
            config.insert(
                "ignore".to_string(),
                serde_json::Value::Array(
                    ignores.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }
    }

    // ignoreDependencies → ignoreDependencies (skip regex values)
    if let Some(ignore_deps_val) = obj.get("ignoreDependencies") {
        let deps = string_or_array(ignore_deps_val);
        let non_regex: Vec<String> = deps
            .into_iter()
            .filter(|d| {
                // Skip values that look like regex patterns
                if d.starts_with('/') && d.ends_with('/') {
                    warnings.push(MigrationWarning {
                        source: "knip",
                        field: "ignoreDependencies".to_string(),
                        message: format!("regex pattern `{d}` skipped (fallow uses exact strings)"),
                        suggestion: Some("add each dependency name explicitly".to_string()),
                    });
                    false
                } else {
                    true
                }
            })
            .collect();
        if !non_regex.is_empty() {
            config.insert(
                "ignoreDependencies".to_string(),
                serde_json::Value::Array(
                    non_regex
                        .into_iter()
                        .map(serde_json::Value::String)
                        .collect(),
                ),
            );
        }
    }

    // rules → rules mapping
    if let Some(rules_val) = obj.get("rules")
        && let Some(rules_obj) = rules_val.as_object()
    {
        let mut fallow_rules = serde_json::Map::new();
        for (knip_name, fallow_name) in KNIP_RULE_MAP {
            if let Some(severity_val) = rules_obj.get(*knip_name)
                && let Some(severity_str) = severity_val.as_str()
            {
                fallow_rules.insert(
                    (*fallow_name).to_string(),
                    serde_json::Value::String(severity_str.to_string()),
                );
            }
        }

        // Warn about unmappable rule names
        for (key, _) in rules_obj {
            let is_mapped = KNIP_RULE_MAP.iter().any(|(k, _)| k == key);
            let is_unmappable = KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&key.as_str());
            if !is_mapped && is_unmappable {
                warnings.push(MigrationWarning {
                    source: "knip",
                    field: format!("rules.{key}"),
                    message: format!("issue type `{key}` has no fallow equivalent"),
                    suggestion: None,
                });
            }
        }

        if !fallow_rules.is_empty() {
            config.insert("rules".to_string(), serde_json::Value::Object(fallow_rules));
        }
    }

    // exclude → set those issue types to "off" in rules
    if let Some(exclude_val) = obj.get("exclude") {
        let excluded = string_or_array(exclude_val);
        if !excluded.is_empty() {
            let rules = config
                .entry("rules".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(rules_obj) = rules.as_object_mut() {
                for knip_name in &excluded {
                    if let Some((_, fallow_name)) =
                        KNIP_RULE_MAP.iter().find(|(k, _)| k == knip_name)
                    {
                        rules_obj.insert(
                            (*fallow_name).to_string(),
                            serde_json::Value::String("off".to_string()),
                        );
                    } else if KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&knip_name.as_str()) {
                        warnings.push(MigrationWarning {
                            source: "knip",
                            field: format!("exclude.{knip_name}"),
                            message: format!("issue type `{knip_name}` has no fallow equivalent"),
                            suggestion: None,
                        });
                    }
                }
            }
        }
    }

    // include → set non-included issue types to "off" in rules
    if let Some(include_val) = obj.get("include") {
        let included = string_or_array(include_val);
        if !included.is_empty() {
            let rules = config
                .entry("rules".to_string())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(rules_obj) = rules.as_object_mut() {
                for (knip_name, fallow_name) in KNIP_RULE_MAP {
                    if !included.iter().any(|i| i == knip_name) {
                        // Not included — set to off (unless already set by rules)
                        rules_obj
                            .entry((*fallow_name).to_string())
                            .or_insert_with(|| serde_json::Value::String("off".to_string()));
                    }
                }
                // Warn about unmappable included types
                for name in &included {
                    let is_mapped = KNIP_RULE_MAP.iter().any(|(k, _)| k == name);
                    if !is_mapped && KNIP_UNMAPPABLE_ISSUE_TYPES.contains(&name.as_str()) {
                        warnings.push(MigrationWarning {
                            source: "knip",
                            field: format!("include.{name}"),
                            message: format!("issue type `{name}` has no fallow equivalent"),
                            suggestion: None,
                        });
                    }
                }
            }
        }
    }

    // Warn about unmappable fields
    for (field, message, suggestion) in KNIP_UNMAPPABLE_FIELDS {
        if obj.contains_key(*field) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: (*field).to_string(),
                message: (*message).to_string(),
                suggestion: suggestion.map(|s| s.to_string()),
            });
        }
    }

    // Warn about plugin-specific config keys
    for key in obj.keys() {
        if KNIP_PLUGIN_KEYS.contains(&key.as_str()) {
            warnings.push(MigrationWarning {
                source: "knip",
                field: key.clone(),
                message: format!(
                    "plugin config `{key}` is auto-detected by fallow's built-in plugins"
                ),
                suggestion: Some(
                    "remove this section; fallow detects framework config automatically"
                        .to_string(),
                ),
            });
        }
    }

    // Warn about workspaces with per-workspace plugin overrides
    if let Some(workspaces_val) = obj.get("workspaces")
        && workspaces_val.is_object()
    {
        warnings.push(MigrationWarning {
            source: "knip",
            field: "workspaces".to_string(),
            message: "per-workspace plugin overrides have limited support in fallow".to_string(),
            suggestion: Some(
                "fallow auto-discovers workspace packages; use --workspace flag to scope output"
                    .to_string(),
            ),
        });
    }
}

// ── jscpd migration ─────────────────────────────────────────────

/// jscpd fields that cannot be mapped and generate warnings.
const JSCPD_UNMAPPABLE_FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("maxLines", "No maximum line count limit in fallow", None),
    ("maxSize", "No maximum file size limit in fallow", None),
    (
        "ignorePattern",
        "Content-based ignore patterns are not supported",
        Some("use inline suppression: // fallow-ignore-next-line code-duplication"),
    ),
    (
        "reporters",
        "Reporters are not configurable in fallow",
        Some("use --format flag instead (human/json/sarif/compact)"),
    ),
    (
        "output",
        "fallow writes to stdout",
        Some("redirect output with shell: fallow dupes > report.json"),
    ),
    (
        "blame",
        "Git blame integration is not supported in fallow",
        None,
    ),
    ("absolute", "fallow always shows relative paths", None),
    (
        "noSymlinks",
        "Symlink handling is not configurable in fallow",
        None,
    ),
    (
        "ignoreCase",
        "Case-insensitive matching is not supported in fallow",
        None,
    ),
    ("format", "fallow auto-detects JS/TS files", None),
    (
        "formatsExts",
        "Custom file extensions are not configurable in fallow",
        None,
    ),
    ("store", "Store backend is not configurable in fallow", None),
    (
        "tokensToSkip",
        "Token skipping is not configurable in fallow",
        None,
    ),
    (
        "exitCode",
        "Exit codes are not configurable in fallow",
        Some("use the rules system to control which issues cause CI failure"),
    ),
    (
        "pattern",
        "Pattern filtering is not supported in fallow",
        None,
    ),
    (
        "path",
        "Source path configuration is not supported",
        Some("run fallow from the project root directory"),
    ),
];

fn migrate_jscpd(
    jscpd: &serde_json::Value,
    config: &mut serde_json::Map<String, serde_json::Value>,
    warnings: &mut Vec<MigrationWarning>,
) {
    let obj = match jscpd.as_object() {
        Some(o) => o,
        None => {
            warnings.push(MigrationWarning {
                source: "jscpd",
                field: "(root)".to_string(),
                message: "expected an object, got something else".to_string(),
                suggestion: None,
            });
            return;
        }
    };

    let mut dupes = serde_json::Map::new();

    // minTokens → duplicates.minTokens
    if let Some(min_tokens) = obj.get("minTokens").and_then(|v| v.as_u64()) {
        dupes.insert(
            "minTokens".to_string(),
            serde_json::Value::Number(min_tokens.into()),
        );
    }

    // minLines → duplicates.minLines
    if let Some(min_lines) = obj.get("minLines").and_then(|v| v.as_u64()) {
        dupes.insert(
            "minLines".to_string(),
            serde_json::Value::Number(min_lines.into()),
        );
    }

    // threshold → duplicates.threshold
    if let Some(threshold) = obj.get("threshold").and_then(|v| v.as_f64())
        && let Some(n) = serde_json::Number::from_f64(threshold)
    {
        dupes.insert("threshold".to_string(), serde_json::Value::Number(n));
    }

    // mode → duplicates.mode
    if let Some(mode_str) = obj.get("mode").and_then(|v| v.as_str()) {
        let fallow_mode = match mode_str {
            "strict" => Some("strict"),
            "mild" => Some("mild"),
            "weak" => {
                warnings.push(MigrationWarning {
                    source: "jscpd",
                    field: "mode".to_string(),
                    message: "jscpd's \"weak\" mode may differ semantically from fallow's \"weak\" \
                              mode. jscpd uses lexer-based tokens while fallow uses AST-based tokens."
                        .to_string(),
                    suggestion: Some(
                        "test with both \"weak\" and \"mild\" to find the best match".to_string(),
                    ),
                });
                Some("weak")
            }
            other => {
                warnings.push(MigrationWarning {
                    source: "jscpd",
                    field: "mode".to_string(),
                    message: format!("unknown mode `{other}`, defaulting to \"mild\""),
                    suggestion: None,
                });
                None
            }
        };
        if let Some(mode) = fallow_mode {
            dupes.insert(
                "mode".to_string(),
                serde_json::Value::String(mode.to_string()),
            );
        }
    }

    // skipLocal → duplicates.skipLocal
    if let Some(skip_local) = obj.get("skipLocal").and_then(|v| v.as_bool()) {
        dupes.insert("skipLocal".to_string(), serde_json::Value::Bool(skip_local));
    }

    // ignore → duplicates.ignore (glob patterns)
    if let Some(ignore_val) = obj.get("ignore") {
        let ignores = string_or_array(ignore_val);
        if !ignores.is_empty() {
            dupes.insert(
                "ignore".to_string(),
                serde_json::Value::Array(
                    ignores.into_iter().map(serde_json::Value::String).collect(),
                ),
            );
        }
    }

    if !dupes.is_empty() {
        config.insert("duplicates".to_string(), serde_json::Value::Object(dupes));
    }

    // Warn about unmappable fields
    for (field, message, suggestion) in JSCPD_UNMAPPABLE_FIELDS {
        if obj.contains_key(*field) {
            warnings.push(MigrationWarning {
                source: "jscpd",
                field: (*field).to_string(),
                message: (*message).to_string(),
                suggestion: suggestion.map(|s| s.to_string()),
            });
        }
    }
}

// ── Output generation ───────────────────────────────────────────

fn generate_jsonc(result: &MigrationResult) -> String {
    let mut output = String::new();
    output.push_str("{\n");
    output.push_str(
        "  \"$schema\": \"https://raw.githubusercontent.com/fallow-rs/fallow/main/schema.json\",\n",
    );

    let obj = result.config.as_object().unwrap();
    let source_comment = result.sources.join(", ");
    output.push_str(&format!("  // Migrated from {source_comment}\n"));

    let mut entries: Vec<(&String, &serde_json::Value)> = obj.iter().collect();
    // Sort keys for consistent output
    let key_order = [
        "entry",
        "ignore",
        "ignoreDependencies",
        "rules",
        "duplicates",
    ];
    entries.sort_by_key(|(k, _)| {
        key_order
            .iter()
            .position(|o| *o == k.as_str())
            .unwrap_or(usize::MAX)
    });

    let total = entries.len();
    for (i, (key, value)) in entries.iter().enumerate() {
        let is_last = i == total - 1;
        let serialized = serde_json::to_string_pretty(value).unwrap_or_default();
        // Indent the serialized value by 2 spaces (but the first line is on the key line)
        let indented = indent_json_value(&serialized, 2);
        if is_last {
            output.push_str(&format!("  \"{key}\": {indented}\n"));
        } else {
            output.push_str(&format!("  \"{key}\": {indented},\n"));
        }
    }

    output.push_str("}\n");
    output
}

/// Indent a pretty-printed JSON value's continuation lines.
fn indent_json_value(json: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    let mut lines: Vec<&str> = json.lines().collect();
    if lines.len() <= 1 {
        return json.to_string();
    }
    // First line stays as-is, subsequent lines get indented
    let first = lines.remove(0);
    let rest: Vec<String> = lines.iter().map(|l| format!("{indent}{l}")).collect();
    let mut result = first.to_string();
    for line in rest {
        result.push('\n');
        result.push_str(&line);
    }
    result
}

fn generate_toml(result: &MigrationResult) -> String {
    let mut output = String::new();
    let source_comment = result.sources.join(", ");
    output.push_str(&format!("# Migrated from {source_comment}\n\n"));

    let obj = result.config.as_object().unwrap();

    // Top-level simple fields first
    // Note: fallow config uses #[serde(rename_all = "camelCase")] so TOML keys must be camelCase
    for key in &["entry", "ignore", "ignoreDependencies"] {
        if let Some(value) = obj.get(*key)
            && let Some(arr) = value.as_array()
        {
            let items: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| format!("\"{s}\"")))
                .collect();
            output.push_str(&format!("{key} = [{}]\n", items.join(", ")));
        }
    }

    // [rules] table
    if let Some(rules) = obj.get("rules")
        && let Some(rules_obj) = rules.as_object()
        && !rules_obj.is_empty()
    {
        output.push_str("\n[rules]\n");
        for (key, value) in rules_obj {
            if let Some(s) = value.as_str() {
                output.push_str(&format!("{key} = \"{s}\"\n"));
            }
        }
    }

    // [duplicates] table
    if let Some(dupes) = obj.get("duplicates")
        && let Some(dupes_obj) = dupes.as_object()
        && !dupes_obj.is_empty()
    {
        output.push_str("\n[duplicates]\n");
        for (key, value) in dupes_obj {
            match value {
                serde_json::Value::Number(n) => {
                    output.push_str(&format!("{key} = {n}\n"));
                }
                serde_json::Value::Bool(b) => {
                    output.push_str(&format!("{key} = {b}\n"));
                }
                serde_json::Value::String(s) => {
                    output.push_str(&format!("{key} = \"{s}\"\n"));
                }
                serde_json::Value::Array(arr) => {
                    let items: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| format!("\"{s}\"")))
                        .collect();
                    output.push_str(&format!("{key} = [{}]\n", items.join(", ")));
                }
                _ => {}
            }
        }
    }

    output
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_config() -> serde_json::Map<String, serde_json::Value> {
        serde_json::Map::new()
    }

    // ── knip migration tests ────────────────────────────────────

    #[test]
    fn migrate_minimal_knip_json() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"entry": ["src/index.ts"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("entry").unwrap(),
            &serde_json::json!(["src/index.ts"])
        );
        assert!(warnings.is_empty());
    }

    #[test]
    fn migrate_knip_with_rules() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"rules": {"files": "warn", "exports": "off", "dependencies": "error"}}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unusedFiles").unwrap(), "warn");
        assert_eq!(rules.get("unusedExports").unwrap(), "off");
        assert_eq!(rules.get("unusedDependencies").unwrap(), "error");
    }

    #[test]
    fn migrate_knip_with_exclude() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"exclude": ["files", "types"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unusedFiles").unwrap(), "off");
        assert_eq!(rules.get("unusedTypes").unwrap(), "off");
    }

    #[test]
    fn migrate_knip_with_include() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"include": ["files", "exports"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        // Included types should NOT be set to "off"
        assert!(!rules.contains_key("unusedFiles") || rules.get("unusedFiles").unwrap() != "off");
        assert!(
            !rules.contains_key("unusedExports") || rules.get("unusedExports").unwrap() != "off"
        );
        // Non-included types should be "off"
        assert_eq!(rules.get("unusedDependencies").unwrap(), "off");
        assert_eq!(rules.get("unusedTypes").unwrap(), "off");
        assert_eq!(rules.get("unusedEnumMembers").unwrap(), "off");
        assert_eq!(rules.get("unusedClassMembers").unwrap(), "off");
    }

    #[test]
    fn migrate_knip_with_ignore_patterns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignore": ["src/generated/**", "**/*.test.ts"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignore").unwrap(),
            &serde_json::json!(["src/generated/**", "**/*.test.ts"])
        );
    }

    #[test]
    fn migrate_knip_with_ignore_dependencies() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": ["@org/lib", "lodash"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["@org/lib", "lodash"])
        );
    }

    #[test]
    fn migrate_knip_regex_ignore_deps_skipped() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"ignoreDependencies": ["/^@org/", "lodash"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("ignoreDependencies").unwrap(),
            &serde_json::json!(["lodash"])
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field == "ignoreDependencies");
    }

    #[test]
    fn migrate_knip_unmappable_fields_generate_warnings() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"project": ["src/**"], "paths": {"@/*": ["src/*"]}}"#)
                .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 2);
        let fields: Vec<&str> = warnings.iter().map(|w| w.field.as_str()).collect();
        assert!(fields.contains(&"project"));
        assert!(fields.contains(&"paths"));
    }

    #[test]
    fn migrate_knip_plugin_keys_generate_warnings() {
        let knip: serde_json::Value = serde_json::from_str(
            r#"{"entry": ["src/index.ts"], "eslint": {"entry": ["eslint.config.js"]}}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].field, "eslint");
        assert!(warnings[0].message.contains("auto-detected"));
    }

    #[test]
    fn migrate_knip_entry_string() {
        let knip: serde_json::Value = serde_json::from_str(r#"{"entry": "src/index.ts"}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(
            config.get("entry").unwrap(),
            &serde_json::json!(["src/index.ts"])
        );
    }

    // ── jscpd migration tests ───────────────────────────────────

    #[test]
    fn migrate_jscpd_basic() {
        let jscpd: serde_json::Value =
            serde_json::from_str(r#"{"minTokens": 100, "minLines": 10, "threshold": 5.0}"#)
                .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        let dupes = config.get("duplicates").unwrap().as_object().unwrap();
        assert_eq!(dupes.get("minTokens").unwrap(), 100);
        assert_eq!(dupes.get("minLines").unwrap(), 10);
        assert_eq!(dupes.get("threshold").unwrap(), 5.0);
        assert!(warnings.is_empty());
    }

    #[test]
    fn migrate_jscpd_mode_weak_warns() {
        let jscpd: serde_json::Value = serde_json::from_str(r#"{"mode": "weak"}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        let dupes = config.get("duplicates").unwrap().as_object().unwrap();
        assert_eq!(dupes.get("mode").unwrap(), "weak");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].message.contains("differ semantically"));
    }

    #[test]
    fn migrate_jscpd_skip_local() {
        let jscpd: serde_json::Value = serde_json::from_str(r#"{"skipLocal": true}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        let dupes = config.get("duplicates").unwrap().as_object().unwrap();
        assert_eq!(dupes.get("skipLocal").unwrap(), true);
    }

    #[test]
    fn migrate_jscpd_ignore_patterns() {
        let jscpd: serde_json::Value =
            serde_json::from_str(r#"{"ignore": ["**/*.test.ts", "dist/**"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        let dupes = config.get("duplicates").unwrap().as_object().unwrap();
        assert_eq!(
            dupes.get("ignore").unwrap(),
            &serde_json::json!(["**/*.test.ts", "dist/**"])
        );
    }

    #[test]
    fn migrate_jscpd_unmappable_fields_generate_warnings() {
        let jscpd: serde_json::Value = serde_json::from_str(
            r#"{"minTokens": 50, "maxLines": 1000, "reporters": ["console"], "blame": true}"#,
        )
        .unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 3);
        let fields: Vec<&str> = warnings.iter().map(|w| w.field.as_str()).collect();
        assert!(fields.contains(&"maxLines"));
        assert!(fields.contains(&"reporters"));
        assert!(fields.contains(&"blame"));
    }

    // ── Combined migration tests ────────────────────────────────

    #[test]
    fn migrate_both_knip_and_jscpd() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"entry": ["src/index.ts"], "ignore": ["dist/**"]}"#).unwrap();
        let jscpd: serde_json::Value =
            serde_json::from_str(r#"{"minTokens": 100, "skipLocal": true}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);
        migrate_jscpd(&jscpd, &mut config, &mut warnings);

        assert!(config.contains_key("entry"));
        assert!(config.contains_key("ignore"));
        assert!(config.contains_key("duplicates"));
    }

    // ── Output format tests ─────────────────────────────────────

    #[test]
    fn jsonc_output_has_schema() {
        let result = MigrationResult {
            config: serde_json::json!({"entry": ["src/index.ts"]}),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_jsonc(&result);
        assert!(output.contains("$schema"));
        assert!(output.contains("fallow-rs/fallow"));
    }

    #[test]
    fn jsonc_output_has_source_comment() {
        let result = MigrationResult {
            config: serde_json::json!({"entry": ["src/index.ts"]}),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_jsonc(&result);
        assert!(output.contains("// Migrated from knip.json"));
    }

    #[test]
    fn toml_output_has_source_comment() {
        let result = MigrationResult {
            config: serde_json::json!({"entry": ["src/index.ts"]}),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_toml(&result);
        assert!(output.contains("# Migrated from knip.json"));
    }

    #[test]
    fn toml_output_rules_section() {
        let result = MigrationResult {
            config: serde_json::json!({
                "rules": {
                    "unusedFiles": "error",
                    "unusedExports": "warn"
                }
            }),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_toml(&result);
        assert!(output.contains("[rules]"));
        assert!(output.contains("unusedFiles = \"error\""));
        assert!(output.contains("unusedExports = \"warn\""));
    }

    #[test]
    fn toml_output_duplicates_section() {
        let result = MigrationResult {
            config: serde_json::json!({
                "duplicates": {
                    "minTokens": 100,
                    "skipLocal": true
                }
            }),
            warnings: vec![],
            sources: vec![".jscpd.json".to_string()],
        };
        let output = generate_toml(&result);
        assert!(output.contains("[duplicates]"));
        assert!(output.contains("minTokens = 100"));
        assert!(output.contains("skipLocal = true"));
    }

    // ── Deserialization roundtrip tests ─────────────────────────

    #[test]
    fn toml_output_deserializes_as_valid_config() {
        let result = MigrationResult {
            config: serde_json::json!({
                "entry": ["src/index.ts"],
                "ignore": ["dist/**"],
                "ignoreDependencies": ["lodash"],
                "rules": {
                    "unusedFiles": "error",
                    "unusedExports": "warn"
                },
                "duplicates": {
                    "minTokens": 100,
                    "skipLocal": true
                }
            }),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_toml(&result);
        let config: fallow_config::FallowConfig = toml::from_str(&output).unwrap();
        assert_eq!(config.entry, vec!["src/index.ts"]);
        assert_eq!(config.ignore, vec!["dist/**"]);
        assert_eq!(config.ignore_dependencies, vec!["lodash"]);
    }

    #[test]
    fn jsonc_output_deserializes_as_valid_config() {
        let result = MigrationResult {
            config: serde_json::json!({
                "entry": ["src/index.ts"],
                "ignoreDependencies": ["lodash"],
                "rules": {
                    "unusedFiles": "warn"
                }
            }),
            warnings: vec![],
            sources: vec!["knip.json".to_string()],
        };
        let output = generate_jsonc(&result);
        let mut stripped = String::new();
        json_comments::StripComments::new(output.as_bytes())
            .read_to_string(&mut stripped)
            .unwrap();
        let config: fallow_config::FallowConfig = serde_json::from_str(&stripped).unwrap();
        assert_eq!(config.entry, vec!["src/index.ts"]);
        assert_eq!(config.ignore_dependencies, vec!["lodash"]);
    }

    // ── JSONC comment stripping test ────────────────────────────

    #[test]
    fn jsonc_comments_stripped() {
        let tmpdir = std::env::temp_dir().join("fallow-test-migrate-jsonc");
        let _ = std::fs::create_dir_all(&tmpdir);
        let path = tmpdir.join("knip.jsonc");
        std::fs::write(
            &path,
            r#"{
                // Entry points
                "entry": ["src/index.ts"],
                /* Block comment */
                "ignore": ["dist/**"]
            }"#,
        )
        .unwrap();

        let value = load_json_or_jsonc(&path).unwrap();
        assert_eq!(value["entry"], serde_json::json!(["src/index.ts"]));
        assert_eq!(value["ignore"], serde_json::json!(["dist/**"]));

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    // ── Package.json embedded config detection ──────────────────

    #[test]
    fn auto_detect_package_json_knip() {
        let tmpdir = std::env::temp_dir().join("fallow-test-migrate-pkg-knip");
        let _ = std::fs::create_dir_all(&tmpdir);
        let pkg_path = tmpdir.join("package.json");
        std::fs::write(
            &pkg_path,
            r#"{"name": "test", "knip": {"entry": ["src/main.ts"]}}"#,
        )
        .unwrap();

        let result = migrate_auto_detect(&tmpdir).unwrap();
        assert!(!result.sources.is_empty());
        assert!(result.sources[0].contains("package.json"));

        let config_obj = result.config.as_object().unwrap();
        assert_eq!(
            config_obj.get("entry").unwrap(),
            &serde_json::json!(["src/main.ts"])
        );

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    #[test]
    fn auto_detect_package_json_jscpd() {
        let tmpdir = std::env::temp_dir().join("fallow-test-migrate-pkg-jscpd");
        let _ = std::fs::create_dir_all(&tmpdir);
        let pkg_path = tmpdir.join("package.json");
        std::fs::write(&pkg_path, r#"{"name": "test", "jscpd": {"minTokens": 75}}"#).unwrap();

        let result = migrate_auto_detect(&tmpdir).unwrap();
        assert!(!result.sources.is_empty());
        assert!(result.sources[0].contains("package.json"));

        let config_obj = result.config.as_object().unwrap();
        let dupes = config_obj.get("duplicates").unwrap().as_object().unwrap();
        assert_eq!(dupes.get("minTokens").unwrap(), 75);

        let _ = std::fs::remove_dir_all(&tmpdir);
    }

    // ── knip exclude with unmappable issue type ─────────────────

    #[test]
    fn migrate_knip_exclude_unmappable_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"exclude": ["optionalPeerDependencies"]}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field.contains("optionalPeerDependencies"));
    }

    // ── knip rules with unmappable issue type ───────────────────

    #[test]
    fn migrate_knip_rules_unmappable_warns() {
        let knip: serde_json::Value =
            serde_json::from_str(r#"{"rules": {"binaries": "warn", "files": "error"}}"#).unwrap();
        let mut config = empty_config();
        let mut warnings = Vec::new();
        migrate_knip(&knip, &mut config, &mut warnings);

        let rules = config.get("rules").unwrap().as_object().unwrap();
        assert_eq!(rules.get("unusedFiles").unwrap(), "error");
        assert!(!rules.contains_key("binaries"));

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].field.contains("binaries"));
    }
}
