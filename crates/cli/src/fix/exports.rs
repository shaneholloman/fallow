use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

use fallow_config::OutputFormat;

use super::io::{read_source, write_fixed_content};

pub(super) struct ExportFix {
    line_idx: usize,
    export_name: String,
}

/// Apply export fixes to source files, returning JSON fix entries.
pub(super) fn apply_export_fixes(
    root: &Path,
    exports_by_file: &FxHashMap<PathBuf, Vec<&fallow_core::results::UnusedExport>>,
    output: OutputFormat,
    dry_run: bool,
    fixes: &mut Vec<serde_json::Value>,
) -> bool {
    let mut had_write_error = false;

    for (path, file_exports) in exports_by_file {
        let Some((content, line_ending)) = read_source(root, path) else {
            continue;
        };
        let lines: Vec<&str> = content.split(line_ending).collect();

        let mut line_fixes: Vec<ExportFix> = Vec::new();
        for export in file_exports {
            // Use the 1-indexed line field from the export directly
            let line_idx = export.line.saturating_sub(1) as usize;

            if line_idx >= lines.len() {
                continue;
            }

            let line = lines[line_idx];
            let trimmed = line.trim_start();

            // Skip lines that don't start with "export "
            if !trimmed.starts_with("export ") {
                continue;
            }

            let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);

            // Handle `export default` cases
            if after_export.starts_with("default ") {
                let after_default = after_export
                    .strip_prefix("default ")
                    .unwrap_or(after_export);
                if after_default.starts_with("function ")
                    || after_default.starts_with("async function ")
                    || after_default.starts_with("class ")
                    || after_default.starts_with("abstract class ")
                {
                    // `export default function Foo` -> `function Foo`
                    // `export default async function Foo` -> `async function Foo`
                    // `export default class Foo` -> `class Foo`
                    // `export default abstract class Foo` -> `abstract class Foo`
                    // handled below via line_fixes
                } else {
                    // `export default expression` -> skip (can't safely remove)
                    continue;
                }
            }

            line_fixes.push(ExportFix {
                line_idx,
                export_name: export.export_name.clone(),
            });
        }

        if line_fixes.is_empty() {
            continue;
        }

        // Sort by line index descending so we can work backwards without shifting indices
        line_fixes.sort_by(|a, b| b.line_idx.cmp(&a.line_idx));

        // Deduplicate by line_idx (multiple exports on the same line shouldn't be applied twice)
        line_fixes.dedup_by_key(|f| f.line_idx);

        let relative = path.strip_prefix(root).unwrap_or(path);

        if dry_run {
            for fix in &line_fixes {
                if !matches!(output, OutputFormat::Json) {
                    eprintln!(
                        "Would remove export from {}:{} `{}`",
                        relative.display(),
                        fix.line_idx + 1,
                        fix.export_name,
                    );
                }
                fixes.push(serde_json::json!({
                    "type": "remove_export",
                    "path": relative.display().to_string(),
                    "line": fix.line_idx + 1,
                    "name": fix.export_name,
                }));
            }
        } else {
            // Apply all fixes to a single in-memory copy
            let mut new_lines: Vec<String> = lines.iter().map(ToString::to_string).collect();
            for fix in &line_fixes {
                let line = &new_lines[fix.line_idx];
                let indent = line.len() - line.trim_start().len();
                let trimmed = line.trim_start();
                let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);

                let replacement = if after_export.starts_with("default function ")
                    || after_export.starts_with("default async function ")
                    || after_export.starts_with("default class ")
                    || after_export.starts_with("default abstract class ")
                {
                    // `export default function Foo` -> `function Foo`
                    after_export
                        .strip_prefix("default ")
                        .unwrap_or(after_export)
                } else {
                    after_export
                };

                let prefix = &line[..indent];
                new_lines[fix.line_idx] = format!("{prefix}{replacement}");
            }
            let success = match write_fixed_content(path, &new_lines, line_ending, &content) {
                Ok(()) => true,
                Err(e) => {
                    had_write_error = true;
                    eprintln!("Error: failed to write {}: {e}", relative.display());
                    false
                }
            };

            for fix in &line_fixes {
                fixes.push(serde_json::json!({
                    "type": "remove_export",
                    "path": relative.display().to_string(),
                    "line": fix.line_idx + 1,
                    "name": fix.export_name,
                    "applied": success,
                }));
            }
        }
    }

    had_write_error
}

