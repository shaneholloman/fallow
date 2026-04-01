use rmcp::ServerHandler;

use super::super::FallowMcp;

#[test]
fn server_info_is_correct() {
    let server = FallowMcp::new();
    let info = ServerHandler::get_info(&server);
    assert_eq!(info.server_info.name, "fallow-mcp");
    assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));
    assert!(info.capabilities.tools.is_some());
    assert!(info.instructions.is_some());
}

#[test]
fn all_tools_registered() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();
    assert!(names.contains(&"analyze".to_string()));
    assert!(names.contains(&"check_changed".to_string()));
    assert!(names.contains(&"find_dupes".to_string()));
    assert!(names.contains(&"fix_preview".to_string()));
    assert!(names.contains(&"fix_apply".to_string()));
    assert!(names.contains(&"project_info".to_string()));
    assert!(names.contains(&"check_health".to_string()));
    assert!(names.contains(&"audit".to_string()));
    assert_eq!(tools.len(), 8);
}

#[test]
fn read_only_tools_have_annotations() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let read_only = [
        "analyze",
        "check_changed",
        "find_dupes",
        "fix_preview",
        "project_info",
        "check_health",
        "audit",
    ];
    for tool in &tools {
        let name = tool.name.to_string();
        if read_only.contains(&name.as_str()) {
            let ann = tool.annotations.as_ref().expect("annotations");
            assert_eq!(ann.read_only_hint, Some(true), "{name} should be read-only");
        }
    }
}

#[test]
fn fix_apply_is_destructive() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let fix = tools.iter().find(|t| t.name == "fix_apply").unwrap();
    let ann = fix.annotations.as_ref().unwrap();
    assert_eq!(ann.destructive_hint, Some(true));
    assert_eq!(ann.read_only_hint, Some(false));
}

#[test]
fn all_tools_have_descriptions() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    for tool in &tools {
        let name = tool.name.to_string();
        let desc = tool.description.as_deref().unwrap_or("");
        assert!(
            !desc.is_empty(),
            "tool '{name}' should have a non-empty description"
        );
    }
}

#[test]
fn all_tools_have_annotations() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    for tool in &tools {
        let name = tool.name.to_string();
        assert!(
            tool.annotations.is_some(),
            "tool '{name}' should have annotations"
        );
    }
}

#[test]
fn open_world_hint_on_analysis_tools() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let open_world = [
        "analyze",
        "check_changed",
        "find_dupes",
        "fix_preview",
        "project_info",
        "check_health",
        "audit",
    ];
    for tool in &tools {
        let name = tool.name.to_string();
        if open_world.contains(&name.as_str()) {
            let ann = tool.annotations.as_ref().unwrap();
            assert_eq!(
                ann.open_world_hint,
                Some(true),
                "{name} should have open_world_hint=true"
            );
        }
    }
}

#[test]
fn fix_preview_is_not_destructive() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let preview = tools.iter().find(|t| t.name == "fix_preview").unwrap();
    let ann = preview.annotations.as_ref().unwrap();
    // fix_preview should be read-only (dry-run only)
    assert_eq!(ann.read_only_hint, Some(true));
    assert_ne!(ann.destructive_hint, Some(true));
}

#[test]
fn server_info_has_description() {
    let server = FallowMcp::new();
    let info = ServerHandler::get_info(&server);
    assert!(
        info.server_info
            .description
            .as_ref()
            .is_some_and(|d| !d.is_empty()),
        "server info should have a description"
    );
}

#[test]
fn server_instructions_mention_all_tools() {
    let server = FallowMcp::new();
    let info = ServerHandler::get_info(&server);
    let instructions = info.instructions.as_deref().unwrap();
    assert!(instructions.contains("analyze"));
    assert!(instructions.contains("check_changed"));
    assert!(instructions.contains("find_dupes"));
    assert!(instructions.contains("fix_preview"));
    assert!(instructions.contains("fix_apply"));
    assert!(instructions.contains("project_info"));
    assert!(instructions.contains("check_health"));
    assert!(instructions.contains("audit"));
}

#[test]
fn all_tools_have_input_schema() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    for tool in &tools {
        let name = tool.name.to_string();
        // input_schema should be present (JSON Schema object)
        assert!(
            !tool.input_schema.is_empty(),
            "tool '{name}' should have a non-empty input_schema"
        );
    }
}

// ── Schema property validation ────────────────────────────────────

#[test]
fn analyze_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "analyze").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "production",
        "workspace",
        "issue_types",
        "baseline",
        "save_baseline",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "analyze schema should contain property '{prop}'"
        );
    }
}

#[test]
fn check_changed_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "check_changed").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "since",
        "config",
        "production",
        "workspace",
        "baseline",
        "save_baseline",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "check_changed schema should contain property '{prop}'"
        );
    }
}

