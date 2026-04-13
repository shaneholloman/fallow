use super::common::{create_config, fixture_path};
use fallow_core::discover::{DiscoveredFile, FileId};

#[test]
fn duplicate_code_detects_exact_clones() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    assert!(
        !report.clone_groups.is_empty(),
        "Should detect clones in duplicate-code fixture"
    );
    assert!(
        report.stats.files_with_clones >= 2,
        "At least 2 files should have clones"
    );
    assert!(
        report.stats.duplication_percentage > 0.0,
        "Duplication percentage should be > 0"
    );
}

#[test]
fn duplicate_code_semantic_mode_detects_type2_clones() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        mode: fallow_core::duplicates::DetectionMode::Semantic,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    // In semantic mode, copy2.ts (renamed variables) should also match
    let files_with_clones: rustc_hash::FxHashSet<_> = report
        .clone_groups
        .iter()
        .flat_map(|g| g.instances.iter())
        .map(|inst| inst.file.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        files_with_clones.contains("copy2.ts"),
        "Semantic mode should detect copy2.ts with renamed variables, files found: {files_with_clones:?}"
    );
}

#[test]
fn duplicate_code_unique_file_has_no_clones() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    // unique.ts should not appear in any clone group (its code is distinct)
    let all_clone_files: Vec<String> = report
        .clone_groups
        .iter()
        .flat_map(|g| g.instances.iter())
        .map(|inst| inst.file.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert!(
        !all_clone_files.contains(&"unique.ts".to_string()),
        "unique.ts should not appear in any clone group, found in: {all_clone_files:?}"
    );
}

#[test]
fn duplicate_code_json_output_serializable() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    // Should be serializable to JSON
    let json = serde_json::to_string_pretty(&report).expect("report should serialize to JSON");
    let reparsed: serde_json::Value = serde_json::from_str(&json).expect("JSON should be valid");
    assert!(reparsed["clone_groups"].is_array());
    assert!(reparsed["stats"]["total_files"].is_number());
}

#[test]
fn duplicate_code_skip_local_filters_same_directory() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        skip_local: true,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    // All fixture files are in the same directory (src/), so skip_local should filter them all
    assert!(
        report.clone_groups.is_empty(),
        "skip_local should filter same-directory clones"
    );
}

#[test]
fn duplicate_code_min_tokens_threshold_filters() {
    let root = fixture_path("duplicate-code");
    let config = create_config(root.clone());
    let files = fallow_core::discover::discover_files(&config);

    // Use very high min_tokens — should find no clones
    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 10000,
        min_lines: 1,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates(&root, &files, &dupes_config);

    assert!(
        report.clone_groups.is_empty(),
        "Very high min_tokens should find no clones"
    );
}

#[test]
fn duplicate_code_find_duplicates_in_project_convenience() {
    let root = fixture_path("duplicate-code");

    let dupes_config = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 20,
        min_lines: 3,
        ..fallow_core::duplicates::DuplicatesConfig::default()
    };

    let report = fallow_core::duplicates::find_duplicates_in_project(&root, &dupes_config);

    assert!(
        !report.clone_groups.is_empty(),
        "Convenience function should detect clones"
    );
}

#[test]
fn ignore_imports_removes_import_only_clones() {
    // Create two files with identical sorted import blocks but different runtime code.
    // Without ignore_imports, the import blocks produce clone groups.
    // With ignore_imports, the imports are stripped and only runtime code is compared.
    let dir = tempfile::tempdir().expect("create temp dir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("create src");

    let imports = "import { A } from './a';\n\
                    import { B } from './b';\n\
                    import { C } from './c';\n\
                    import { D } from './d';\n\
                    import { E } from './e';\n\
                    import { F } from './f';\n\
                    import { G } from './g';\n\
                    import { H } from './h';\n";

    // File 1: identical imports + unique code
    let file1 = format!("{imports}\nexport function foo() {{ return A + B + C; }}\n");
    // File 2: identical imports + different code
    let file2 = format!("{imports}\nexport function bar() {{ return D * E * F; }}\n");

    std::fs::write(src.join("file1.ts"), &file1).expect("write file1");
    std::fs::write(src.join("file2.ts"), &file2).expect("write file2");
    std::fs::write(dir.path().join("package.json"), r#"{"name": "test"}"#)
        .expect("write package.json");

    let files = vec![
        DiscoveredFile {
            id: FileId(0),
            path: src.join("file1.ts"),
            size_bytes: file1.len() as u64,
        },
        DiscoveredFile {
            id: FileId(1),
            path: src.join("file2.ts"),
            size_bytes: file2.len() as u64,
        },
    ];

    // Without ignore_imports: should detect import block duplication
    let config_with_imports = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 10,
        min_lines: 3,
        ..Default::default()
    };
    let report_with =
        fallow_core::duplicates::find_duplicates(dir.path(), &files, &config_with_imports);
    assert!(
        !report_with.clone_groups.is_empty(),
        "Without ignore_imports, identical import blocks should be detected as clones"
    );

    // With ignore_imports: import block clones should disappear
    let config_ignore = fallow_core::duplicates::DuplicatesConfig {
        min_tokens: 10,
        min_lines: 3,
        ignore_imports: true,
        ..Default::default()
    };
    let report_without =
        fallow_core::duplicates::find_duplicates(dir.path(), &files, &config_ignore);
    assert!(
        report_without.clone_groups.is_empty(),
        "With ignore_imports=true, import-only clones should be eliminated, but found {} groups",
        report_without.clone_groups.len()
    );
}
