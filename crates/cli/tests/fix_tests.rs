#[path = "common/mod.rs"]
mod common;

use common::{parse_json, run_fallow, run_fallow_in_root};

// ---------------------------------------------------------------------------
// fix --dry-run
// ---------------------------------------------------------------------------

#[test]
fn fix_dry_run_exits_0() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    assert_eq!(
        output.code, 0,
        "fix --dry-run should exit 0, stderr: {}",
        output.stderr
    );
}

#[test]
fn fix_dry_run_json_has_dry_run_flag() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    assert_eq!(
        json["dry_run"].as_bool(),
        Some(true),
        "dry_run should be true"
    );
}

#[test]
fn fix_dry_run_finds_fixable_items() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let fixes = json["fixes"].as_array().unwrap();
    assert!(!fixes.is_empty(), "basic-project should have fixable items");

    // Each fix should have a type
    for fix in fixes {
        assert!(fix.get("type").is_some(), "fix should have 'type'");
        // Export fixes have "path", dependency fixes have "package"
        let has_path = fix.get("path").is_some() || fix.get("package").is_some();
        assert!(has_path, "fix should have 'path' or 'package'");
    }
}

#[test]
fn fix_dry_run_does_not_have_applied_key() {
    let output = run_fallow(
        "fix",
        "basic-project",
        &["--dry-run", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let fixes = json["fixes"].as_array().unwrap();
    for fix in fixes {
        assert!(
            fix.get("applied").is_none(),
            "dry-run fixes should not have 'applied' key"
        );
    }
}

#[test]
fn fix_removes_unused_exported_enum_declaration() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"enum-fix","main":"src/index.ts"}"#,
    )
    .unwrap();
    std::fs::write(root.join("src/index.ts"), "import './enum';\n").unwrap();
    std::fs::write(
        root.join("src/enum.ts"),
        "export enum MyEnum {\n  A,\n  B,\n}\n",
    )
    .unwrap();

    let output = run_fallow_in_root("fix", root, &["--yes", "--quiet"]);

    assert_eq!(
        output.code, 0,
        "fix should exit 0, stdout: {}, stderr: {}",
        output.stdout, output.stderr
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/enum.ts")).unwrap(),
        "\n"
    );

    let output = run_fallow_in_root("fix", root, &["--dry-run", "--format", "json", "--quiet"]);
    let json = parse_json(&output);
    assert!(json["fixes"].as_array().unwrap().is_empty());
}

#[test]
fn fix_folds_imported_enum_with_all_members_unused() {
    // Regression for issue #232: an exported enum that has importers but
    // whose members are all unused should be removed entirely, not stripped
    // member-by-member into a zombie `export enum X {}` shell.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"name":"enum-fold","main":"src/index.ts"}"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/index.ts"),
        "import { MyEnum } from './enum';\nconsole.log(typeof MyEnum);\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/enum.ts"),
        "export enum MyEnum {\n  A,\n  B,\n}\n",
    )
    .unwrap();

    let output = run_fallow_in_root("fix", root, &["--dry-run", "--format", "json", "--quiet"]);
    let json = parse_json(&output);
    let fixes = json["fixes"].as_array().unwrap();
    assert_eq!(
        fixes.len(),
        1,
        "fold should collapse the per-member fixes into a single remove_export entry"
    );
    assert_eq!(fixes[0]["type"], "remove_export");
    assert_eq!(fixes[0]["name"], "MyEnum");

    let output = run_fallow_in_root("fix", root, &["--yes", "--quiet"]);
    assert_eq!(
        output.code, 0,
        "fix should exit 0, stdout: {}, stderr: {}",
        output.stdout, output.stderr
    );

    let after = std::fs::read_to_string(root.join("src/enum.ts")).unwrap();
    assert_eq!(
        after, "\n",
        "enum.ts should be empty after the fold (single trailing newline)"
    );

    // Second pass: the empty-shell zombie that 2.54.3 would have left behind
    // must not be present, and the fold must not produce any new fix.
    let output = run_fallow_in_root("fix", root, &["--dry-run", "--format", "json", "--quiet"]);
    let json = parse_json(&output);
    assert!(
        json["fixes"].as_array().unwrap().is_empty(),
        "second pass should find nothing more to fix"
    );
}

// ---------------------------------------------------------------------------
// fix without --yes in non-TTY
// ---------------------------------------------------------------------------

#[test]
fn fix_without_yes_in_non_tty_exits_2() {
    // Running fix without --dry-run and without --yes in a non-TTY (test runner)
    // should exit 2 with an error
    let output = run_fallow("fix", "basic-project", &["--format", "json", "--quiet"]);
    assert_eq!(output.code, 2, "fix without --yes in non-TTY should exit 2");
}