#[cfg(test)]
mod tests {
    use super::*;
    use fallow_core::results::UnusedExport;

    fn make_export(path: &Path, name: &str, line: u32) -> UnusedExport {
        UnusedExport {
            path: path.to_path_buf(),
            export_name: name.to_string(),
            is_type_only: false,
            line,
            col: 0,
            span_start: 0,
            is_re_export: false,
        }
    }

    #[test]
    fn dry_run_export_fix_does_not_modify_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("src/utils.ts");
        std::fs::create_dir_all(root.join("src")).unwrap();
        let original = "export function foo() {}\nexport function bar() {}\n";
        std::fs::write(&file, original).unwrap();

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(root, &exports_by_file, OutputFormat::Json, true, &mut fixes);

        // File should not be modified
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        // Fix should be reported
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["type"], "remove_export");
        assert_eq!(fixes[0]["name"], "foo");
        assert!(fixes[0].get("applied").is_none());
    }

    #[test]
    fn actual_export_fix_removes_export_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("utils.ts");
        std::fs::write(&file, "export function foo() {}\nexport const bar = 1;\n").unwrap();

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        let had_error = apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        assert!(!had_error);
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "function foo() {}\nexport const bar = 1;\n");
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["applied"], true);
    }

    #[test]
    fn export_fix_removes_default_from_function() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("component.ts");
        std::fs::write(&file, "export default function App() {}\n").unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "function App() {}\n");
    }

    #[test]
    fn export_fix_removes_default_from_class() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("service.ts");
        std::fs::write(&file, "export default class MyService {}\n").unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "class MyService {}\n");
    }

    #[test]
    fn export_fix_removes_default_from_abstract_class() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("base.ts");
        std::fs::write(&file, "export default abstract class Base {}\n").unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "abstract class Base {}\n");
    }

    #[test]
    fn export_fix_removes_default_from_async_function() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("handler.ts");
        std::fs::write(&file, "export default async function handler() {}\n").unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "async function handler() {}\n");
    }

    #[test]
    fn export_fix_skips_default_expression_export() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("config.ts");
        let original = "export default { key: 'value' };\n";
        std::fs::write(&file, original).unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        // File unchanged — expression defaults are not safely removable
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_preserves_indentation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("mod.ts");
        std::fs::write(&file, "  export const x = 1;\n").unwrap();

        let export = make_export(&file, "x", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "  const x = 1;\n");
    }

    #[test]
    fn export_fix_preserves_crlf_line_endings() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("win.ts");
        std::fs::write(
            &file,
            "export function foo() {}\r\nexport function bar() {}\r\n",
        )
        .unwrap();

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "function foo() {}\r\nexport function bar() {}\r\n");
    }

    #[test]
    fn export_fix_skips_path_outside_project_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let outside_file = dir.path().join("outside.ts");
        let original = "export function evil() {}\n";
        std::fs::write(&outside_file, original).unwrap();

        let export = make_export(&outside_file, "evil", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(outside_file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            &root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        // File should be untouched and no fixes generated
        assert_eq!(std::fs::read_to_string(&outside_file).unwrap(), original);
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_skips_line_not_starting_with_export() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("tricky.ts");
        let original = "const foo = 'export something';\n";
        std::fs::write(&file, original).unwrap();

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_handles_multiple_exports_in_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("multi.ts");
        std::fs::write(
            &file,
            "export function a() {}\nexport const b = 1;\nexport class C {}\n",
        )
        .unwrap();

        let e1 = make_export(&file, "a", 1);
        let e2 = make_export(&file, "C", 3);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&e1, &e2]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            content,
            "function a() {}\nexport const b = 1;\nclass C {}\n"
        );
        assert_eq!(fixes.len(), 2);
    }

    #[test]
    fn export_fix_skips_out_of_bounds_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("short.ts");
        std::fs::write(&file, "export function a() {}\n").unwrap();

        // Line 999 is way out of bounds
        let export = make_export(&file, "ghost", 999);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        // File unchanged, no fixes
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "export function a() {}\n");
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_removes_export_from_const() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("constants.ts");
        std::fs::write(&file, "export const MAX = 100;\n").unwrap();

        let export = make_export(&file, "MAX", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "const MAX = 100;\n");
    }

    #[test]
    fn export_fix_removes_export_from_let() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("state.ts");
        std::fs::write(&file, "export let counter = 0;\n").unwrap();

        let export = make_export(&file, "counter", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "let counter = 0;\n");
    }

    #[test]
    fn export_fix_removes_export_from_type_alias() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("types.ts");
        std::fs::write(&file, "export type Foo = string;\n").unwrap();

        let export = make_export(&file, "Foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "type Foo = string;\n");
    }

    #[test]
    fn export_fix_removes_export_from_interface() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("types.ts");
        std::fs::write(&file, "export interface Bar {\n  name: string;\n}\n").unwrap();

        let export = make_export(&file, "Bar", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "interface Bar {\n  name: string;\n}\n");
    }

    #[test]
    fn export_fix_removes_export_from_enum() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("enums.ts");
        std::fs::write(&file, "export enum Status { Active, Inactive }\n").unwrap();

        let export = make_export(&file, "Status", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "enum Status { Active, Inactive }\n");
    }

    #[test]
    fn export_fix_deduplicates_same_line() {
        // Two exports pointing to the same line should only apply one fix
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("dup.ts");
        std::fs::write(&file, "export function foo() {}\n").unwrap();

        let e1 = make_export(&file, "foo", 1);
        let e2 = make_export(&file, "foo", 1); // duplicate line
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&e1, &e2]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "function foo() {}\n");
        // Dedup means only one fix entry in the JSON
        assert_eq!(fixes.len(), 1);
    }

    #[test]
    fn export_fix_preserves_tab_indentation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("tabbed.ts");
        std::fs::write(&file, "\texport const x = 1;\n").unwrap();

        let export = make_export(&file, "x", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "\tconst x = 1;\n");
    }

    #[test]
    fn export_fix_line_zero_saturating_sub() {
        // line=0 should saturate to 0 (line_idx = 0)
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("zero.ts");
        std::fs::write(&file, "export function first() {}\n").unwrap();

        let export = make_export(&file, "first", 0);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "function first() {}\n");
    }

    #[test]
    fn export_fix_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("empty.ts");
        std::fs::write(&file, "").unwrap();

        let export = make_export(&file, "x", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "");
        assert!(fixes.is_empty());
    }

    #[test]
    fn dry_run_with_human_output_reports_fixes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("mod.ts");
        let original = "export function foo() {}\n";
        std::fs::write(&file, original).unwrap();

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            true,
            &mut fixes,
        );

        // File not modified
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert_eq!(fixes.len(), 1);
        assert_eq!(fixes[0]["type"], "remove_export");
        assert!(fixes[0].get("applied").is_none());
    }

    #[test]
    fn export_fix_skips_default_variable_export() {
        // `export default someVariable;` should not be touched
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("config.ts");
        let original = "export default someVariable;\n";
        std::fs::write(&file, original).unwrap();

        let export = make_export(&file, "default", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file.clone(), vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_nonexistent_file_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("missing.ts"); // Does not exist

        let export = make_export(&file, "foo", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file, vec![&export]);

        let mut fixes = Vec::new();
        let had_error = apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        assert!(!had_error);
        assert!(fixes.is_empty());
    }

    #[test]
    fn export_fix_returns_relative_path_in_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let file = root.join("src").join("utils.ts");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(&file, "export const x = 1;\n").unwrap();

        let export = make_export(&file, "x", 1);
        let mut exports_by_file: FxHashMap<PathBuf, Vec<&UnusedExport>> = FxHashMap::default();
        exports_by_file.insert(file, vec![&export]);

        let mut fixes = Vec::new();
        apply_export_fixes(
            root,
            &exports_by_file,
            OutputFormat::Human,
            false,
            &mut fixes,
        );

        let path_str = fixes[0]["path"].as_str().unwrap().replace('\\', "/");
        assert_eq!(path_str, "src/utils.ts");
    }

}