#[test]
fn check_changed_schema_requires_since() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "check_changed").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    // "since" should appear in the "required" array
    assert!(
        schema.contains("\"required\""),
        "check_changed schema should have a required array"
    );
    // The required field should reference "since"
    let schema_value: serde_json::Value = serde_json::from_str(&schema).unwrap();
    if let Some(required) = schema_value.get("required").and_then(|r| r.as_array()) {
        assert!(
            required.iter().any(|v| v.as_str() == Some("since")),
            "check_changed schema should require 'since'"
        );
    }
}

#[test]
fn find_dupes_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "find_dupes").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "workspace",
        "mode",
        "min_tokens",
        "min_lines",
        "threshold",
        "skip_local",
        "cross_language",
        "top",
        "baseline",
        "save_baseline",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "find_dupes schema should contain property '{prop}'"
        );
    }
}

#[test]
fn fix_preview_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "fix_preview").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "production",
        "workspace",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "fix_preview schema should contain property '{prop}'"
        );
    }
}

#[test]
fn fix_apply_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "fix_apply").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "production",
        "workspace",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "fix_apply schema should contain property '{prop}'"
        );
    }
}

#[test]
fn project_info_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "project_info").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in ["root", "config", "no_cache", "threads"] {
        assert!(
            schema.contains(prop),
            "project_info schema should contain property '{prop}'"
        );
    }
}

#[test]
fn check_health_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "check_health").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "max_cyclomatic",
        "max_cognitive",
        "top",
        "sort",
        "changed_since",
        "complexity",
        "file_scores",
        "hotspots",
        "targets",
        "since",
        "min_commits",
        "workspace",
        "production",
        "save_snapshot",
        "baseline",
        "save_baseline",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "check_health schema should contain property '{prop}'"
        );
    }
}

#[test]
fn audit_schema_contains_expected_properties() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "audit").unwrap();
    let schema = serde_json::to_string(&tool.input_schema).unwrap();
    for prop in [
        "root",
        "config",
        "base",
        "production",
        "workspace",
        "no_cache",
        "threads",
    ] {
        assert!(
            schema.contains(prop),
            "audit schema should contain property '{prop}'"
        );
    }
}

// ── fix_apply is not open_world ───────────────────────────────────

#[test]
fn fix_apply_does_not_have_open_world_hint() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let fix = tools.iter().find(|t| t.name == "fix_apply").unwrap();
    let ann = fix.annotations.as_ref().unwrap();
    // fix_apply is destructive, should not have open_world_hint=true
    assert_ne!(
        ann.open_world_hint,
        Some(true),
        "fix_apply should not have open_world_hint=true"
    );
}

// ── Tool descriptions mention key behaviors ───────────────────────

#[test]
fn analyze_description_mentions_unused_code() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "analyze").unwrap();
    let desc = tool.description.as_deref().unwrap();
    assert!(
        desc.contains("unused"),
        "analyze description should mention 'unused'"
    );
}

#[test]
fn find_dupes_description_mentions_duplication() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "find_dupes").unwrap();
    let desc = tool.description.as_deref().unwrap();
    assert!(
        desc.contains("duplic"),
        "find_dupes description should mention duplication"
    );
}

#[test]
fn check_health_description_mentions_complexity() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "check_health").unwrap();
    let desc = tool.description.as_deref().unwrap();
    assert!(
        desc.contains("complexity"),
        "check_health description should mention 'complexity'"
    );
}

#[test]
fn fix_apply_description_warns_about_modification() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "fix_apply").unwrap();
    let desc = tool.description.as_deref().unwrap();
    assert!(
        desc.contains("modif") || desc.contains("disk") || desc.contains("destructi"),
        "fix_apply description should warn about file modification"
    );
}

#[test]
fn fix_preview_description_mentions_dry_run_or_preview() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    let tool = tools.iter().find(|t| t.name == "fix_preview").unwrap();
    let desc = tool.description.as_deref().unwrap();
    assert!(
        desc.contains("preview") || desc.contains("dry") || desc.contains("without modif"),
        "fix_preview description should mention preview/dry-run behavior"
    );
}

// ── All schemas are valid JSON objects ─────────────────────────────

#[test]
fn all_tool_schemas_are_json_objects() {
    let server = FallowMcp::new();
    let tools = server.tool_router.list_all();
    for tool in &tools {
        let name = tool.name.to_string();
        let schema_str = serde_json::to_string(&tool.input_schema).unwrap();
        let schema_value: serde_json::Value = serde_json::from_str(&schema_str).unwrap();
        assert!(
            schema_value.is_object(),
            "tool '{name}' schema should be a JSON object"
        );
        // Should have "type": "object" at the top level
        assert_eq!(
            schema_value.get("type").and_then(|t| t.as_str()),
            Some("object"),
            "tool '{name}' schema should have type=object"
        );
    }
}

// ── Server can be cloned (required for rmcp runtime) ───────────────

#[test]
fn server_is_cloneable() {
    let server = FallowMcp::new();
    // Use clone in a way that isn't redundant — verify both instances work
    let cloned = server.clone();
    let tools_original = server.tool_router.list_all();
    let tools_cloned = cloned.tool_router.list_all();
    assert_eq!(tools_original.len(), tools_cloned.len());
}
