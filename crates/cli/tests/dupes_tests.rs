#[path = "common/mod.rs"]
mod common;

use common::{fixture_path, parse_json, redact_all, run_fallow};

// ---------------------------------------------------------------------------
// JSON output structure
// ---------------------------------------------------------------------------

#[test]
fn dupes_json_output_has_clone_groups() {
    let output = run_fallow("dupes", "duplicate-code", &["--format", "json", "--quiet"]);
    let json = parse_json(&output);
    assert!(
        json.get("clone_groups").is_some(),
        "dupes JSON should have clone_groups key"
    );
    let groups = json["clone_groups"].as_array().unwrap();
    assert!(
        !groups.is_empty(),
        "duplicate-code fixture should have clone groups"
    );
}

#[test]
fn dupes_json_has_stats() {
    let output = run_fallow("dupes", "duplicate-code", &["--format", "json", "--quiet"]);
    let json = parse_json(&output);
    assert!(
        json.get("stats").is_some(),
        "dupes JSON should have stats key"
    );
}

// ---------------------------------------------------------------------------
// Mode flags
// ---------------------------------------------------------------------------

#[test]
fn dupes_strict_mode_accepted() {
    let output = run_fallow(
        "dupes",
        "duplicate-code",
        &["--mode", "strict", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "dupes --mode strict should not crash, got exit code {}",
        output.code
    );
}

#[test]
fn dupes_mild_mode_accepted() {
    let output = run_fallow(
        "dupes",
        "duplicate-code",
        &["--mode", "mild", "--format", "json", "--quiet"],
    );
    assert!(
        output.code == 0 || output.code == 1,
        "dupes --mode mild should not crash"
    );
}

// ---------------------------------------------------------------------------
// Filtering
// ---------------------------------------------------------------------------

#[test]
fn dupes_min_tokens_filter() {
    let output = run_fallow(
        "dupes",
        "duplicate-code",
        &["--min-tokens", "1000", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let groups = json["clone_groups"].as_array().unwrap();
    assert!(
        groups.is_empty(),
        "high min-tokens should filter out all clones"
    );
}

#[test]
fn dupes_top_flag() {
    let output = run_fallow(
        "dupes",
        "duplicate-code",
        &["--top", "1", "--format", "json", "--quiet"],
    );
    let json = parse_json(&output);
    let groups = json["clone_groups"].as_array().unwrap();
    assert!(
        groups.len() <= 1,
        "--top 1 should return at most 1 clone group"
    );
}

// ---------------------------------------------------------------------------
// Path relativization (regression: #85)
// ---------------------------------------------------------------------------

#[test]
fn dupes_json_paths_are_relative() {
    let output = run_fallow("dupes", "duplicate-code", &["--format", "json", "--quiet"]);
    let json = parse_json(&output);
    let groups = json["clone_groups"].as_array().unwrap();
    assert!(!groups.is_empty(), "fixture should have clone groups");

    // All instance paths must be relative (no leading /)
    for group in groups {
        for instance in group["instances"].as_array().unwrap() {
            let path = instance["file"].as_str().unwrap();
            assert!(
                !path.starts_with('/'),
                "clone group instance path should be relative, got: {path}"
            );
        }
    }

    // Clone families should also have relative paths
    if let Some(families) = json.get("clone_families").and_then(|f| f.as_array()) {
        for family in families {
            if let Some(files) = family.get("files").and_then(|f| f.as_array()) {
                for file in files {
                    let path = file.as_str().unwrap();
                    assert!(
                        !path.starts_with('/'),
                        "clone family file path should be relative, got: {path}"
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Human output snapshot
// ---------------------------------------------------------------------------

#[test]
fn dupes_human_output_snapshot() {
    let output = run_fallow("dupes", "duplicate-code", &["--quiet"]);
    let root = fixture_path("duplicate-code");
    let redacted = redact_all(&output.stdout, &root);
    insta::assert_snapshot!("dupes_human_output", redacted);
}
